use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ffi::{CStr, c_char, c_void};

use defmt::info;
use elf_loader::{
    Loader, Relocator,
    image::{SyntheticModule, SyntheticSymbol},
    input::ElfBinary,
};
#[cfg(feature = "perf-diagnostics")]
use embassy_time::Instant;
use heapless::spsc::{Consumer, Producer};
use picolv2_image_format::{Bundle, FLASH_ADDRESS, MAX_SIZE, PluginMetadata, PortKind};

use crate::audio_buffer::{
    AudioBlockIndex, BLOCK_SIZE, MIDI_SCHEDULING_DELAY_BLOCKS, SAMPLE_RATE, block_mut_ptr,
};
#[cfg(feature = "perf-diagnostics")]
use crate::audio_buffer::REPORT_BLOCKS;
use crate::host_hooks::HOST_SYMBOLS;
use crate::log_heap;
use crate::lv2::{
    ATOM_SEQUENCE_URI, ATOM_SEQUENCE_URID, Lv2Descriptor, Lv2Feature, Lv2UridMap, MIDI_EVENT_URI,
    MIDI_EVENT_URID, URID_MAP_URI,
};
use crate::midi::{Lv2MidiSequence, MidiEvent};

static mut MIDI_SEQUENCE: Lv2MidiSequence = Lv2MidiSequence::empty();

extern "C" fn map_uri(_handle: *mut c_void, uri: *const c_char) -> u32 {
    if uri.is_null() {
        return 0;
    }

    let uri = unsafe { CStr::from_ptr(uri) }.to_bytes_with_nul();
    if uri == ATOM_SEQUENCE_URI {
        ATOM_SEQUENCE_URID
    } else if uri == MIDI_EVENT_URI {
        MIDI_EVENT_URID
    } else {
        0
    }
}

static mut URID_MAP: Lv2UridMap = Lv2UridMap {
    handle: core::ptr::null_mut(),
    map: map_uri,
};
static mut URID_MAP_FEATURE: Lv2Feature = Lv2Feature {
    uri: URID_MAP_URI.as_ptr() as *const c_char,
    data: core::ptr::addr_of_mut!(URID_MAP) as *mut c_void,
};
static mut FEATURES: [*const Lv2Feature; 2] =
    [core::ptr::addr_of!(URID_MAP_FEATURE), core::ptr::null()];

/// Represents a loaded, relocated LV2 plugin library binary.
pub struct PluginBinary {
    descriptor: &'static Lv2Descriptor,
}

impl PluginBinary {
    pub fn load(name: &str, elf_bytes: &[u8]) -> Self {
        info!(
            "plugin load begin name={} elf_bytes={}",
            name,
            elf_bytes.len()
        );
        log_heap("before elf load");
        let raw = Loader::new()
            .run()
            .load_dylib(ElfBinary::new(name, elf_bytes))
            .expect("failed to load lv2 plugin binary");
        log_heap("after elf load");
        // `plugins/pico-alloc.c` leaves these symbols undefined so plugin heap
        // allocations and stdout writes are served by the firmware (see `host_hooks.rs`)
        let host = SyntheticModule::new(
            "picolv2-host",
            HOST_SYMBOLS.map(|(name, address)| SyntheticSymbol::function(name, address)),
        );
        let lib = Relocator::new()
            .run(raw)
            .modules([host])
            .relocate()
            .expect("failed to relocate lv2 plugin binary");
        lib.initialize()
            .expect("failed to initialize lv2 plugin binary");
        log_heap("after relocation");

        let lv2_descriptor = unsafe {
            lib.get::<extern "C" fn(u32) -> *const Lv2Descriptor>("lv2_descriptor")
                .expect("symbol `lv2_descriptor` not found")
        };
        let descriptor: &'static Lv2Descriptor = unsafe { &*lv2_descriptor(0) };

        // Keep the relocated ELF resident in memory for the lifetime of the firmware
        core::mem::forget(lib);

        Self { descriptor }
    }

    pub fn instantiate(
        &self,
        sample_rate: f64,
        features: *const *const Lv2Feature,
    ) -> PluginInstance {
        let handle = (self.descriptor.instantiate)(
            self.descriptor,
            sample_rate,
            core::ptr::null(),
            features,
        );
        assert!(!handle.is_null(), "failed to instantiate lv2 plugin");

        PluginInstance {
            descriptor: self.descriptor,
            handle,
        }
    }
}

/// An active instance of a loaded LV2 plugin.
pub struct PluginInstance {
    descriptor: &'static Lv2Descriptor,
    handle: *mut c_void,
}

// Bridges a block-rate control output to an audio-rate CV input (e.g.
// picolv2's Note plugin exposes gate/trigger as ControlPort, while ams-lv2's
// env expects them as CVPort); the source value is broadcast across the
// destination's whole block every render cycle.
struct ControlToCvBridge {
    source_node: usize,
    source: *const f32,
    destination: *mut [f32; BLOCK_SIZE],
}

struct PluginNode {
    instance: PluginInstance,
    // Kept only to own the connected buffers for the node's lifetime; the
    // plugin reads/writes them through the pointers handed to `connect_port`.
    // Any audio port with no incoming/outgoing edge just stays at its
    // initial value (silence), rather than being left as a null pointer.
    #[allow(dead_code)]
    audio_inputs: Vec<(u32, Box<[f32; BLOCK_SIZE]>)>,
    audio_outputs: Vec<(u32, Box<[f32; BLOCK_SIZE]>)>,
    control_inputs: Vec<(u32, Box<f32>)>,
    control_outputs: Vec<(u32, Box<f32>)>,
    cv_inputs: Vec<(u32, Box<[f32; BLOCK_SIZE]>)>,
    cv_outputs: Vec<(u32, Box<[f32; BLOCK_SIZE]>)>,
}

impl PluginInstance {
    pub fn connect_port(&mut self, port: u32, data_location: *mut c_void) {
        (self.descriptor.connect_port)(self.handle, port, data_location);
    }

    pub fn activate(&mut self) {
        (self.descriptor.activate)(self.handle);
    }

    pub fn run(&mut self, sample_count: u32) {
        (self.descriptor.run)(self.handle, sample_count);
    }

    #[allow(dead_code)]
    pub fn deactivate(&mut self) {
        (self.descriptor.deactivate)(self.handle);
    }
}

/// Manages loaded LV2 plugin instances and bridges queued MIDI events
/// through the audio processing pipeline.
pub struct PluginHost {
    nodes: Vec<PluginNode>,
    output_node: usize,
    control_to_cv_bridges: Vec<ControlToCvBridge>,
    midi_consumer: Consumer<'static, MidiEvent>,
    pending_midi: Option<MidiEvent>,
    timeline_origin_micros: u64,
    block_start_frame: u64,
    // Diagnostics: per-node total render time, reported and reset every
    // REPORT_BLOCKS blocks to identify which plugin(s) in the graph are
    // expensive.
    #[cfg(feature = "perf-diagnostics")]
    node_micros: Vec<u64>,
    #[cfg(feature = "perf-diagnostics")]
    report_block_count: u32,
}

impl PluginHost {
    pub fn load(midi_consumer: Consumer<'static, MidiEvent>) -> Self {
        let bundle_bytes =
            unsafe { core::slice::from_raw_parts(FLASH_ADDRESS as *const u8, MAX_SIZE) };
        let bundle = Bundle::parse(bundle_bytes).expect("invalid plugin bundle");
        let graph = bundle.graph().expect("invalid plugin graph");

        let features_ptr = core::ptr::addr_of!(FEATURES) as *const *const Lv2Feature;

        // Relocated binaries are shared across graph nodes that reference the
        // same plugin URI, so repeated nodes only cost another instance, not
        // another ELF load/relocation and RAM mapping.
        let mut binaries: Vec<(&[u8], PluginBinary)> = Vec::new();
        let mut nodes = Vec::new();
        for node_index in 0..graph.node_count {
            let graph_node = graph.node(node_index).expect("invalid graph node");
            let node_uri = graph_node.uri;
            let entry = bundle
                .find(node_uri)
                .expect("graph plugin missing from bundle");
            let binary = if let Some(pos) = binaries.iter().position(|(uri, _)| *uri == node_uri) {
                info!(
                    "graph node {} uri={} reusing loaded binary",
                    node_index, node_uri
                );
                &binaries[pos].1
            } else {
                info!(
                    "graph node {} uri={} binary_bytes={} metadata_bytes={}",
                    node_index,
                    node_uri,
                    entry.binary.len(),
                    entry.metadata.len()
                );
                let binary = PluginBinary::load("graph-plugin.so", entry.binary);
                binaries.push((node_uri, binary));
                &binaries.last().expect("just pushed").1
            };
            let metadata = PluginMetadata::parse(entry.metadata).expect("invalid graph metadata");
            let mut instance = binary.instantiate(SAMPLE_RATE as f64, features_ptr);
            log_heap("after instantiate");
            if let Some(port) = metadata.port(PortKind::AtomInput, 0) {
                instance.connect_port(
                    port.index,
                    core::ptr::addr_of_mut!(MIDI_SEQUENCE) as *mut c_void,
                );
            }
            let mut audio_input_index = 0;
            let mut audio_inputs = Vec::new();
            while let Some(port) = metadata.port(PortKind::AudioInput, audio_input_index) {
                let mut buffer = Box::new([0.0f32; BLOCK_SIZE]);
                instance.connect_port(port.index, buffer.as_mut_ptr() as *mut c_void);
                audio_inputs.push((port.index, buffer));
                audio_input_index += 1;
            }
            let mut audio_output_index = 0;
            let mut audio_outputs = Vec::new();
            while let Some(port) = metadata.port(PortKind::AudioOutput, audio_output_index) {
                let mut buffer = Box::new([0.0f32; BLOCK_SIZE]);
                instance.connect_port(port.index, buffer.as_mut_ptr() as *mut c_void);
                audio_outputs.push((port.index, buffer));
                audio_output_index += 1;
            }
            let mut control_index = 0;
            let mut control_inputs = Vec::new();
            while let Some(port) = metadata.port(PortKind::ControlInput, control_index) {
                let value = graph_node
                    .override_value(port.index as u8)
                    .unwrap_or_else(|| port.default.unwrap_or(0.0));
                let mut control = Box::new(value);
                instance.connect_port(port.index, control.as_mut() as *mut f32 as *mut c_void);
                control_inputs.push((port.index, control));
                control_index += 1;
            }
            let mut control_index = 0;
            let mut control_outputs = Vec::new();
            while let Some(port) = metadata.port(PortKind::ControlOutput, control_index) {
                let mut control = Box::new(0.0f32);
                instance.connect_port(port.index, control.as_mut() as *mut f32 as *mut c_void);
                control_outputs.push((port.index, control));
                control_index += 1;
            }
            let mut cv_index = 0;
            let mut cv_inputs = Vec::new();
            while let Some(port) = metadata.port(PortKind::CvInput, cv_index) {
                let value = graph_node
                    .override_value(port.index as u8)
                    .unwrap_or_else(|| port.default.unwrap_or(0.0));
                let mut buffer = Box::new([value; BLOCK_SIZE]);
                instance.connect_port(port.index, buffer.as_mut_ptr() as *mut c_void);
                cv_inputs.push((port.index, buffer));
                cv_index += 1;
            }
            let mut cv_index = 0;
            let mut cv_outputs = Vec::new();
            while let Some(port) = metadata.port(PortKind::CvOutput, cv_index) {
                let mut buffer = Box::new([0.0f32; BLOCK_SIZE]);
                instance.connect_port(port.index, buffer.as_mut_ptr() as *mut c_void);
                cv_outputs.push((port.index, buffer));
                cv_index += 1;
            }
            instance.activate();
            nodes.push(PluginNode {
                instance,
                audio_inputs,
                audio_outputs,
                control_inputs,
                control_outputs,
                cv_inputs,
                cv_outputs,
            });
        }
        let mut control_to_cv_bridges = Vec::new();
        for edge_index in 0..graph.edge_count {
            let edge = graph.edge(edge_index).expect("invalid graph edge");
            assert!(
                (edge.source_node as usize) < nodes.len()
                    && (edge.destination_node as usize) < nodes.len(),
                "graph node reference out of range"
            );
            assert!(
                edge.source_node < edge.destination_node,
                "graph nodes must be topologically ordered"
            );
            let source_index = edge.source_node as usize;
            let destination_index = edge.destination_node as usize;
            let source_metadata = PluginMetadata::parse(
                bundle
                    .find(
                        graph
                            .node(edge.source_node)
                            .expect("invalid graph node")
                            .uri,
                    )
                    .expect("graph plugin missing from bundle")
                    .metadata,
            )
            .expect("invalid graph metadata");
            let destination_metadata = PluginMetadata::parse(
                bundle
                    .find(
                        graph
                            .node(edge.destination_node)
                            .expect("invalid graph node")
                            .uri,
                    )
                    .expect("graph plugin missing from bundle")
                    .metadata,
            )
            .expect("invalid graph metadata");
            match (
                source_metadata.port_by_index(edge.source_port as u32),
                destination_metadata.port_by_index(edge.destination_port as u32),
            ) {
                (Some(source), Some(destination))
                    if source.kind == PortKind::AudioOutput
                        && destination.kind == PortKind::AudioInput =>
                {
                    let buffer = nodes[source_index]
                        .audio_outputs
                        .iter()
                        .find(|(port, _)| *port == source.index)
                        .map(|(_, buffer)| buffer.as_ptr() as *mut c_void)
                        .expect("graph source audio port is not connected");
                    assert!(
                        nodes[destination_index]
                            .audio_inputs
                            .iter()
                            .any(|(port, _)| *port == destination.index),
                        "graph destination audio port is not connected"
                    );
                    nodes[destination_index]
                        .instance
                        .connect_port(destination.index, buffer);
                }
                (Some(source), Some(destination))
                    if source.kind == PortKind::ControlOutput
                        && destination.kind == PortKind::ControlInput =>
                {
                    let control = nodes[source_index]
                        .control_outputs
                        .iter()
                        .find(|(port, _)| *port == source.index)
                        .map(|(_, control)| control.as_ref() as *const f32 as *mut f32)
                        .expect("graph source control port is not connected");
                    assert!(
                        nodes[destination_index]
                            .control_inputs
                            .iter()
                            .any(|(port, _)| *port == destination.index),
                        "graph destination control port is not connected"
                    );
                    nodes[destination_index]
                        .instance
                        .connect_port(destination.index, control as *mut c_void);
                }
                (Some(source), Some(destination))
                    if source.kind == PortKind::CvOutput
                        && destination.kind == PortKind::CvInput =>
                {
                    let buffer = nodes[source_index]
                        .cv_outputs
                        .iter()
                        .find(|(port, _)| *port == source.index)
                        .map(|(_, buffer)| buffer.as_ref().as_ptr() as *mut f32)
                        .expect("graph source cv port is not connected");
                    assert!(
                        nodes[destination_index]
                            .cv_inputs
                            .iter()
                            .any(|(port, _)| *port == destination.index),
                        "graph destination cv port is not connected"
                    );
                    nodes[destination_index]
                        .instance
                        .connect_port(destination.index, buffer as *mut c_void);
                }
                (Some(source), Some(destination))
                    if source.kind == PortKind::ControlOutput
                        && destination.kind == PortKind::CvInput =>
                {
                    let source_ptr = nodes[source_index]
                        .control_outputs
                        .iter()
                        .find(|(port, _)| *port == source.index)
                        .map(|(_, control)| control.as_ref() as *const f32)
                        .expect("graph source control port is not connected");
                    let destination_ptr = nodes[destination_index]
                        .cv_inputs
                        .iter_mut()
                        .find(|(port, _)| *port == destination.index)
                        .map(|(_, buffer)| buffer.as_mut() as *mut [f32; BLOCK_SIZE])
                        .expect("graph destination cv port is not connected");
                    control_to_cv_bridges.push(ControlToCvBridge {
                        source_node: source_index,
                        source: source_ptr,
                        destination: destination_ptr,
                    });
                }
                _ => panic!("graph edge connects incompatible ports"),
            }
        }
        let mut has_outgoing = alloc::vec![false; nodes.len()];
        for edge_index in 0..graph.edge_count {
            has_outgoing[graph.edge(edge_index).unwrap().source_node as usize] = true;
        }
        let output_node = (0..nodes.len())
            .rev()
            .find(|index| !has_outgoing[*index])
            .expect("graph has no output");

        #[cfg(feature = "perf-diagnostics")]
        let node_micros = alloc::vec![0u64; nodes.len()];
        Self {
            nodes,
            output_node,
            control_to_cv_bridges,
            midi_consumer,
            pending_midi: None,
            timeline_origin_micros: embassy_time::Instant::now().as_micros(),
            block_start_frame: 0,
            #[cfg(feature = "perf-diagnostics")]
            node_micros,
            #[cfg(feature = "perf-diagnostics")]
            report_block_count: 0,
        }
    }

    unsafe fn process(&mut self, output: *mut f32) {
        let midi_sequence = unsafe { &mut *core::ptr::addr_of_mut!(MIDI_SEQUENCE) };
        let mut event_count = 0;
        while event_count < midi_sequence.events.len() {
            let Some(event) = self
                .pending_midi
                .take()
                .or_else(|| self.midi_consumer.dequeue())
            else {
                break;
            };

            // Schedule by reception time plus the maximum render-ahead depth:
            // static float-pool blocks plus packed I2S DMA buffers.
            let received_micros = event
                .timestamp_micros
                .saturating_sub(self.timeline_origin_micros);
            let received_frame = received_micros.saturating_mul(SAMPLE_RATE as u64) / 1_000_000;
            let target_frame = received_frame
                .saturating_add(MIDI_SCHEDULING_DELAY_BLOCKS as u64 * BLOCK_SIZE as u64);
            let block_end_frame = self.block_start_frame + BLOCK_SIZE as u64;
            if target_frame >= block_end_frame {
                self.pending_midi = Some(event);
                break;
            }

            midi_sequence.events[event_count].frame =
                target_frame.saturating_sub(self.block_start_frame) as i64;
            midi_sequence.events[event_count].message = [event.status, event.data1, event.data2];
            event_count += 1;
        }
        midi_sequence.set_event_count(event_count);

        for (index, node) in self.nodes.iter_mut().enumerate() {
            #[cfg(feature = "perf-diagnostics")]
            let node_start = Instant::now();
            node.instance.run(BLOCK_SIZE as u32);
            #[cfg(feature = "perf-diagnostics")]
            {
                self.node_micros[index] += node_start.elapsed().as_micros();
            }
            for bridge in &self.control_to_cv_bridges {
                if bridge.source_node != index {
                    continue;
                }
                let value = unsafe { *bridge.source };
                unsafe { (*bridge.destination).fill(value) };
            }
        }
        unsafe {
            core::ptr::copy_nonoverlapping(
                self.nodes[self.output_node]
                    .audio_outputs
                    .first()
                    .expect("output node has no audio output port")
                    .1
                    .as_ptr(),
                output,
                BLOCK_SIZE,
            );
        }

        self.block_start_frame += BLOCK_SIZE as u64;

        #[cfg(feature = "perf-diagnostics")]
        {
            self.report_block_count += 1;
            if self.report_block_count >= REPORT_BLOCKS {
                for (index, micros) in self.node_micros.iter_mut().enumerate() {
                    info!(
                        "plugin node {} total={}us over {} blocks",
                        index, *micros, self.report_block_count
                    );
                    *micros = 0;
                }
                self.report_block_count = 0;
            }
        }
    }
}

#[embassy_executor::task]
pub async fn plugin_host_task(
    midi_consumer: Consumer<'static, MidiEvent>,
    mut free_consumer: Consumer<'static, AudioBlockIndex>,
    mut ready_producer: Producer<'static, AudioBlockIndex>,
) -> ! {
    info!("Starting LV2 plugin host task");
    let mut plugin = PluginHost::load(midi_consumer);

    // Diagnostics: a block must render in BUDGET_MICROS to keep up with
    // real time. Report max render time and overrun count roughly once a
    // second so CPU-bound choppiness (vs. e.g. midi timing) can be confirmed.
    #[cfg(feature = "perf-diagnostics")]
    const BUDGET_MICROS: u64 = (BLOCK_SIZE as u64 * 1_000_000) / SAMPLE_RATE as u64;
    #[cfg(feature = "perf-diagnostics")]
    let mut block_count: u32 = 0;
    #[cfg(feature = "perf-diagnostics")]
    let mut max_micros: u64 = 0;
    #[cfg(feature = "perf-diagnostics")]
    let mut overrun_count: u32 = 0;

    loop {
        let index = loop {
            if let Some(index) = free_consumer.dequeue() {
                break index;
            }
            embassy_futures::yield_now().await;
        };

        #[cfg(feature = "perf-diagnostics")]
        let start = Instant::now();
        unsafe { plugin.process(block_mut_ptr(index)) };
        #[cfg(feature = "perf-diagnostics")]
        {
            let elapsed_micros = start.elapsed().as_micros();
            if elapsed_micros > BUDGET_MICROS {
                overrun_count += 1;
            }
            if elapsed_micros > max_micros {
                max_micros = elapsed_micros;
            }
            block_count += 1;
            if block_count >= REPORT_BLOCKS {
                info!(
                    "plugin process: budget={}us max={}us overruns={}/{}",
                    BUDGET_MICROS, max_micros, overrun_count, block_count
                );
                block_count = 0;
                max_micros = 0;
                overrun_count = 0;
            }
        }

        while ready_producer.enqueue(index).is_err() {
            embassy_futures::yield_now().await;
        }
    }
}
