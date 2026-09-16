use crate::{
    graph::{Node, SourceGraph},
    turtle::{self, RDF_TYPE},
};

const INGEN_BLOCK: &str = "http://drobilla.net/ns/ingen#Block";
const INGEN_TAIL: &str = "http://drobilla.net/ns/ingen#tail";
const INGEN_HEAD: &str = "http://drobilla.net/ns/ingen#head";
const LV2_PROTOTYPE: &str = "http://lv2plug.in/ns/lv2core#prototype";
const LV2_PORT: &str = "http://lv2plug.in/ns/lv2core#port";
const LV2_INDEX: &str = "http://lv2plug.in/ns/lv2core#index";
const LV2_AUDIO_PORT: &str = "http://lv2plug.in/ns/lv2core#AudioPort";
const LV2_OUTPUT_PORT: &str = "http://lv2plug.in/ns/lv2core#OutputPort";
const INGEN_VALUE: &str = "http://drobilla.net/ns/ingen#value";

/// Load the Ingen-specific RDF representation into the common graph model.
pub fn load(graph_file: &str) -> Result<SourceGraph, String> {
    let triples = turtle::parse(graph_file, "Ingen")?;
    let mut nodes = Vec::new();
    for triple in &triples {
        if triple.predicate != RDF_TYPE || triple.object != INGEN_BLOCK {
            continue;
        }
        if triple.subject.starts_with("_:") {
            return Err("Ingen block must have a URI".into());
        }
        if nodes
            .iter()
            .any(|node: &Node| node.subject == triple.subject)
        {
            continue;
        }
        let prototype = turtle::object_for(&triples, &triple.subject, LV2_PROTOTYPE)
            .ok_or_else(|| format!("Ingen block {} has no lv2:prototype", triple.subject))?;
        let ports: Vec<String> = triples
            .iter()
            .filter(|candidate| {
                candidate.subject == triple.subject && candidate.predicate == LV2_PORT
            })
            .map(|candidate| candidate.object.clone())
            .collect();
        let mut port_indices = Vec::new();
        let mut overrides = Vec::new();
        for port in &ports {
            let Some(index) = turtle::object_for(&triples, port, LV2_INDEX) else {
                continue;
            };
            let index = index
                .parse::<u8>()
                .map_err(|_| format!("Ingen port {port} has invalid lv2:index"))?;
            port_indices.push((port.clone(), index));
            if let Some(value) = turtle::object_for(&triples, port, INGEN_VALUE) {
                overrides.push((
                    index,
                    value
                        .parse::<f32>()
                        .map_err(|_| format!("Ingen port {port} has invalid ingen:value"))?,
                ));
            }
        }
        nodes.push(Node {
            subject: triple.subject.clone(),
            prototype: prototype.to_string(),
            ports,
            port_indices,
            overrides,
        });
    }

    let mut arcs = arcs(&triples);
    arcs.sort();
    arcs.dedup();
    let mut outputs = audio_outputs(&triples)?;
    outputs.sort();
    outputs.dedup();
    Ok(SourceGraph {
        nodes,
        arcs,
        outputs,
    })
}

fn arcs(triples: &[turtle::Triple]) -> Vec<(String, String)> {
    triples
        .iter()
        .filter(|triple| triple.predicate == INGEN_HEAD)
        .filter_map(|triple| {
            turtle::object_for(triples, &triple.subject, INGEN_TAIL)
                .map(|tail| (tail.to_string(), triple.object.clone()))
        })
        .collect()
}

fn audio_outputs(triples: &[turtle::Triple]) -> Result<Vec<(String, u8)>, String> {
    let mut outputs = Vec::new();
    for triple in triples {
        if triple.predicate != RDF_TYPE || triple.object != LV2_AUDIO_PORT {
            continue;
        }
        let is_output = triples.iter().any(|candidate| {
            candidate.subject == triple.subject
                && candidate.predicate == RDF_TYPE
                && candidate.object == LV2_OUTPUT_PORT
        });
        if !is_output {
            continue;
        }
        let Some(index) = turtle::object_for(triples, &triple.subject, LV2_INDEX) else {
            continue;
        };
        outputs.push((
            triple.subject.clone(),
            index
                .parse::<u8>()
                .map_err(|_| format!("Ingen output {} has invalid lv2:index", triple.subject))?,
        ));
    }
    Ok(outputs)
}
