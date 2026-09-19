mod lv2;
mod midi_binding;
mod plugin_graph;

use alloc::vec::Vec;
use core::ffi::c_void;

use heapless::spsc::{Consumer, Producer};
use picolv2_image_format::{Bundle, FLASH_ADDRESS, MAX_SIZE};

use self::lv2::{
    Lv2AtomSequence, Lv2AtomSequenceBody, MIDI_BLOCK_CAPACITY, PluginBinary, features_ptr,
};
use self::midi_binding::{MidiControlBinding, load_bindings};
use self::plugin_graph::{ControlToCvBridge, PluginNode, connect_edges, resolve_outputs};

use crate::audio::{
    AUDIO_BLOCK_COUNT, AudioBlockIndex, BLOCK_SIZE, I2S_DMA_BUFFER_COUNT, SAMPLE_RATE,
    block_mut_ptr,
};
use crate::diagnostics::{PluginPerformance, atom_output, diag_info, midi_input};
use crate::midi::MidiEvent;

const TRANSPORT_BPM: f32 = 120.0;
const MIDI_SCHEDULING_DELAY_BLOCKS: usize = AUDIO_BLOCK_COUNT + I2S_DMA_BUFFER_COUNT;
static mut MIDI_SEQUENCE: Lv2AtomSequence = Lv2AtomSequence::empty();

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
    performance: PluginPerformance,
}

impl PluginHost {
    pub fn load(midi_consumer: Consumer<'static, MidiEvent>) -> Self {
        let bundle_bytes =
            unsafe { core::slice::from_raw_parts(FLASH_ADDRESS as *const u8, MAX_SIZE) };
        let bundle = Bundle::parse(bundle_bytes).expect("invalid plugin bundle");
        let graph = bundle.graph().expect("invalid plugin graph");

        let features_ptr = features_ptr();

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
                diag_info!(
                    "graph node {} uri={} reusing loaded binary",
                    node_index,
                    core::str::from_utf8(node_uri).unwrap_or("<invalid utf8>")
                );
                &binaries[pos].1
            } else {
                diag_info!(
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
            nodes.push(PluginNode::load(
                node_index,
                graph_node,
                entry,
                binary,
                features_ptr,
                core::ptr::addr_of_mut!(MIDI_SEQUENCE) as *mut c_void,
            ));
        }
        let control_to_cv_bridges = connect_edges(&bundle, &graph, &mut nodes);
        let midi_control_bindings = load_bindings(&graph, &mut nodes);
        diag_info!(
            "plugin graph ready nodes={} edges={} midi_bindings={}",
            graph.node_count,
            graph.edge_count,
            graph.midi_binding_count
        );
        let (left_output, right_output) = resolve_outputs(&graph);

        let performance = PluginPerformance::new(nodes.len());
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
            performance,
        }
    }

    unsafe fn process(&mut self, output: *mut f32) {
        let block_measurement = PluginPerformance::start();
        let midi_sequence = unsafe { &mut *core::ptr::addr_of_mut!(MIDI_SEQUENCE) };
        midi_sequence.clear();
        assert!(
            midi_sequence.push_position(self.block_start_frame, SAMPLE_RATE, TRANSPORT_BPM),
            "LV2 atom input buffer too small for transport position"
        );
        let mut event_count = 0;
        while event_count < MIDI_BLOCK_CAPACITY {
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
        midi_input(event_count);

        for (index, node) in self.nodes.iter_mut().enumerate() {
            // LV2 Atom outputs receive writable capacity in atom.size; the
            // plugin replaces it with the emitted sequence size during run().
            for (_, sequence) in &mut node.atom_outputs {
                sequence.set_capacity();
            }
            let node_measurement = PluginPerformance::start();
            node.instance.run(BLOCK_SIZE as u32);
            for (port, sequence) in &node.atom_outputs {
                atom_output(
                    index,
                    *port,
                    sequence.atom.size,
                    core::mem::size_of::<Lv2AtomSequenceBody>() as u32,
                );
            }
            self.performance.node_finished(index, node_measurement);
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
        self.performance.block_finished(block_measurement);
    }
}

#[embassy_executor::task]
pub async fn plugin_host_task(
    midi_consumer: Consumer<'static, MidiEvent>,
    mut free_consumer: Consumer<'static, AudioBlockIndex>,
    mut ready_producer: Producer<'static, AudioBlockIndex>,
) -> ! {
    diag_info!("Starting LV2 plugin host task");
    let mut plugin = PluginHost::load(midi_consumer);

    loop {
        let index = loop {
            if let Some(index) = free_consumer.dequeue() {
                break index;
            }
            core::hint::spin_loop();
        };

        unsafe { plugin.process(block_mut_ptr(index)) };

        ready_producer
            .enqueue(index)
            .expect("ready audio block queue unexpectedly full");
    }
}
