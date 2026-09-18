use std::path::{Path, PathBuf};

use crate::{
    patch_formats::{ingen, mod_pedalboard},
    turtle::{self, RDF_TYPE},
};

const INGEN_GRAPH: &str = "http://drobilla.net/ns/ingen#Graph";
const INGEN_GRAPH_PROTOTYPE: &str = "http://drobilla.net/ns/ingen#GraphPrototype";
const LV2_PROTOTYPE: &str = "http://lv2plug.in/ns/lv2core#prototype";
const RDFS_SEE_ALSO: &str = "http://www.w3.org/2000/01/rdf-schema#seeAlso";
const MOD_PEDALBOARD: &str = "http://moddevices.com/ns/modpedal#Pedalboard";

#[derive(Clone, Debug)]
pub struct Node {
    pub subject: String,
    pub prototype: String,
    pub ports: Vec<String>,
    pub port_indices: Vec<(String, u8)>,
    pub overrides: Vec<(u8, f32)>,
}

#[derive(Debug)]
pub struct SourceGraph {
    pub nodes: Vec<Node>,
    pub arcs: Vec<(String, String)>,
    pub outputs: Vec<(String, u8)>,
    pub midi_bindings: Vec<SourceMidiBinding>,
}

#[derive(Clone, Debug)]
pub struct SourceMidiBinding {
    pub port: String,
    pub channel: u8,
    pub controller: u8,
    pub flags: u8,
    pub minimum: f32,
    pub maximum: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputKind {
    Ingen,
    ModPedalboard,
}

pub fn compile(path: &str, search_path: &str) -> Result<Vec<u8>, String> {
    let (kind, graph_file) = resolve_input(path)?;
    let source = match kind {
        InputKind::Ingen => ingen::load(&graph_file)?,
        InputKind::ModPedalboard => mod_pedalboard::load(&graph_file, search_path)?,
    };
    encode(source)
}

fn resolve_input(path: &str) -> Result<(InputKind, String), String> {
    let p = Path::new(path);
    let manifest_path = if p.is_dir() {
        let candidate = p.join("manifest.ttl");
        if !candidate.is_file() {
            return Err(format!("no manifest.ttl found in graph bundle {path}"));
        }
        candidate
    } else if p.file_name().and_then(|n| n.to_str()) == Some("manifest.ttl") && p.is_file() {
        p.to_path_buf()
    } else {
        return Err(format!(
            "expected an Ingen or MOD pedalboard bundle directory: {path}"
        ));
    };

    let manifest_str = manifest_path.to_string_lossy();
    let triples = turtle::parse(&manifest_str, "graph manifest")?;
    let kind = if triples
        .iter()
        .any(|triple| triple.predicate == RDF_TYPE && triple.object == MOD_PEDALBOARD)
    {
        InputKind::ModPedalboard
    } else if triples.iter().any(|triple| {
        (triple.predicate == RDF_TYPE
            && (triple.object == INGEN_GRAPH || triple.object == INGEN_GRAPH_PROTOTYPE))
            || (triple.predicate == LV2_PROTOTYPE && triple.object == INGEN_GRAPH_PROTOTYPE)
    }) {
        InputKind::Ingen
    } else {
        return Err(format!(
            "manifest {} is neither an Ingen graph nor a MOD pedalboard",
            manifest_path.display()
        ));
    };

    let graph_uri = triples
        .iter()
        .find(|triple| {
            triple.predicate == RDF_TYPE
                && match kind {
                    InputKind::Ingen => {
                        triple.object == INGEN_GRAPH || triple.object == INGEN_GRAPH_PROTOTYPE
                    }
                    InputKind::ModPedalboard => triple.object == MOD_PEDALBOARD,
                }
        })
        .and_then(|triple| {
            turtle::object_for(&triples, &triple.subject, RDFS_SEE_ALSO)
                .or(Some(triple.subject.as_str()))
        })
        .or_else(|| {
            triples
                .iter()
                .find(|triple| triple.predicate == RDFS_SEE_ALSO)
                .map(|triple| triple.object.as_str())
        })
        .ok_or_else(|| format!("no graph found in manifest {}", manifest_path.display()))?;

    Ok((
        kind,
        local_path(graph_uri, &manifest_path)?
            .to_string_lossy()
            .into_owned(),
    ))
}

fn local_path(uri: &str, manifest_path: &Path) -> Result<PathBuf, String> {
    let raw_path = uri.strip_prefix("file:").unwrap_or(uri);
    let decoded = percent_decode(raw_path)?;
    let path = Path::new(&decoded);
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        manifest_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path)
    })
}

fn percent_decode(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let encoded = bytes
                .get(index + 1..index + 3)
                .ok_or_else(|| format!("invalid percent escape in URI {value}"))?;
            let encoded = std::str::from_utf8(encoded)
                .map_err(|_| format!("invalid percent escape in URI {value}"))?;
            result.push(
                u8::from_str_radix(encoded, 16)
                    .map_err(|_| format!("invalid percent escape in URI {value}"))?,
            );
            index += 3;
        } else {
            result.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(result).map_err(|_| format!("URI path is not valid UTF-8: {value}"))
}

fn encode(source: SourceGraph) -> Result<Vec<u8>, String> {
    if source.nodes.is_empty() {
        return Err("graph contains no plugin blocks".into());
    }

    let find_node = |port: &str| -> Option<usize> {
        source.nodes.iter().position(|node| {
            node.ports.iter().any(|candidate| candidate == port)
                || port.starts_with(&format!("{}/", node.subject))
                || port == node.subject
        })
    };
    let port_index = |port: &str| -> Result<u8, String> {
        source
            .nodes
            .iter()
            .flat_map(|node| node.port_indices.iter())
            .find(|(subject, _)| subject == port)
            .map(|(_, index)| *index)
            .ok_or_else(|| format!("graph port {port} has no LV2 index"))
    };

    let mut raw_edges = Vec::new();
    for (tail, head) in &source.arcs {
        if let (Some(src), Some(dst)) = (find_node(tail), find_node(head))
            && src != dst
        {
            raw_edges.push((src, port_index(tail)?, dst, port_index(head)?));
        }
    }
    raw_edges.sort();
    raw_edges.dedup();

    let mut graph_outputs = Vec::new();
    for (tail, head) in &source.arcs {
        if find_node(head).is_some() {
            continue;
        }
        let Some(output_index) = source
            .outputs
            .iter()
            .find(|(subject, _)| subject == head)
            .map(|(_, index)| *index)
        else {
            continue;
        };
        let Some(src) = find_node(tail) else {
            continue;
        };
        graph_outputs.push((output_index, src, port_index(tail)?));
    }
    graph_outputs.sort();
    graph_outputs.dedup();

    let num_nodes = source.nodes.len();
    let mut in_degree = vec![0usize; num_nodes];
    let mut adjacency = vec![Vec::new(); num_nodes];
    for &(src, _, dst, _) in &raw_edges {
        in_degree[dst] += 1;
        adjacency[src].push(dst);
    }
    let mut ready: Vec<usize> = (0..num_nodes).filter(|&i| in_degree[i] == 0).collect();
    let mut sorted_indices = Vec::with_capacity(num_nodes);
    while !ready.is_empty() {
        let u = ready.remove(0);
        sorted_indices.push(u);
        for &v in &adjacency[u] {
            in_degree[v] -= 1;
            if in_degree[v] == 0 {
                ready.push(v);
                ready.sort();
            }
        }
    }
    if sorted_indices.len() != num_nodes {
        return Err("graph contains a cycle".into());
    }

    let mut old_to_new = vec![0usize; num_nodes];
    for (new_idx, &old_idx) in sorted_indices.iter().enumerate() {
        old_to_new[old_idx] = new_idx;
    }
    let sorted_nodes: Vec<Node> = sorted_indices
        .into_iter()
        .map(|i| source.nodes[i].clone())
        .collect();
    let mut edges: Vec<_> = raw_edges
        .into_iter()
        .map(|(src, source_port, dst, destination_port)| {
            (
                old_to_new[src],
                source_port,
                old_to_new[dst],
                destination_port,
            )
        })
        .collect();
    edges.sort();
    edges.dedup();
    let outputs: Vec<_> = graph_outputs
        .into_iter()
        .map(|(_, src, source_port)| (old_to_new[src], source_port))
        .collect();
    let mut midi_bindings = Vec::new();
    for binding in &source.midi_bindings {
        let node = find_node(&binding.port)
            .ok_or_else(|| format!("MIDI binding port {} has no graph node", binding.port))?;
        midi_bindings.push((
            old_to_new[node],
            port_index(&binding.port)?,
            binding.channel,
            binding.controller,
            binding.flags,
            binding.minimum,
            binding.maximum,
        ));
    }
    midi_bindings.sort_by_key(|binding| (binding.0, binding.1, binding.2, binding.3));
    midi_bindings.dedup();

    let mut result = Vec::new();
    result.extend_from_slice(picolv2_image_format::GRAPH_MAGIC);
    result.extend_from_slice(&picolv2_image_format::GRAPH_VERSION.to_le_bytes());
    result.extend_from_slice(&(sorted_nodes.len() as u16).to_le_bytes());
    result.extend_from_slice(&(edges.len() as u16).to_le_bytes());
    for node in &sorted_nodes {
        result.extend_from_slice(&(node.prototype.len() as u16).to_le_bytes());
        result.extend_from_slice(&(node.overrides.len() as u16).to_le_bytes());
        result.extend_from_slice(node.prototype.as_bytes());
        for &(port_index, value) in &node.overrides {
            result.push(port_index);
            result.push(0);
            result.extend_from_slice(&value.to_le_bytes());
        }
    }
    for (source, source_port, destination, destination_port) in edges {
        result.extend_from_slice(&(source as u16).to_le_bytes());
        result.extend_from_slice(&[source_port, 0]);
        result.extend_from_slice(&(destination as u16).to_le_bytes());
        result.extend_from_slice(&[destination_port, 0]);
    }
    result.extend_from_slice(&(outputs.len() as u16).to_le_bytes());
    for (node, port) in outputs {
        result.extend_from_slice(&(node as u16).to_le_bytes());
        result.extend_from_slice(&[port, 0]);
    }
    result.extend_from_slice(&(midi_bindings.len() as u16).to_le_bytes());
    for (node, port, channel, controller, flags, minimum, maximum) in midi_bindings {
        result.extend_from_slice(&(node as u16).to_le_bytes());
        result.extend_from_slice(&[port, channel, controller, flags, 0, 0]);
        result.extend_from_slice(&minimum.to_le_bytes());
        result.extend_from_slice(&maximum.to_le_bytes());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use picolv2_image_format::Graph;

    #[test]
    fn compiles_existing_ingen_bundles() {
        let bundles = [
            (
                "../patches/throwaway/monosynth-plus-delay.ingen",
                b"https://joebutton.co.uk/lv2/monosynth-poc" as &[u8],
            ),
            (
                "../patches/throwaway/oxynth-plus-delay.ingen",
                b"https://joebutton.co.uk/lv2/oxynth-poc",
            ),
            (
                "../patches/throwaway/string-synth-plus-delay.ingen",
                b"https://joebutton.co.uk/lv2/string-synth",
            ),
            (
                "../patches/throwaway/tine-piano-plus-delay.ingen",
                b"https://joebutton.co.uk/lv2/tine-piano",
            ),
        ];
        for (path, source_uri) in bundles {
            let bytes = compile(path, "../plugins/pico")
                .unwrap_or_else(|error| panic!("failed to compile {path}: {error}"));
            let graph = Graph::parse(&bytes).expect("failed to parse compiled graph");
            assert_eq!(graph.node_count, 2);
            assert_eq!(graph.edge_count, 1);
            assert_eq!(graph.node(0).unwrap().uri, source_uri);
            assert_eq!(
                graph.node(1).unwrap().uri,
                b"https://joebutton.co.uk/lv2/delay-poc"
            );
        }
    }

    #[test]
    fn accepts_ingen_manifest_path() {
        let bytes = compile(
            "../patches/throwaway/tine-piano-plus-delay.ingen/manifest.ttl",
            "../plugins/pico",
        )
        .expect("failed to compile Ingen manifest");
        assert_eq!(Graph::parse(&bytes).unwrap().node_count, 2);
    }

    #[test]
    fn rejects_bare_turtle_graph() {
        let result = compile(
            "../patches/throwaway/tine-piano-plus-delay.ingen/main.ttl",
            "../plugins/pico",
        );
        assert!(result.is_err());
    }

    #[test]
    fn compiles_mod_pedalboard_using_plugin_port_symbols() {
        let bytes = compile("tests/fixtures/mod basic.pedalboard", "../plugins/pico")
            .expect("failed to compile MOD pedalboard");
        let graph = Graph::parse(&bytes).expect("failed to parse compiled graph");
        assert_eq!(graph.node_count, 1);
        assert_eq!(graph.edge_count, 0);
        assert_eq!(graph.output_count, 1);
        assert_eq!(graph.midi_binding_count, 1);
        let node = graph.node(0).unwrap();
        assert_eq!(node.uri, b"http://moddevices.com/plugins/mda/DX10");
        assert_eq!(node.override_value(0), Some(123.5));
        let output = graph.output(0).unwrap();
        assert_eq!(output.node, 0);
        assert_eq!(output.port, 16);
        let binding = graph.midi_binding(0).unwrap();
        assert_eq!(binding.node, 0);
        assert_eq!(binding.port, 0);
        assert_eq!(binding.channel, 0);
        assert_eq!(binding.controller, 1);
        assert_eq!(
            binding.flags,
            picolv2_image_format::MIDI_BINDING_LOGARITHMIC
        );
        assert_eq!(binding.minimum, 2.5);
        assert_eq!(binding.maximum, 4000.0);
    }
}
