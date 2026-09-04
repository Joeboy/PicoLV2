use std::{env, fs, io::BufReader, process::ExitCode};

use goblin::elf::{Elf, program_header::PT_LOAD};
use lv2_bundle_format::{Bundle, FLASH_ADDRESS, Graph, MAGIC, MAX_SIZE, VERSION};
use rio_api::{
    model::{Subject, Term},
    parser::TriplesParser,
};
use rio_turtle::TurtleParser;

const UF2_BLOCK_SIZE: usize = 512;
const UF2_PAYLOAD_SIZE: usize = 256;
const UF2_MAGIC_START0: u32 = 0x0a324655;
const UF2_MAGIC_START1: u32 = 0x9e5d5157;
const UF2_MAGIC_END: u32 = 0x0ab16f30;
const UF2_FLAG_FAMILY_ID_PRESENT: u32 = 0x00002000;
const RP2350_FAMILY_ID: u32 = 0xe48bff56;

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if arguments.first().map(String::as_str) == Some("uf2") {
        return uf2(&arguments[1..]);
    }
    if arguments.first().map(String::as_str) == Some("info") {
        return info(&arguments[1..]);
    }
    if arguments.first().map(String::as_str) != Some("pack") {
        eprintln!(
            "usage: lv2-bundle pack -o IMAGE (--firmware-elf ELF | --firmware-bin BIN) --ingen GRAPH.ttl --plugin URI BINARY TTL [...]"
        );
        eprintln!("       lv2-bundle uf2 -i IMAGE -o IMAGE.uf2");
        eprintln!("       lv2-bundle info -i IMAGE");
        return ExitCode::from(2);
    }

    let mut output = None;
    let mut firmware_elf_path = None;
    let mut firmware_bin_path = None;
    let mut graph_path = None;
    let mut plugins = Vec::new();
    let mut index = 1;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "-o" | "--output" => {
                index += 1;
                output = arguments.get(index).cloned();
            }
            "--firmware-elf" => {
                index += 1;
                firmware_elf_path = arguments.get(index).cloned();
            }
            "--firmware-bin" => {
                index += 1;
                firmware_bin_path = arguments.get(index).cloned();
            }
            "--graph" | "--ingen" => {
                index += 1;
                graph_path = arguments.get(index).cloned();
            }
            "--plugin" => {
                if index + 3 >= arguments.len() {
                    eprintln!("--plugin requires URI, binary path, and TTL path");
                    return ExitCode::from(2);
                }
                plugins.push((
                    arguments[index + 1].clone(),
                    arguments[index + 2].clone(),
                    arguments[index + 3].clone(),
                ));
                index += 3;
            }
            argument => {
                eprintln!("unknown argument: {argument}");
                return ExitCode::from(2);
            }
        }
        index += 1;
    }

    let (Some(output), Some(graph_path)) = (output, graph_path) else {
        eprintln!("missing output or graph path");
        return ExitCode::from(2);
    };
    let firmware = match (firmware_elf_path, firmware_bin_path) {
        (Some(_), Some(_)) => {
            eprintln!("--firmware-elf and --firmware-bin cannot be used together");
            return ExitCode::from(2);
        }
        (Some(path), None) => match firmware_from_elf(&path) {
            Ok(bytes) => bytes,
            Err(error) => return fail(&error),
        },
        (None, Some(path)) => match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => return fail(&format!("cannot read {path}: {error}")),
        },
        (None, None) => {
            eprintln!("missing --firmware-elf or --firmware-bin");
            return ExitCode::from(2);
        }
    };
    if plugins.is_empty() || plugins.len() > u32::MAX as usize {
        eprintln!("bundle must contain at least one plugin");
        return ExitCode::from(2);
    }

    let graph = match graph_path.ends_with(".ttl") {
        true => match ingen_graph(&graph_path) {
            Ok(bytes) => bytes,
            Err(error) => return fail(&error),
        },
        false => match fs::read(&graph_path) {
            Ok(bytes) => bytes,
            Err(error) => return fail(&format!("cannot read {graph_path}: {error}")),
        },
    };
    if Graph::parse(&graph).is_err() {
        return fail("invalid graph file");
    }
    let mut bundle = Vec::new();
    bundle.extend_from_slice(MAGIC);
    bundle.extend_from_slice(&VERSION.to_le_bytes());
    bundle.extend_from_slice(&(plugins.len() as u32).to_le_bytes());
    bundle.extend_from_slice(&(graph.len() as u32).to_le_bytes());
    let mut uris = Vec::new();
    for (uri, binary_path, metadata_path) in plugins {
        if uris.iter().any(|existing| existing == &uri) {
            return fail(&format!("duplicate plugin URI: {uri}"));
        }
        uris.push(uri.clone());
        let binary = match fs::read(&binary_path) {
            Ok(bytes) => bytes,
            Err(error) => return fail(&format!("cannot read {binary_path}: {error}")),
        };
        let metadata = match fs::read(&metadata_path) {
            Ok(bytes) => bytes,
            Err(error) => return fail(&format!("cannot read {metadata_path}: {error}")),
        };
        if uri.len() > u16::MAX as usize {
            return fail("plugin URI is too long");
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
        return fail(&format!(
            "bundle is {} bytes, maximum is {MAX_SIZE}",
            bundle.len()
        ));
    }

    let bundle_offset = FLASH_ADDRESS - 0x1000_0000;
    if firmware.len() > bundle_offset {
        return fail("firmware overlaps the reserved bundle region");
    }
    let mut image = vec![0xff; 2 * 1024 * 1024];
    image[..firmware.len()].copy_from_slice(&firmware);
    image[bundle_offset..bundle_offset + bundle.len()].copy_from_slice(&bundle);
    if let Err(error) = fs::write(&output, image) {
        return fail(&format!("cannot write {output}: {error}"));
    }
    println!("wrote {output}");
    ExitCode::SUCCESS
}

fn firmware_from_elf(path: &str) -> Result<Vec<u8>, String> {
    const FLASH_BASE: u64 = 0x1000_0000;

    let elf_bytes = fs::read(path).map_err(|error| format!("cannot read {path}: {error}"))?;
    let elf = Elf::parse(&elf_bytes).map_err(|error| format!("invalid firmware ELF: {error}"))?;
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

fn fail(message: &str) -> ExitCode {
    eprintln!("error: {message}");
    ExitCode::from(1)
}

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const INGEN_BLOCK: &str = "http://drobilla.net/ns/ingen#Block";
const INGEN_ARC: &str = "http://drobilla.net/ns/ingen#Arc";
const INGEN_TAIL: &str = "http://drobilla.net/ns/ingen#tail";
const INGEN_HEAD: &str = "http://drobilla.net/ns/ingen#head";
const LV2_PROTOTYPE: &str = "http://lv2plug.in/ns/lv2core#prototype";

fn ingen_graph(path: &str) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|error| format!("cannot read {path}: {error}"))?;
    let base = oxiri::Iri::parse(format!("file:{path}"))
        .map_err(|_| "invalid graph base URI".to_string())?;
    let mut triples = Vec::new();
    TurtleParser::new(BufReader::new(file), Some(base))
        .parse_all(&mut |triple| {
            triples.push(OwnedTriple {
                subject: subject_key(triple.subject),
                predicate: triple.predicate.iri.to_string(),
                object: term_key(triple.object),
            });
            Ok::<(), rio_turtle::TurtleError>(())
        })
        .map_err(|error| format!("invalid Ingen Turtle: {error}"))?;

    let mut nodes = Vec::new();
    for triple in &triples {
        if triple.predicate == RDF_TYPE && triple.object == INGEN_BLOCK {
            if triple.subject.starts_with("_:") {
                return Err("Ingen block must have a URI".into());
            }
            let node = &triple.subject;
            let prototype = triples
                .iter()
                .find(|candidate| {
                    candidate.subject == triple.subject && candidate.predicate == LV2_PROTOTYPE
                })
                .map(|candidate| candidate.object.as_str())
                .ok_or_else(|| format!("Ingen block {node} has no lv2:prototype"))?;
            nodes.push((node.to_string(), prototype.to_string()));
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
        let arc = triple.subject.as_str();
        let tail = object_for(&triples, arc, INGEN_TAIL)?;
        let head = object_for(&triples, arc, INGEN_HEAD)?;
        let source = node_for_port(&nodes, tail)?;
        let destination = node_for_port(&nodes, head)?;
        edges.push((source as u16, destination as u16));
    }

    let mut result = Vec::new();
    result.extend_from_slice(lv2_bundle_format::GRAPH_MAGIC);
    result.extend_from_slice(&lv2_bundle_format::GRAPH_VERSION.to_le_bytes());
    result.extend_from_slice(&(nodes.len() as u16).to_le_bytes());
    result.extend_from_slice(&(edges.len() as u16).to_le_bytes());
    for (_, prototype) in &nodes {
        result.extend_from_slice(&(prototype.len() as u16).to_le_bytes());
        result.extend_from_slice(&[0, 0]);
        result.extend_from_slice(prototype.as_bytes());
    }
    for (source, destination) in edges {
        result.extend_from_slice(&source.to_le_bytes());
        result.push(0);
        result.push(0);
        result.extend_from_slice(&destination.to_le_bytes());
        result.push(0);
        result.push(0);
    }
    Ok(result)
}

struct OwnedTriple {
    subject: String,
    predicate: String,
    object: String,
}

fn subject_key(subject: Subject<'_>) -> String {
    match subject {
        Subject::NamedNode(node) => node.iri.to_string(),
        Subject::BlankNode(node) => format!("_:{}", node.id),
        Subject::Triple(_) => String::new(),
    }
}

fn term_key(term: Term<'_>) -> String {
    match term {
        Term::NamedNode(node) => node.iri.to_string(),
        Term::BlankNode(node) => format!("_:{}", node.id),
        Term::Literal(literal) => literal.to_string(),
        Term::Triple(_) => String::new(),
    }
}

fn object_for<'a>(
    triples: &'a [OwnedTriple],
    subject: &str,
    predicate: &str,
) -> Result<&'a str, String> {
    triples
        .iter()
        .find(|triple| triple.subject == subject && triple.predicate == predicate)
        .map(|triple| triple.object.as_str())
        .ok_or_else(|| format!("Ingen arc missing {predicate}"))
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

fn uf2(arguments: &[String]) -> ExitCode {
    let mut input = None;
    let mut output = None;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "-i" | "--input" => {
                index += 1;
                input = arguments.get(index).cloned();
            }
            "-o" | "--output" => {
                index += 1;
                output = arguments.get(index).cloned();
            }
            argument => return fail(&format!("unknown argument: {argument}")),
        }
        index += 1;
    }
    let (Some(input), Some(output)) = (input, output) else {
        return fail("uf2 requires input and output paths");
    };
    let image = match fs::read(&input) {
        Ok(bytes) => bytes,
        Err(error) => return fail(&format!("cannot read {input}: {error}")),
    };
    if image.len() > 2 * 1024 * 1024 {
        return fail("flash image exceeds 2 MiB");
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
    match fs::write(&output, result) {
        Ok(()) => {
            println!("wrote {output} ({populated_blocks} blocks)");
            ExitCode::SUCCESS
        }
        Err(error) => fail(&format!("cannot write {output}: {error}")),
    }
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn info(arguments: &[String]) -> ExitCode {
    let mut input = None;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "-i" | "--input" => {
                index += 1;
                input = arguments.get(index).cloned();
            }
            argument => return fail(&format!("unknown argument: {argument}")),
        }
        index += 1;
    }
    let Some(input) = input else {
        return fail("info requires an input path");
    };
    let image = match fs::read(&input) {
        Ok(bytes) => bytes,
        Err(error) => return fail(&format!("cannot read {input}: {error}")),
    };
    let bundle_offset = FLASH_ADDRESS - 0x1000_0000;
    let Some(bundle_bytes) = image.get(bundle_offset..) else {
        return fail("image is smaller than the firmware region");
    };
    let bundle = match Bundle::parse(bundle_bytes) {
        Ok(bundle) => bundle,
        Err(error) => return fail(&format!("invalid bundle: {error:?}")),
    };

    let firmware_bytes = &image[..bundle_offset.min(image.len())];
    let firmware_size = firmware_bytes
        .iter()
        .rposition(|byte| *byte != 0xff)
        .map(|index| index + 1)
        .unwrap_or(0);
    println!("image: {input} ({} bytes)", image.len());
    println!(
        "firmware: {firmware_size} bytes (0x{:08x}..0x{:08x})",
        0x1000_0000,
        0x1000_0000 + firmware_size
    );
    println!(
        "bundle: {} bytes (0x{FLASH_ADDRESS:08x}..), format version {VERSION}",
        bundle_bytes.len()
    );
    println!("plugins: {}", bundle.plugin_count());
    for plugin_index in 0..bundle.plugin_count() {
        let entry = match bundle.entry_at(plugin_index) {
            Ok(entry) => entry,
            Err(error) => return fail(&format!("invalid plugin entry {plugin_index}: {error:?}")),
        };
        println!(
            "  [{plugin_index}] {} (binary {} bytes, metadata {} bytes)",
            String::from_utf8_lossy(entry.uri),
            entry.binary.len(),
            entry.metadata.len(),
        );
    }

    let graph = match bundle.graph() {
        Ok(graph) => graph,
        Err(error) => return fail(&format!("invalid graph: {error:?}")),
    };
    println!(
        "graph: {} nodes, {} edges",
        graph.node_count, graph.edge_count
    );
    for node_index in 0..graph.node_count {
        let Ok(node) = graph.node(node_index) else {
            return fail(&format!("invalid graph node {node_index}"));
        };
        println!("  node[{node_index}] {}", String::from_utf8_lossy(node.uri));
    }
    for edge_index in 0..graph.edge_count {
        let Ok(edge) = graph.edge(edge_index) else {
            return fail(&format!("invalid graph edge {edge_index}"));
        };
        println!(
            "  edge[{edge_index}] node[{}]:{} -> node[{}]:{}",
            edge.source_node, edge.source_port, edge.destination_node, edge.destination_port
        );
    }
    ExitCode::SUCCESS
}
