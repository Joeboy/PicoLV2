use core::ffi::{CStr, c_char, c_void};

use defmt::info;
use elf_loader::{Loader, Relocator, input::ElfBinary};
use heapless::{Vec, spsc::{Consumer, Producer}};
use lv2_bundle_format::{Bundle, FLASH_ADDRESS, MAX_SIZE};

use crate::audio_buffer::{
    AudioBlockIndex, BLOCK_SIZE, MIDI_SCHEDULING_DELAY_BLOCKS, SAMPLE_RATE,
    block_mut_ptr,
};
use crate::log_heap;
use crate::lv2::{
    ATOM_SEQUENCE_URI, ATOM_SEQUENCE_URID, Lv2Descriptor, Lv2Feature, Lv2UridMap, MIDI_EVENT_URI,
    MIDI_EVENT_URID, URID_MAP_URI,
};
use crate::midi::{Lv2MidiSequence, MidiEvent};
use crate::plugin_metadata::{PluginMetadata, PortKind};

const MAX_NODES: usize = 8;
const MAX_CONTROLS: usize = 8;

static mut MIDI_SEQUENCE: Lv2MidiSequence = Lv2MidiSequence::empty();
static mut NODE_INPUT_AUDIO: [[f32; BLOCK_SIZE]; MAX_NODES] = [[0.0; BLOCK_SIZE]; MAX_NODES];
static mut NODE_OUTPUT_AUDIO: [[f32; BLOCK_SIZE]; MAX_NODES] = [[0.0; BLOCK_SIZE]; MAX_NODES];
static mut NODE_CONTROLS: [[f32; MAX_CONTROLS]; MAX_NODES] = [[0.0; MAX_CONTROLS]; MAX_NODES];

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
        info!("plugin load begin name={} elf_bytes={}", name, elf_bytes.len());
        log_heap("before elf load");
        let raw = Loader::new()
            .run()
            .load_dylib(ElfBinary::new(name, elf_bytes))
            .expect("failed to load lv2 plugin binary");
        log_heap("after elf load");
        let lib = Relocator::new()
            .run(raw)
            .relocate()
            .expect("failed to relocate lv2 plugin binary");
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

    pub fn instantiate(&self, sample_rate: f64, features: *const *const Lv2Feature) -> PluginInstance {
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

struct PluginNode {
    instance: PluginInstance,
    input_port: Option<u32>,
    output_port: Option<u32>,
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
    nodes: Vec<PluginNode, MAX_NODES>,
    output_node: usize,
    midi_consumer: Consumer<'static, MidiEvent>,
    pending_midi: Option<MidiEvent>,
    timeline_origin_micros: u64,
    block_start_frame: u64,
}

impl PluginHost {
    pub fn load(midi_consumer: Consumer<'static, MidiEvent>) -> Self {
        let bundle_bytes = unsafe {
            core::slice::from_raw_parts(FLASH_ADDRESS as *const u8, MAX_SIZE)
        };
        let bundle = Bundle::parse(bundle_bytes).expect("invalid plugin bundle");
        let graph = bundle.graph().expect("invalid plugin graph");
        assert!(graph.node_count as usize <= MAX_NODES, "too many graph nodes");

        let features_ptr = core::ptr::addr_of!(FEATURES) as *const *const Lv2Feature;

        let mut nodes = Vec::new();
        for node_index in 0..graph.node_count {
            let node_uri = graph.node(node_index).expect("invalid graph node").uri;
            let entry = bundle.find(node_uri).expect("graph plugin missing from bundle");
            info!(
                "graph node {} uri={} binary_bytes={} metadata_bytes={}",
                node_index,
                node_uri,
                entry.binary.len(),
                entry.metadata.len()
            );
            let binary = PluginBinary::load("graph-plugin.so", entry.binary);
            let metadata = PluginMetadata::parse(entry.metadata).expect("invalid graph metadata");
            let mut instance = binary.instantiate(SAMPLE_RATE as f64, features_ptr);
            log_heap("after instantiate");
            let input = metadata.port(PortKind::AudioInput, 0).map(|port| port.index);
            let output = metadata.port(PortKind::AudioOutput, 0).map(|port| port.index);
            if let Some(port) = metadata.port(PortKind::AtomInput, 0) {
                instance.connect_port(port.index, core::ptr::addr_of_mut!(MIDI_SEQUENCE) as *mut c_void);
            }
            if let Some(port) = input {
                let buffer = unsafe { core::ptr::addr_of_mut!(NODE_INPUT_AUDIO[node_index as usize]) };
                instance.connect_port(port, buffer as *mut c_void);
            }
            if let Some(port) = output {
                let buffer = unsafe { core::ptr::addr_of_mut!(NODE_OUTPUT_AUDIO[node_index as usize]) };
                instance.connect_port(port, buffer as *mut c_void);
            }
            let mut control_index = 0;
            while let Some(port) = metadata.port(PortKind::ControlInput, control_index) {
                assert!(control_index < MAX_CONTROLS, "too many graph controls");
                unsafe { NODE_CONTROLS[node_index as usize][control_index] = port.default.unwrap_or(0.0); }
                let control = unsafe {
                    core::ptr::addr_of_mut!(NODE_CONTROLS[node_index as usize][control_index])
                };
                instance.connect_port(port.index, control as *mut c_void);
                control_index += 1;
            }
            instance.activate();
            nodes
                .push(PluginNode { instance, input_port: input, output_port: output })
                .unwrap_or_else(|_| panic!("too many graph nodes"));
        }
        for edge_index in 0..graph.edge_count {
            let edge = graph.edge(edge_index).expect("invalid graph edge");
            assert!((edge.source_node as usize) < nodes.len() && (edge.destination_node as usize) < nodes.len(), "graph node reference out of range");
            assert!(edge.source_node < edge.destination_node, "graph nodes must be topologically ordered");
            nodes[edge.source_node as usize].output_port.expect("graph source has no audio output");
            let destination_port = nodes[edge.destination_node as usize].input_port.expect("graph destination has no audio input");
            assert!(edge.source_port == 0 && edge.destination_port == 0, "only first audio ports are supported");
            let buffer = unsafe {
                core::ptr::addr_of_mut!(NODE_OUTPUT_AUDIO[edge.source_node as usize])
            };
            nodes[edge.destination_node as usize]
                .instance
                .connect_port(destination_port, buffer as *mut c_void);
        }
        let mut has_outgoing = [false; MAX_NODES];
        for edge_index in 0..graph.edge_count {
            has_outgoing[graph.edge(edge_index).unwrap().source_node as usize] = true;
        }
        let output_node = (0..nodes.len()).rev().find(|index| !has_outgoing[*index]).expect("graph has no output");

        Self {
            nodes,
            output_node,
            midi_consumer,
            pending_midi: None,
            timeline_origin_micros: embassy_time::Instant::now().as_micros(),
            block_start_frame: 0,
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

        for node in &mut self.nodes {
            node.instance.run(BLOCK_SIZE as u32);
        }
        unsafe {
            core::ptr::copy_nonoverlapping(
                NODE_OUTPUT_AUDIO[self.output_node].as_ptr(), output,
                BLOCK_SIZE,
            );
        }

        self.block_start_frame += BLOCK_SIZE as u64;
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

    loop {
        let index = loop {
            if let Some(index) = free_consumer.dequeue() {
                break index;
            }
            embassy_futures::yield_now().await;
        };

        unsafe { plugin.process(block_mut_ptr(index)) };
        while ready_producer.enqueue(index).is_err() {
            embassy_futures::yield_now().await;
        }
    }
}
