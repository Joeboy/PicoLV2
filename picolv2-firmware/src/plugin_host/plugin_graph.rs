use alloc::{boxed::Box, vec::Vec};
use core::ffi::c_void;

use defmt::info;
use picolv2_image_format::{Bundle, Entry, Graph, Node, PluginMetadata, PortKind};

use super::lv2::Lv2Feature;
use super::lv2_runtime::{PluginBinary, PluginInstance};
use crate::audio_buffer::{BLOCK_SIZE, SAMPLE_RATE};
use crate::log_heap;
use crate::midi::Lv2AtomSequence;

// Bridges a block-rate control output to an audio-rate CV input; the source
// value is broadcast across the destination's whole block every render cycle.
pub(super) struct ControlToCvBridge {
    pub(super) source_node: usize,
    pub(super) source: *const f32,
    pub(super) destination: *mut [f32; BLOCK_SIZE],
}

pub(super) struct PluginNode {
    pub(super) instance: PluginInstance,
    // These collections own every buffer connected to the plugin instance.
    #[allow(dead_code)]
    pub(super) audio_inputs: Vec<(u32, Box<[f32; BLOCK_SIZE]>)>,
    pub(super) audio_outputs: Vec<(u32, Box<[f32; BLOCK_SIZE]>)>,
    pub(super) control_inputs: Vec<(u32, Box<f32>)>,
    pub(super) control_outputs: Vec<(u32, Box<f32>)>,
    pub(super) cv_inputs: Vec<(u32, Box<[f32; BLOCK_SIZE]>)>,
    pub(super) cv_outputs: Vec<(u32, Box<[f32; BLOCK_SIZE]>)>,
    // Atom output buffers are shared directly with downstream Atom inputs.
    pub(super) atom_outputs: Vec<(u32, Box<Lv2AtomSequence>)>,
}

impl PluginNode {
    pub(super) fn load(
        node_index: u16,
        graph_node: Node<'_>,
        entry: Entry<'_>,
        binary: &PluginBinary,
        features: *const *const Lv2Feature,
        midi_sequence: *mut c_void,
    ) -> Self {
        let metadata = PluginMetadata::parse(entry.metadata).expect("invalid graph metadata");
        let mut instance = binary.instantiate(SAMPLE_RATE as f64, features);
        log_heap("after instantiate");
        if let Some(port) = metadata.port(PortKind::AtomInput, 0) {
            instance.connect_port(port.index, midi_sequence);
        }

        let mut audio_inputs = Vec::new();
        let mut index = 0;
        while let Some(port) = metadata.port(PortKind::AudioInput, index) {
            let mut buffer = Box::new([0.0f32; BLOCK_SIZE]);
            instance.connect_port(port.index, buffer.as_mut_ptr() as *mut c_void);
            audio_inputs.push((port.index, buffer));
            index += 1;
        }

        let mut audio_outputs = Vec::new();
        let mut index = 0;
        while let Some(port) = metadata.port(PortKind::AudioOutput, index) {
            let mut buffer = Box::new([0.0f32; BLOCK_SIZE]);
            instance.connect_port(port.index, buffer.as_mut_ptr() as *mut c_void);
            audio_outputs.push((port.index, buffer));
            index += 1;
        }

        let mut control_inputs = Vec::new();
        let mut index = 0;
        while let Some(port) = metadata.port(PortKind::ControlInput, index) {
            let value = graph_node
                .override_value(port.index as u8)
                .unwrap_or_else(|| port.default.unwrap_or(0.0));
            let mut control = Box::new(value);
            instance.connect_port(port.index, control.as_mut() as *mut f32 as *mut c_void);
            control_inputs.push((port.index, control));
            index += 1;
        }

        let mut control_outputs = Vec::new();
        let mut index = 0;
        while let Some(port) = metadata.port(PortKind::ControlOutput, index) {
            let mut control = Box::new(0.0f32);
            instance.connect_port(port.index, control.as_mut() as *mut f32 as *mut c_void);
            control_outputs.push((port.index, control));
            index += 1;
        }

        let mut cv_inputs = Vec::new();
        let mut index = 0;
        while let Some(port) = metadata.port(PortKind::CvInput, index) {
            let value = graph_node
                .override_value(port.index as u8)
                .unwrap_or_else(|| port.default.unwrap_or(0.0));
            let mut buffer = Box::new([value; BLOCK_SIZE]);
            instance.connect_port(port.index, buffer.as_mut_ptr() as *mut c_void);
            cv_inputs.push((port.index, buffer));
            index += 1;
        }

        let mut cv_outputs = Vec::new();
        let mut index = 0;
        while let Some(port) = metadata.port(PortKind::CvOutput, index) {
            let mut buffer = Box::new([0.0f32; BLOCK_SIZE]);
            instance.connect_port(port.index, buffer.as_mut_ptr() as *mut c_void);
            cv_outputs.push((port.index, buffer));
            index += 1;
        }

        let mut atom_outputs = Vec::new();
        let mut index = 0;
        while let Some(port) = metadata.port(PortKind::AtomOutput, index) {
            let mut sequence = Box::new(Lv2AtomSequence::empty());
            sequence.set_capacity();
            instance.connect_port(port.index, sequence.as_mut() as *mut _ as *mut c_void);
            atom_outputs.push((port.index, sequence));
            index += 1;
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
        Self {
            instance,
            audio_inputs,
            audio_outputs,
            control_inputs,
            control_outputs,
            cv_inputs,
            cv_outputs,
            atom_outputs,
        }
    }
}

pub(super) fn connect_edges(
    bundle: &Bundle<'_>,
    graph: &Graph<'_>,
    nodes: &mut [PluginNode],
) -> Vec<ControlToCvBridge> {
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
        let metadata = |node| {
            PluginMetadata::parse(
                bundle
                    .find(graph.node(node).expect("invalid graph node").uri)
                    .expect("graph plugin missing from bundle")
                    .metadata,
            )
            .expect("invalid graph metadata")
        };
        let source_metadata = metadata(edge.source_node);
        let destination_metadata = metadata(edge.destination_node);
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
                if source.kind == PortKind::CvOutput && destination.kind == PortKind::CvInput =>
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
    control_to_cv_bridges
}

pub(super) fn resolve_outputs(graph: &Graph<'_>) -> ((usize, u32), (usize, u32)) {
    assert!(graph.output_count > 0, "graph has no audio output");
    let output = |output_index: u16| {
        let output = graph.output(output_index).expect("invalid graph output");
        (output.node as usize, output.port as u32)
    };
    let left = output(0);
    let right = if graph.output_count > 1 {
        output(1)
    } else {
        left
    };
    (left, right)
}
