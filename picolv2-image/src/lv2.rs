use std::path::Path;

use crate::turtle::{self, RDF_TYPE};
use picolv2_image_format::{METADATA_MAGIC, METADATA_VERSION, PortKind};

const RDFS_SEE_ALSO: &str = "http://www.w3.org/2000/01/rdf-schema#seeAlso";
const LV2_PORT: &str = "http://lv2plug.in/ns/lv2core#port";
const LV2_INDEX: &str = "http://lv2plug.in/ns/lv2core#index";
const LV2_DEFAULT: &str = "http://lv2plug.in/ns/lv2core#default";
const LV2_INPUT_PORT: &str = "http://lv2plug.in/ns/lv2core#InputPort";
const LV2_OUTPUT_PORT: &str = "http://lv2plug.in/ns/lv2core#OutputPort";
const LV2_AUDIO_PORT: &str = "http://lv2plug.in/ns/lv2core#AudioPort";
const LV2_CONTROL_PORT: &str = "http://lv2plug.in/ns/lv2core#ControlPort";
const ATOM_PORT: &str = "http://lv2plug.in/ns/ext/atom#AtomPort";

pub fn compile_metadata(plugin_uri: &str, manifest_path: &str) -> Result<Vec<u8>, String> {
    let manifest = turtle::parse(manifest_path, "plugin manifest")?;
    let metadata_uri =
        turtle::object_for(&manifest, plugin_uri, RDFS_SEE_ALSO).ok_or_else(|| {
            format!("plugin manifest {manifest_path} has no rdfs:seeAlso for {plugin_uri}")
        })?;
    let metadata_path = metadata_uri.strip_prefix("file:").ok_or_else(|| {
        format!("plugin manifest {manifest_path} has a non-local rdfs:seeAlso for {plugin_uri}")
    })?;
    let path = Path::new(metadata_path);
    let metadata_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(manifest_path)
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path)
    };
    let metadata_path = metadata_path.to_string_lossy();
    let triples = turtle::parse(&metadata_path, "plugin")?;
    let mut port_subjects: Vec<_> = triples
        .iter()
        .filter(|triple| triple.predicate == LV2_PORT)
        .map(|triple| triple.object.clone())
        .collect();
    port_subjects.sort();
    port_subjects.dedup();
    if port_subjects.is_empty() || port_subjects.len() > u16::MAX as usize {
        return Err(format!(
            "plugin metadata {metadata_path} has an invalid port count"
        ));
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
                "plugin metadata {metadata_path} has unsupported port {subject}"
            ));
        };
        let index = turtle::object_for(&triples, &subject, LV2_INDEX)
            .ok_or_else(|| format!("plugin metadata {metadata_path} port {subject} has no index"))?
            .parse::<u32>()
            .map_err(|_| {
                format!("plugin metadata {metadata_path} has invalid port index {subject}")
            })?;
        let default = turtle::object_for(&triples, &subject, LV2_DEFAULT)
            .map(|value| value.parse::<f32>())
            .transpose()
            .map_err(|_| {
                format!("plugin metadata {metadata_path} has invalid default for {subject}")
            })?;
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
