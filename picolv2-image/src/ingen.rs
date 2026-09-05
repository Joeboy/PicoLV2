use std::path::Path;

use crate::turtle::{self, RDF_TYPE};

const INGEN_GRAPH: &str = "http://drobilla.net/ns/ingen#Graph";
const INGEN_GRAPH_PROTOTYPE: &str = "http://drobilla.net/ns/ingen#GraphPrototype";
const INGEN_BLOCK: &str = "http://drobilla.net/ns/ingen#Block";
const INGEN_TAIL: &str = "http://drobilla.net/ns/ingen#tail";
const INGEN_HEAD: &str = "http://drobilla.net/ns/ingen#head";
const LV2_PROTOTYPE: &str = "http://lv2plug.in/ns/lv2core#prototype";
const LV2_PORT: &str = "http://lv2plug.in/ns/lv2core#port";
const RDFS_SEE_ALSO: &str = "http://www.w3.org/2000/01/rdf-schema#seeAlso";

#[derive(Clone, Debug)]
struct Block {
    subject: String,
    prototype: String,
    ports: Vec<String>,
}

pub fn compile(path: &str) -> Result<Vec<u8>, String> {
    let graph_file = resolve_graph_file(path)?;
    let triples = turtle::parse(&graph_file, "Ingen")?;

    let mut blocks = Vec::new();
    for triple in &triples {
        if triple.predicate == RDF_TYPE && triple.object == INGEN_BLOCK {
            if triple.subject.starts_with("_:") {
                return Err("Ingen block must have a URI".into());
            }
            if blocks.iter().any(|b: &Block| b.subject == triple.subject) {
                continue;
            }
            let prototype = turtle::object_for(&triples, &triple.subject, LV2_PROTOTYPE)
                .ok_or_else(|| format!("Ingen block {} has no lv2:prototype", triple.subject))?;
            let ports: Vec<String> = triples
                .iter()
                .filter(|t| t.subject == triple.subject && t.predicate == LV2_PORT)
                .map(|t| t.object.clone())
                .collect();
            blocks.push(Block {
                subject: triple.subject.clone(),
                prototype: prototype.to_string(),
                ports,
            });
        }
    }
    if blocks.is_empty() {
        return Err("Ingen graph contains no plugin blocks".into());
    }

    let mut arcs = Vec::new();
    for triple in &triples {
        if triple.predicate == INGEN_HEAD {
            let head = triple.object.clone();
            if let Some(tail) = turtle::object_for(&triples, &triple.subject, INGEN_TAIL) {
                arcs.push((tail.to_string(), head));
            }
        }
    }
    arcs.sort();
    arcs.dedup();

    let find_block = |port: &str| -> Option<usize> {
        blocks.iter().position(|b| {
            b.ports.iter().any(|p| p == port)
                || port.starts_with(&format!("{}/", b.subject))
                || port == b.subject
        })
    };

    let mut raw_edges = Vec::new();
    for (tail, head) in arcs {
        if let (Some(src), Some(dst)) = (find_block(&tail), find_block(&head)) {
            if src != dst {
                raw_edges.push((src, dst));
            }
        }
    }
    raw_edges.sort();
    raw_edges.dedup();

    let num_blocks = blocks.len();
    let mut in_degree = vec![0usize; num_blocks];
    let mut adjacency = vec![Vec::new(); num_blocks];
    for &(src, dst) in &raw_edges {
        in_degree[dst] += 1;
        adjacency[src].push(dst);
    }

    let mut ready: Vec<usize> = (0..num_blocks).filter(|&i| in_degree[i] == 0).collect();
    let mut sorted_indices = Vec::with_capacity(num_blocks);

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

    if sorted_indices.len() != num_blocks {
        return Err("Ingen graph contains a cycle".into());
    }

    let mut old_to_new = vec![0usize; num_blocks];
    for (new_idx, &old_idx) in sorted_indices.iter().enumerate() {
        old_to_new[old_idx] = new_idx;
    }

    let sorted_blocks: Vec<Block> = sorted_indices.into_iter().map(|i| blocks[i].clone()).collect();
    let mut edges: Vec<(usize, usize)> = raw_edges
        .into_iter()
        .map(|(src, dst)| (old_to_new[src], old_to_new[dst]))
        .collect();
    edges.sort();
    edges.dedup();

    let mut result = Vec::new();
    result.extend_from_slice(picolv2_image_format::GRAPH_MAGIC);
    result.extend_from_slice(&picolv2_image_format::GRAPH_VERSION.to_le_bytes());
    result.extend_from_slice(&(sorted_blocks.len() as u16).to_le_bytes());
    result.extend_from_slice(&(edges.len() as u16).to_le_bytes());
    for block in &sorted_blocks {
        result.extend_from_slice(&(block.prototype.len() as u16).to_le_bytes());
        result.extend_from_slice(&[0, 0]);
        result.extend_from_slice(block.prototype.as_bytes());
    }
    for (source, destination) in edges {
        result.extend_from_slice(&(source as u16).to_le_bytes());
        result.extend_from_slice(&[0, 0]);
        result.extend_from_slice(&(destination as u16).to_le_bytes());
        result.extend_from_slice(&[0, 0]);
    }
    Ok(result)
}

fn resolve_graph_file(path: &str) -> Result<String, String> {
    let p = Path::new(path);
    let manifest_path = if p.is_dir() {
        let candidate = p.join("manifest.ttl");
        if candidate.is_file() {
            candidate
        } else {
            return Err(format!("no manifest.ttl found in Ingen bundle {path}"));
        }
    } else if p.file_name().and_then(|n| n.to_str()) == Some("manifest.ttl") && p.is_file() {
        p.to_path_buf()
    } else {
        return Err(format!(
            "expected an Ingen bundle directory (e.g. *.ingen): {path}"
        ));
    };

    let manifest_str = manifest_path.to_string_lossy();
    let triples = turtle::parse(&manifest_str, "Ingen manifest")?;

    for triple in &triples {
        if (triple.predicate == RDF_TYPE
            && (triple.object == INGEN_GRAPH || triple.object == INGEN_GRAPH_PROTOTYPE))
            || (triple.predicate == LV2_PROTOTYPE && triple.object == INGEN_GRAPH_PROTOTYPE)
        {
            let see_also = turtle::object_for(&triples, &triple.subject, RDFS_SEE_ALSO)
                .unwrap_or(&triple.subject);
            return local_path(see_also, &manifest_str);
        }
    }

    for triple in &triples {
        if triple.predicate == RDFS_SEE_ALSO {
            return local_path(&triple.object, &manifest_str);
        }
    }

    Err(format!(
        "no graph found in manifest {}",
        manifest_path.display()
    ))
}

fn local_path(uri: &str, referring_path: &str) -> Result<String, String> {
    let raw_path = uri.strip_prefix("file:").unwrap_or(uri);
    let path = Path::new(raw_path);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(referring_path)
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path)
    };
    Ok(resolved.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use picolv2_image_format::Graph;

    #[test]
    fn test_compile_all_ingen_bundles() {
        let bundles = [
            (
                "../graphs/monosynth-plus-delay.ingen",
                b"https://joebutton.co.uk/lv2/monosynth-poc" as &[u8],
            ),
            (
                "../graphs/oxynth-plus-delay.ingen",
                b"https://joebutton.co.uk/lv2/oxynth-poc",
            ),
            (
                "../graphs/string-synth-plus-delay.ingen",
                b"https://joebutton.co.uk/lv2/string-synth",
            ),
            (
                "../graphs/tine-piano-plus-delay.ingen",
                b"https://joebutton.co.uk/lv2/tine-piano",
            ),
        ];

        for (bundle_path, synth_uri) in bundles {
            let bytes = compile(bundle_path)
                .unwrap_or_else(|e| panic!("failed to compile {bundle_path}: {e}"));
            let graph = Graph::parse(&bytes)
                .unwrap_or_else(|_| panic!("failed to parse graph for {bundle_path}"));
            assert_eq!(graph.node_count, 2, "node count mismatch for {bundle_path}");
            assert_eq!(graph.edge_count, 1, "edge count mismatch for {bundle_path}");

            let node0 = graph.node(0).unwrap();
            assert_eq!(
                node0.uri, source_uri,
                "source node URI mismatch for {bundle_path}"
            );

            let node1 = graph.node(1).unwrap();
            assert_eq!(
                node1.uri, destination_uri,
                "destination node URI mismatch for {bundle_path}"
            );

            let edge0 = graph.edge(0).unwrap();
            assert_eq!(edge0.source_node, 0);
            assert_eq!(edge0.destination_node, 1);
            assert_eq!(edge0.source_port, 1);
            assert_eq!(edge0.destination_port, 0);
            assert!(edge0.source_node < edge0.destination_node);
        }
    }

    #[test]
    fn test_compile_bundle_manifest() {
        let bytes = compile("../graphs/tine-piano-plus-delay.ingen/manifest.ttl")
            .expect("failed to compile manifest");
        let graph = Graph::parse(&bytes).expect("failed to parse compiled graph");
        assert_eq!(graph.node_count, 2);
        assert_eq!(graph.edge_count, 1);
        let node0 = graph.node(0).unwrap();
        assert_eq!(node0.uri, b"https://joebutton.co.uk/lv2/tine-piano");
    }

    #[test]
    fn test_rejects_bare_ttl_without_manifest() {
        let result = compile("../graphs/tine-piano-plus-delay.ingen/main.ttl");
        assert!(result.is_err());
    }
}
