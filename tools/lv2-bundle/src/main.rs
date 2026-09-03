use std::{env, fs, io::BufReader, process::ExitCode};

use lv2_bundle_format::{FLASH_ADDRESS, Graph, MAGIC, MAX_SIZE, VERSION};
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
    if arguments.first().map(String::as_str) != Some("pack") {
        eprintln!(
            "usage: lv2-bundle pack -o IMAGE -f FIRMWARE --ingen GRAPH.ttl --plugin URI BINARY TTL [...]"
        );
        eprintln!("       lv2-bundle uf2 -i IMAGE -o IMAGE.uf2");
        return ExitCode::from(2);
    }

    let mut output = None;
    let mut firmware_path = None;
    let mut graph_path = None;
    let mut plugins = Vec::new();
    let mut index = 1;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "-o" | "--output" => {
                index += 1;
                output = arguments.get(index).cloned();
            }
            "-f" | "--firmware" => {
                index += 1;
                firmware_path = arguments.get(index).cloned();
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

    let (Some(output), Some(firmware_path), Some(graph_path)) =
        (output, firmware_path, graph_path)
    else {
        eprintln!("missing output, firmware, or graph path");
        return ExitCode::from(2);
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

    let firmware = match fs::read(&firmware_path) {
        Ok(bytes) => bytes,
        Err(error) => return fail(&format!("cannot read {firmware_path}: {error}")),
    };
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
