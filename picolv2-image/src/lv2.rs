use crate::turtle::{self, RDF_TYPE};
use picolv2_image_format::{METADATA_MAGIC, METADATA_VERSION, PortKind};

const LV2_PORT: &str = "http://lv2plug.in/ns/lv2core#port";
const LV2_INDEX: &str = "http://lv2plug.in/ns/lv2core#index";
const LV2_DEFAULT: &str = "http://lv2plug.in/ns/lv2core#default";
const LV2_INPUT_PORT: &str = "http://lv2plug.in/ns/lv2core#InputPort";
const LV2_OUTPUT_PORT: &str = "http://lv2plug.in/ns/lv2core#OutputPort";
const LV2_AUDIO_PORT: &str = "http://lv2plug.in/ns/lv2core#AudioPort";
const LV2_CONTROL_PORT: &str = "http://lv2plug.in/ns/lv2core#ControlPort";
const ATOM_PORT: &str = "http://lv2plug.in/ns/ext/atom#AtomPort";

pub fn compile_metadata(path: &str) -> Result<Vec<u8>, String> {
    let triples = turtle::parse(path, "plugin")?;
    let mut port_subjects: Vec<_> = triples
        .iter()
        .filter(|triple| triple.predicate == LV2_PORT)
        .map(|triple| triple.object.clone())
        .collect();
    port_subjects.sort();
    port_subjects.dedup();
    if port_subjects.is_empty() || port_subjects.len() > u16::MAX as usize {
        return Err(format!("plugin metadata {path} has an invalid port count"));
    }

    let mut ports = Vec::new();
    for subject in port_subjects {
        let types: Vec<_> = triples
            .iter()
            .filter(|triple| triple.subject == subject && triple.predicate == RDF_TYPE)
            .map(|triple| triple.object.as_str())
            .collect();
        let kind = if types.contains(&ATOM_PORT) && types.contains(&LV2_INPUT_PORT) {
            PortKind::AtomInput
        } else if types.contains(&LV2_AUDIO_PORT) && types.contains(&LV2_INPUT_PORT) {
            PortKind::AudioInput
        } else if types.contains(&LV2_AUDIO_PORT) && types.contains(&LV2_OUTPUT_PORT) {
            PortKind::AudioOutput
        } else if types.contains(&LV2_CONTROL_PORT) && types.contains(&LV2_INPUT_PORT) {
            PortKind::ControlInput
        } else {
            return Err(format!(
                "plugin metadata {path} has unsupported port {subject}"
            ));
        };
        let index = turtle::object_for(&triples, &subject, LV2_INDEX)
            .ok_or_else(|| format!("plugin metadata {path} port {subject} has no index"))?
            .parse::<u32>()
            .map_err(|_| format!("plugin metadata {path} has invalid port index {subject}"))?;
        let default = turtle::object_for(&triples, &subject, LV2_DEFAULT)
            .map(|value| value.parse::<f32>())
            .transpose()
            .map_err(|_| format!("plugin metadata {path} has invalid default for {subject}"))?;
        ports.push((index, kind, default));
    }
    ports.sort_by_key(|(index, _, _)| *index);

    let mut result = Vec::with_capacity(16 + ports.len() * 12);
    result.extend_from_slice(METADATA_MAGIC);
    result.extend_from_slice(&METADATA_VERSION.to_le_bytes());
    result.extend_from_slice(&(ports.len() as u16).to_le_bytes());
    result.extend_from_slice(&[0, 0]);
    for (index, kind, default) in ports {
        result.push(kind as u8);
        result.push(default.is_some() as u8);
        result.extend_from_slice(&[0, 0]);
        result.extend_from_slice(&index.to_le_bytes());
        result.extend_from_slice(&default.unwrap_or(0.0).to_le_bytes());
    }
    Ok(result)
}
