use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ffi::{CStr, c_char, c_void};
use core::mem::size_of;

use defmt::{debug, info};
use elf_loader::{
    Loader, Relocator,
    image::{SyntheticModule, SyntheticSymbol},
    input::ElfBinary,
};
#[cfg(feature = "perf-diagnostics")]
use embassy_time::Instant;
use heapless::spsc::{Consumer, Producer};
use picolv2_image_format::{
    Bundle, FLASH_ADDRESS, MAX_SIZE, MIDI_BINDING_INTEGER, MIDI_BINDING_LOGARITHMIC,
    MIDI_BINDING_TOGGLED, MIDI_BINDING_TRIGGER, PluginMetadata, PortKind,
};

#[cfg(feature = "perf-diagnostics")]
use crate::audio_buffer::REPORT_BLOCKS;
use crate::audio_buffer::{
    AudioBlockIndex, BLOCK_SIZE, MIDI_SCHEDULING_DELAY_BLOCKS, SAMPLE_RATE, block_mut_ptr,
};
use crate::host_hooks::HOST_SYMBOLS;
use crate::log_heap;
use crate::lv2::{
    ATOM_BLANK_URI, ATOM_BLANK_URID, ATOM_DOUBLE_URI, ATOM_DOUBLE_URID, ATOM_FLOAT_URI,
    ATOM_FLOAT_URID, ATOM_INT_URI, ATOM_INT_URID, ATOM_LONG_URI, ATOM_LONG_URID, ATOM_OBJECT_URI,
    ATOM_OBJECT_URID, ATOM_SEQUENCE_URI, ATOM_SEQUENCE_URID, Lv2Descriptor, Lv2Feature, Lv2UridMap,
    MIDI_EVENT_URI, MIDI_EVENT_URID, TIME_BAR_BEAT_URI, TIME_BAR_BEAT_URID, TIME_BAR_URI,
    TIME_BAR_URID, TIME_BEAT_UNIT_URI, TIME_BEAT_UNIT_URID, TIME_BEATS_PER_BAR_URI,
    TIME_BEATS_PER_BAR_URID, TIME_BEATS_PER_MINUTE_URI, TIME_BEATS_PER_MINUTE_URID, TIME_FRAME_URI,
    TIME_FRAME_URID, TIME_POSITION_URI, TIME_POSITION_URID, TIME_SPEED_URI, TIME_SPEED_URID,
    URID_MAP_URI,
};
use crate::midi::{Lv2AtomSequence, Lv2AtomSequenceBody, MidiEvent};

const TRANSPORT_BPM: f32 = 120.0;
static mut MIDI_SEQUENCE: Lv2AtomSequence = Lv2AtomSequence::empty();

extern "C" fn map_uri(_handle: *mut c_void, uri: *const c_char) -> u32 {
    if uri.is_null() {
        return 0;
    }

    let uri = unsafe { CStr::from_ptr(uri) }.to_bytes_with_nul();
    match uri {
        ATOM_SEQUENCE_URI => ATOM_SEQUENCE_URID,
        MIDI_EVENT_URI => MIDI_EVENT_URID,
        ATOM_BLANK_URI => ATOM_BLANK_URID,
        ATOM_OBJECT_URI => ATOM_OBJECT_URID,
        ATOM_DOUBLE_URI => ATOM_DOUBLE_URID,
        ATOM_FLOAT_URI => ATOM_FLOAT_URID,
        ATOM_INT_URI => ATOM_INT_URID,
        ATOM_LONG_URI => ATOM_LONG_URID,
        TIME_POSITION_URI => TIME_POSITION_URID,
        TIME_BAR_URI => TIME_BAR_URID,
        TIME_BAR_BEAT_URI => TIME_BAR_BEAT_URID,
        TIME_BEATS_PER_BAR_URI => TIME_BEATS_PER_BAR_URID,
        TIME_BEATS_PER_MINUTE_URI => TIME_BEATS_PER_MINUTE_URID,
        TIME_BEAT_UNIT_URI => TIME_BEAT_UNIT_URID,
        TIME_FRAME_URI => TIME_FRAME_URID,
        TIME_SPEED_URI => TIME_SPEED_URID,
        _ => 0,
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
    pub fn load(name: &str, elf_bytes: &[u8], plugin_uri: &[u8]) -> Self {
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
        let mut descriptor_index = 0;
        let descriptor: &'static Lv2Descriptor = loop {
            let candidate = lv2_descriptor(descriptor_index);
            assert!(
                !candidate.is_null(),
                "plugin binary does not contain the requested LV2 descriptor"
            );
            let candidate = unsafe { &*candidate };
            assert!(!candidate.uri.is_null(), "LV2 descriptor has a null URI");
            if unsafe { CStr::from_ptr(candidate.uri) }.to_bytes() == plugin_uri {
                break candidate;
            }
            descriptor_index += 1;
        };
        info!("selected LV2 descriptor index={}", descriptor_index);

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

struct MidiControlBinding {
    channel: u8,
    controller: u8,
    flags: u8,
    minimum: f32,
    maximum: f32,
    target: *mut f32,
}

impl MidiControlBinding {
    fn apply(&self, event: MidiEvent) {
        if event.status & 0xf0 != 0xb0
            || event.status & 0x0f != self.channel
            || event.data1 != self.controller
        {
            return;
        }
        let mut value = if self.flags & MIDI_BINDING_TRIGGER != 0 {
            self.maximum
        } else if self.flags & MIDI_BINDING_TOGGLED != 0 {
            if event.data2 >= 64 {
                self.maximum
            } else {
                self.minimum
            }
        } else if event.data2 == 0 {
            self.minimum
        } else if event.data2 == 127 {
            self.maximum
        } else {
            let normalized = f32::from(event.data2) / 127.0;
            if self.flags & MIDI_BINDING_LOGARITHMIC != 0 {
                self.minimum * libm::powf(self.maximum / self.minimum, normalized)
            } else {
                self.minimum + (self.maximum - self.minimum) * normalized
            }
        };
        if self.flags & MIDI_BINDING_INTEGER != 0 {
            value = libm::roundf(value);
        }
        unsafe { *self.target = value };
    }

    fn reset_trigger(&self) {
        if self.flags & MIDI_BINDING_TRIGGER != 0 {
            unsafe { *self.target = self.minimum };
        }
    }
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
    // Atom output buffers are shared directly with downstream Atom inputs.
    atom_outputs: Vec<(u32, Box<Lv2AtomSequence>)>,
}

impl PluginInstance {
    pub fn connect_port(&mut self, port: u32, data_location: *mut c_void) {
        (self.descriptor.connect_port)(self.handle, port, data_location);
    }

    pub fn activate(&mut self) {
        if let Some(activate) = self.descriptor.activate {
            activate(self.handle);
        }
    }

    pub fn run(&mut self, sample_count: u32) {
        (self.descriptor.run)(self.handle, sample_count);
    }

    #[allow(dead_code)]
    pub fn deactivate(&mut self) {
        if let Some(deactivate) = self.descriptor.deactivate {
            deactivate(self.handle);
        }
    }
}

/// Manages loaded LV2 plugin instances and bridges queued MIDI events
/// through the audio processing pipeline.
pub struct PluginHost {
    nodes: Vec<PluginNode>,
    // (node index, lv2 port index) of the graph's left/mono and right audio
    // outputs, resolved from the graph's declared output ports.
    left_output: (usize, u32),
    right_output: (usize, u32),
    control_to_cv_bridges: Vec<ControlToCvBridge>,
    midi_control_bindings: Vec<MidiControlBinding>,
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
                    node_index,
                    core::str::from_utf8(node_uri).unwrap_or("<invalid utf8>")
                );
                &binaries[pos].1
            } else {
                info!(
                    "graph node {} uri={} binary_bytes={} metadata_bytes={}",
                    node_index,
                    core::str::from_utf8(node_uri).unwrap_or("<invalid utf8>"),
                    entry.binary.len(),
                    entry.metadata.len()
                );
                let binary = PluginBinary::load("graph-plugin.so", entry.binary, node_uri);
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
            let mut atom_index = 0;
            let mut atom_outputs = Vec::new();
            while let Some(port) = metadata.port(PortKind::AtomOutput, atom_index) {
                let mut sequence = Box::new(Lv2AtomSequence::empty());
                sequence.set_capacity();
                instance.connect_port(port.index, sequence.as_mut() as *mut _ as *mut c_void);
                atom_outputs.push((port.index, sequence));
                atom_index += 1;
            }
            instance.activate();
            info!(
                "graph node {} ready audio_in={} audio_out={} control_in={} control_out={} cv_in={} cv_out={} atom_out={}",
                node_index,
                audio_inputs.len(),
                audio_outputs.len(),
                control_inputs.len(),
                control_outputs.len(),
                cv_inputs.len(),
                cv_outputs.len(),
                atom_outputs.len()
            );
            log_heap("after node setup");
            nodes.push(PluginNode {
                instance,
                audio_inputs,
                audio_outputs,
                control_inputs,
                control_outputs,
                cv_inputs,
                cv_outputs,
                atom_outputs,
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
                    if source.kind == PortKind::AtomOutput
                        && destination.kind == PortKind::AtomInput =>
                {
                    let sequence = nodes[source_index]
                        .atom_outputs
                        .iter_mut()
                        .find(|(port, _)| *port == source.index)
                        .map(|(_, sequence)| sequence.as_mut() as *mut _ as *mut c_void)
                        .expect("graph source atom port is not connected");
                    nodes[destination_index]
                        .instance
                        .connect_port(destination.index, sequence);
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
        let mut midi_control_bindings = Vec::new();
        for binding_index in 0..graph.midi_binding_count {
            let binding = graph
                .midi_binding(binding_index)
                .expect("invalid MIDI binding");
            assert!(binding.channel < 16, "MIDI binding channel out of range");
            assert!(
                binding.controller < 128,
                "MIDI binding controller out of range"
            );
            let node = nodes
                .get_mut(binding.node as usize)
                .expect("MIDI binding node out of range");
            let target = node
                .control_inputs
                .iter_mut()
                .find(|(port, _)| *port == binding.port as u32)
                .map(|(_, control)| control.as_mut() as *mut f32)
                .expect("MIDI binding target is not a control input");
            midi_control_bindings.push(MidiControlBinding {
                channel: binding.channel,
                controller: binding.controller,
                flags: binding.flags,
                minimum: binding.minimum,
                maximum: binding.maximum,
                target,
            });
        }
        info!(
            "plugin graph ready nodes={} edges={} midi_bindings={}",
            graph.node_count, graph.edge_count, graph.midi_binding_count
        );
        // The graph's declared output ports (e.g. Ingen's `audio_out_1`/
        // `audio_out_2`) tell us exactly which node/port feeds the left and
        // right channels, rather than guessing from the node topology.
        let output = |output_index: u16| -> (usize, u32) {
            let output = graph.output(output_index).expect("invalid graph output");
            (output.node as usize, output.port as u32)
        };
        assert!(graph.output_count > 0, "graph has no audio output");
        let left_output = output(0);
        let right_output = if graph.output_count > 1 {
            output(1)
        } else {
            left_output
        };

        #[cfg(feature = "perf-diagnostics")]
        let node_micros = alloc::vec![0u64; nodes.len()];
        Self {
            nodes,
            left_output,
            right_output,
            control_to_cv_bridges,
            midi_control_bindings,
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
        midi_sequence.clear();
        assert!(
            midi_sequence.push_position(self.block_start_frame, SAMPLE_RATE, TRANSPORT_BPM),
            "LV2 atom input buffer too small for transport position"
        );
        let mut event_count = 0;
        while event_count < crate::midi::MIDI_BLOCK_CAPACITY {
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

            if !midi_sequence.push_midi(
                target_frame.saturating_sub(self.block_start_frame) as i64,
                [event.status, event.data1, event.data2],
            ) {
                self.pending_midi = Some(event);
                break;
            }
            for binding in &self.midi_control_bindings {
                binding.apply(event);
            }
            event_count += 1;
        }
        if event_count > 0 {
            debug!("MIDI input block events={}", event_count);
        }

        for (index, node) in self.nodes.iter_mut().enumerate() {
            // LV2 Atom outputs receive writable capacity in atom.size; the
            // plugin replaces it with the emitted sequence size during run().
            for (_, sequence) in &mut node.atom_outputs {
                sequence.set_capacity();
            }
            #[cfg(feature = "perf-diagnostics")]
            let node_start = Instant::now();
            node.instance.run(BLOCK_SIZE as u32);
            for (port, sequence) in &node.atom_outputs {
                if sequence.atom.size > size_of::<Lv2AtomSequenceBody>() as u32 {
                    debug!(
                        "MIDI output node={} port={} bytes={}",
                        index, port, sequence.atom.size
                    );
                }
            }
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
        for binding in &self.midi_control_bindings {
            binding.reset_trigger();
        }
        let (left_node, left_port) = self.left_output;
        let (right_node, right_port) = self.right_output;
        let left = self.nodes[left_node]
            .audio_outputs
            .iter()
            .find(|(port, _)| *port == left_port)
            .map(|(_, buffer)| buffer.as_ref())
            .expect("left output port is not connected");
        let right = self.nodes[right_node]
            .audio_outputs
            .iter()
            .find(|(port, _)| *port == right_port)
            .map(|(_, buffer)| buffer.as_ref())
            .expect("right output port is not connected");
        for sample_index in 0..BLOCK_SIZE {
            unsafe {
                *output.add(sample_index * 2) = left[sample_index];
                *output.add(sample_index * 2 + 1) = right[sample_index];
            }
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
