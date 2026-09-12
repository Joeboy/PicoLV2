use std::{fs, path::Path};

use goblin::elf::{program_header::PT_LOAD, Elf};
use picolv2_image_format::{Bundle, Graph, FLASH_ADDRESS, MAGIC, MAX_SIZE, VERSION};

use crate::{ingen, lv2};

pub const UF2_BLOCK_SIZE: usize = 512;
pub const UF2_PAYLOAD_SIZE: usize = 256;
pub const UF2_MAGIC_START0: u32 = 0x0a324655;
pub const UF2_MAGIC_START1: u32 = 0x9e5d5157;
pub const UF2_MAGIC_END: u32 = 0x0ab16f30;
pub const UF2_FLAG_FAMILY_ID_PRESENT: u32 = 0x00002000;
pub const RP2350_FAMILY_ID: u32 = 0xe48bff56;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Uf2,
    RawBinary,
}

pub struct CreateOptions<'a> {
    pub firmware_path: &'a Path,
    pub graph_path: &'a Path,
    pub search_path: &'a str,
    pub explicit_plugins: &'a [String],
}

pub struct CreateResult {
    pub image_bytes: Vec<u8>,
    pub firmware_size: usize,
    pub graph_nodes: usize,
    pub graph_edges: usize,
    pub plugins: Vec<String>,
    pub bundle_size: usize,
}

pub fn load_firmware(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    if bytes.is_empty() {
        return Err(format!("firmware file {} is empty", path.display()));
    }
    // Check for ELF magic: \x7fELF
    if bytes.starts_with(&[0x7f, b'E', b'L', b'F']) || path.extension().and_then(|e| e.to_str()) == Some("elf") {
        firmware_from_elf_bytes(&bytes, &path.display().to_string())
    } else {
        Ok(bytes)
    }
}

pub fn firmware_from_elf_bytes(elf_bytes: &[u8], display_path: &str) -> Result<Vec<u8>, String> {
    const FLASH_BASE: u64 = 0x1000_0000;

    let elf = Elf::parse(elf_bytes)
        .map_err(|error| format!("invalid firmware ELF {display_path}: {error}"))?;
    let mut firmware = Vec::new();
    for segment in &elf.program_headers {
        if segment.p_type != PT_LOAD || segment.p_filesz == 0 {
            continue;
        }
        let start = segment
            .p_paddr
            .checked_sub(FLASH_BASE)
            .ok_or_else(|| "firmware ELF contains a segment below flash".to_string())?;
        let end = start
            .checked_add(segment.p_filesz)
            .ok_or_else(|| "firmware ELF segment address overflows".to_string())?;
        let source_start = usize::try_from(segment.p_offset)
            .map_err(|_| "firmware ELF segment offset is too large".to_string())?;
        let source_end = source_start
            .checked_add(
                usize::try_from(segment.p_filesz)
                    .map_err(|_| "firmware ELF segment is too large".to_string())?,
            )
            .ok_or_else(|| "firmware ELF segment size overflows".to_string())?;
        let data = elf_bytes
            .get(source_start..source_end)
            .ok_or_else(|| "firmware ELF segment is outside the file".to_string())?;
        let end = usize::try_from(end)
            .map_err(|_| "firmware ELF segment address is too large".to_string())?;
        let start = usize::try_from(start)
            .map_err(|_| "firmware ELF segment address is too large".to_string())?;
        firmware.resize(end, 0);
        firmware[start..end].copy_from_slice(data);
    }
    if firmware.is_empty() {
        return Err("firmware ELF contains no loadable data".into());
    }
    Ok(firmware)
}

pub fn create_flash_image(options: &CreateOptions) -> Result<CreateResult, String> {
    let firmware = load_firmware(options.firmware_path)?;

    let graph_path_str = options.graph_path.to_string_lossy();
    let graph = ingen::compile(&graph_path_str)?;
    let parsed_graph = Graph::parse(&graph).map_err(|_| "invalid graph file".to_string())?;

    let mut plugins = options.explicit_plugins.to_vec();
    if plugins.is_empty() {
        for node_index in 0..parsed_graph.node_count {
            let node = parsed_graph
                .node(node_index)
                .map_err(|_| "invalid graph node".to_string())?;
            let uri = String::from_utf8(node.uri.to_vec())
                .map_err(|_| "graph node URI is not valid UTF-8".to_string())?;
            if !plugins.contains(&uri) {
                plugins.push(uri);
            }
        }
    }
    if plugins.is_empty() {
        return Err("bundle must contain at least one plugin".to_string());
    }
    if plugins.len() > u32::MAX as usize {
        return Err("too many plugins".to_string());
    }

    let mut bundle = Vec::new();
    bundle.extend_from_slice(MAGIC);
    bundle.extend_from_slice(&VERSION.to_le_bytes());
    bundle.extend_from_slice(&(plugins.len() as u32).to_le_bytes());
    bundle.extend_from_slice(&(graph.len() as u32).to_le_bytes());

    let mut uris = Vec::new();
    for uri in &plugins {
        if uris.iter().any(|existing| existing == uri) {
            return Err(format!("duplicate plugin URI: {uri}"));
        }
        uris.push(uri.clone());
        let (binary_path, manifest_path) = lv2::discover(uri, options.search_path)?;
        let binary = fs::read(&binary_path)
            .map_err(|error| format!("cannot read {binary_path}: {error}"))?;
        let metadata = lv2::compile_metadata(uri, &manifest_path)?;
        if uri.len() > u16::MAX as usize {
            return Err("plugin URI is too long".to_string());
        }
        bundle.extend_from_slice(&(uri.len() as u16).to_le_bytes());
        bundle.extend_from_slice(&[0, 0]);
        bundle.extend_from_slice(&(binary.len() as u32).to_le_bytes());
        bundle.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
        bundle.extend_from_slice(uri.as_bytes());
        bundle.extend_from_slice(&binary);
        bundle.extend_from_slice(&metadata);
    }
    bundle.extend_from_slice(&graph);
    if bundle.len() > MAX_SIZE {
        return Err(format!(
            "bundle is {} bytes, maximum is {MAX_SIZE}",
            bundle.len()
        ));
    }

    let bundle_offset = FLASH_ADDRESS - 0x1000_0000;
    if firmware.len() > bundle_offset {
        return Err("firmware overlaps the reserved bundle region".to_string());
    }
    let mut image = vec![0xff; 2 * 1024 * 1024];
    image[..firmware.len()].copy_from_slice(&firmware);
    image[bundle_offset..bundle_offset + bundle.len()].copy_from_slice(&bundle);

    Ok(CreateResult {
        image_bytes: image,
        firmware_size: firmware.len(),
        graph_nodes: parsed_graph.node_count as usize,
        graph_edges: parsed_graph.edge_count as usize,
        plugins,
        bundle_size: bundle.len(),
    })
}

pub fn image_to_uf2(image: &[u8]) -> Result<Vec<u8>, String> {
    if image.len() > 2 * 1024 * 1024 {
        return Err("flash image exceeds 2 MiB".to_string());
    }

    let populated_blocks = image
        .chunks(UF2_PAYLOAD_SIZE)
        .filter(|chunk| chunk.iter().any(|byte| *byte != 0xff))
        .count();
    let mut result = Vec::with_capacity(populated_blocks * UF2_BLOCK_SIZE);
    let mut block_number = 0;
    for (chunk_index, chunk) in image.chunks(UF2_PAYLOAD_SIZE).enumerate() {
        if chunk.iter().all(|byte| *byte == 0xff) {
            continue;
        }
        let address = 0x1000_0000u32 + (chunk_index * UF2_PAYLOAD_SIZE) as u32;
        let mut block = [0u8; UF2_BLOCK_SIZE];
        write_u32(&mut block, 0, UF2_MAGIC_START0);
        write_u32(&mut block, 4, UF2_MAGIC_START1);
        write_u32(&mut block, 8, UF2_FLAG_FAMILY_ID_PRESENT);
        write_u32(&mut block, 12, address);
        write_u32(&mut block, 16, UF2_PAYLOAD_SIZE as u32);
        write_u32(&mut block, 20, block_number);
        write_u32(&mut block, 24, populated_blocks as u32);
        write_u32(&mut block, 28, RP2350_FAMILY_ID);
        block[32..32 + chunk.len()].copy_from_slice(chunk);
        write_u32(&mut block, 508, UF2_MAGIC_END);
        result.extend_from_slice(&block);
        block_number += 1;
    }
    Ok(result)
}

pub fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

pub fn image_info(image_bytes: &[u8], path_display: &str) -> Result<String, String> {
    let bundle_offset = FLASH_ADDRESS - 0x1000_0000;
    let bundle_bytes = image_bytes
        .get(bundle_offset..)
        .ok_or_else(|| "image is smaller than the firmware region".to_string())?;
    let bundle = Bundle::parse(bundle_bytes)
        .map_err(|error| format!("invalid bundle: {error:?}"))?;

    let firmware_bytes = &image_bytes[..bundle_offset.min(image_bytes.len())];
    let firmware_size = firmware_bytes
        .iter()
        .rposition(|byte| *byte != 0xff)
        .map(|index| index + 1)
        .unwrap_or(0);

    let mut out = String::new();
    out.push_str(&format!("image: {path_display} ({} bytes)\n", image_bytes.len()));
    out.push_str(&format!(
        "firmware: {firmware_size} bytes (0x{:08x}..0x{:08x})\n",
        0x1000_0000,
        0x1000_0000 + firmware_size
    ));
    out.push_str(&format!(
        "bundle: {} bytes (0x{FLASH_ADDRESS:08x}..), format version {VERSION}\n",
        bundle_bytes.len()
    ));
    out.push_str(&format!("plugins: {}\n", bundle.plugin_count()));
    for plugin_index in 0..bundle.plugin_count() {
        let entry = bundle
            .entry_at(plugin_index)
            .map_err(|error| format!("invalid plugin entry {plugin_index}: {error:?}"))?;
        out.push_str(&format!(
            "  [{plugin_index}] {} (binary {} bytes, metadata {} bytes)\n",
            String::from_utf8_lossy(entry.uri),
            entry.binary.len(),
            entry.metadata.len(),
        ));
    }

    let graph = bundle
        .graph()
        .map_err(|error| format!("invalid graph: {error:?}"))?;
    out.push_str(&format!(
        "graph: {} nodes, {} edges\n",
        graph.node_count, graph.edge_count
    ));
    for node_index in 0..graph.node_count {
        let node = graph
            .node(node_index)
            .map_err(|_| format!("invalid graph node {node_index}"))?;
        out.push_str(&format!("  node[{node_index}] {}\n", String::from_utf8_lossy(node.uri)));
    }
    for edge_index in 0..graph.edge_count {
        let edge = graph
            .edge(edge_index)
            .map_err(|_| format!("invalid graph edge {edge_index}"))?;
        out.push_str(&format!(
            "  edge[{edge_index}] node[{}]:{} -> node[{}]:{}\n",
            edge.source_node, edge.source_port, edge.destination_node, edge.destination_port
        ));
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_to_uf2_empty_fails_or_generates() {
        let image = vec![0xff; 2 * 1024 * 1024];
        let uf2 = image_to_uf2(&image).unwrap();
        // All 0xff chunks are skipped
        assert_eq!(uf2.len(), 0);

        let mut image_with_data = vec![0xff; 2 * 1024 * 1024];
        image_with_data[0] = 0x42;
        let uf2_with_data = image_to_uf2(&image_with_data).unwrap();
        assert_eq!(uf2_with_data.len(), UF2_BLOCK_SIZE);
        assert_eq!(
            u32::from_le_bytes(uf2_with_data[0..4].try_into().unwrap()),
            UF2_MAGIC_START0
        );
        assert_eq!(
            u32::from_le_bytes(uf2_with_data[4..8].try_into().unwrap()),
            UF2_MAGIC_START1
        );
        assert_eq!(
            u32::from_le_bytes(uf2_with_data[28..32].try_into().unwrap()),
            RP2350_FAMILY_ID
        );
        assert_eq!(
            u32::from_le_bytes(uf2_with_data[508..512].try_into().unwrap()),
            UF2_MAGIC_END
        );
    }
}
