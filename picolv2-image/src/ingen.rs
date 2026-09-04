use crate::turtle::{self, RDF_TYPE};

const INGEN_BLOCK: &str = "http://drobilla.net/ns/ingen#Block";
const INGEN_ARC: &str = "http://drobilla.net/ns/ingen#Arc";
const INGEN_TAIL: &str = "http://drobilla.net/ns/ingen#tail";
const INGEN_HEAD: &str = "http://drobilla.net/ns/ingen#head";
const LV2_PROTOTYPE: &str = "http://lv2plug.in/ns/lv2core#prototype";

pub fn compile(path: &str) -> Result<Vec<u8>, String> {
    let triples = turtle::parse(path, "Ingen")?;
    let mut nodes = Vec::new();
    for triple in &triples {
        if triple.predicate == RDF_TYPE && triple.object == INGEN_BLOCK {
            if triple.subject.starts_with("_:") {
                return Err("Ingen block must have a URI".into());
            }
            let prototype = turtle::object_for(&triples, &triple.subject, LV2_PROTOTYPE)
                .ok_or_else(|| format!("Ingen block {} has no lv2:prototype", triple.subject))?;
            nodes.push((triple.subject.clone(), prototype.to_string()));
        }
    }
    if nodes.is_empty() {
        return Err("Ingen graph contains no plugin blocks".into());
    }

    let mut edges = Vec::new();
    for triple in &triples {
        if triple.predicate != RDF_TYPE || triple.object != INGEN_ARC {
            continue;
        }
        let tail = turtle::object_for(&triples, &triple.subject, INGEN_TAIL)
            .ok_or_else(|| format!("Ingen arc missing {INGEN_TAIL}"))?;
        let head = turtle::object_for(&triples, &triple.subject, INGEN_HEAD)
            .ok_or_else(|| format!("Ingen arc missing {INGEN_HEAD}"))?;
        edges.push((node_for_port(&nodes, tail)?, node_for_port(&nodes, head)?));
    }

    let mut result = Vec::new();
    result.extend_from_slice(picolv2_image_format::GRAPH_MAGIC);
    result.extend_from_slice(&picolv2_image_format::GRAPH_VERSION.to_le_bytes());
    result.extend_from_slice(&(nodes.len() as u16).to_le_bytes());
    result.extend_from_slice(&(edges.len() as u16).to_le_bytes());
    for (_, prototype) in &nodes {
        result.extend_from_slice(&(prototype.len() as u16).to_le_bytes());
        result.extend_from_slice(&[0, 0]);
        result.extend_from_slice(prototype.as_bytes());
    }
    for (source, destination) in edges {
        result.extend_from_slice(&(source as u16).to_le_bytes());
        result.extend_from_slice(&[0, 0]);
        result.extend_from_slice(&(destination as u16).to_le_bytes());
        result.extend_from_slice(&[0, 0]);
    }
    Ok(result)
}

fn node_for_port(nodes: &[(String, String)], port: &str) -> Result<usize, String> {
    nodes
        .iter()
        .enumerate()
        .find(|(_, (node, _))| {
            port.starts_with(node) && port.as_bytes().get(node.len()) == Some(&b'/')
        })
        .map(|(index, _)| index)
        .ok_or_else(|| format!("Ingen arc port {port} does not belong to a block"))
}
