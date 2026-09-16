use crate::{
    graph::{Node, SourceGraph},
    lv2,
    turtle::{self, RDF_TYPE},
};

const INGEN_BLOCK: &str = "http://drobilla.net/ns/ingen#Block";
const INGEN_TAIL: &str = "http://drobilla.net/ns/ingen#tail";
const INGEN_HEAD: &str = "http://drobilla.net/ns/ingen#head";
const INGEN_VALUE: &str = "http://drobilla.net/ns/ingen#value";
const INGEN_ENABLED: &str = "http://drobilla.net/ns/ingen#enabled";
const LV2_PROTOTYPE: &str = "http://lv2plug.in/ns/lv2core#prototype";
const LV2_PORT: &str = "http://lv2plug.in/ns/lv2core#port";
const LV2_INDEX: &str = "http://lv2plug.in/ns/lv2core#index";
const LV2_AUDIO_PORT: &str = "http://lv2plug.in/ns/lv2core#AudioPort";
const LV2_OUTPUT_PORT: &str = "http://lv2plug.in/ns/lv2core#OutputPort";

/// Load a MOD pedalboard into the common graph model.  Unlike an Ingen export,
/// a pedalboard only stores instance port symbols, so indices are resolved from
/// the Pico plugin bundles that will be placed in the image.
pub fn load(graph_file: &str, search_path: &str) -> Result<SourceGraph, String> {
    let triples = turtle::parse_mod(graph_file, "MOD pedalboard")?;
    let mut nodes = Vec::new();
    for triple in &triples {
        if triple.predicate != RDF_TYPE || triple.object != INGEN_BLOCK {
            continue;
        }
        if triple.subject.starts_with("_:") {
            return Err("MOD pedalboard block must have a URI".into());
        }
        if nodes
            .iter()
            .any(|node: &Node| node.subject == triple.subject)
        {
            continue;
        }
        if turtle::object_for(&triples, &triple.subject, INGEN_ENABLED) == Some("false") {
            return Err(format!(
                "MOD pedalboard block {} is disabled; bypassed blocks are not yet supported",
                instance_name(&triple.subject)
            ));
        }
        let prototype = turtle::object_for(&triples, &triple.subject, LV2_PROTOTYPE)
            .ok_or_else(|| format!("MOD block {} has no lv2:prototype", triple.subject))?;
        let plugin_ports = lv2::port_indices(prototype, search_path)?;
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
            let symbol = port_symbol(&triple.subject, port)?;
            if symbol == ":bypass" {
                let bypassed = turtle::object_for(&triples, port, INGEN_VALUE)
                    .map(|value| {
                        value.parse::<f32>().map(|value| value != 0.0).map_err(|_| {
                            format!("MOD bypass port {port} has invalid value {value}")
                        })
                    })
                    .transpose()?
                    .unwrap_or(false);
                if bypassed {
                    return Err(format!(
                        "MOD pedalboard block {} is bypassed; bypassed blocks are not yet supported",
                        instance_name(&triple.subject)
                    ));
                }
                continue;
            }
            let index = plugin_ports
                .iter()
                .find(|(candidate, _)| candidate == symbol)
                .map(|(_, index)| *index)
                .ok_or_else(|| {
                    format!(
                        "MOD block {} port {symbol} is not present in Pico plugin {prototype}",
                        instance_name(&triple.subject)
                    )
                })?;
            port_indices.push((port.clone(), index));
            if let Some(value) = turtle::object_for(&triples, port, INGEN_VALUE) {
                overrides.push((
                    index,
                    value
                        .parse::<f32>()
                        .map_err(|_| format!("MOD port {port} has invalid ingen:value {value}"))?,
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

    let mut arcs = triples
        .iter()
        .filter(|triple| triple.predicate == INGEN_HEAD)
        .filter_map(|triple| {
            turtle::object_for(&triples, &triple.subject, INGEN_TAIL)
                .map(|tail| (tail.to_string(), triple.object.clone()))
        })
        .collect::<Vec<_>>();
    arcs.sort();
    arcs.dedup();

    // MOD graph-level playback ports carry indices just like Ingen graph
    // outputs, so the common encoder can preserve their channel ordering.
    let mut outputs = Vec::new();
    for triple in &triples {
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
        let Some(index) = turtle::object_for(&triples, &triple.subject, LV2_INDEX) else {
            continue;
        };
        outputs.push((
            triple.subject.clone(),
            index
                .parse::<u8>()
                .map_err(|_| format!("MOD output {} has invalid lv2:index", triple.subject))?,
        ));
    }
    outputs.sort();
    outputs.dedup();
    Ok(SourceGraph {
        nodes,
        arcs,
        outputs,
    })
}

fn port_symbol<'a>(block: &str, port: &'a str) -> Result<&'a str, String> {
    port.strip_prefix(block)
        .and_then(|suffix| suffix.strip_prefix('/'))
        .ok_or_else(|| format!("MOD port {port} does not belong to block {block}"))
}

fn instance_name(subject: &str) -> &str {
    subject.rsplit('/').next().unwrap_or(subject)
}
