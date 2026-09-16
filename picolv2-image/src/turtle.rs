use std::{fs, path::Path};

use oxrdf::{NamedOrBlankNode, Term};
use oxttl::TurtleParser;

pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

pub struct Triple {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

pub fn parse(path: &str, kind: &str) -> Result<Vec<Triple>, String> {
    parse_with(path, kind, |source| source.to_string())
}

/// MOD pedalboards use root-relative identifiers such as `<:bpm>`.  They are
/// accepted by MOD's tooling but are not valid RFC 3987 relative IRIs, so make
/// them explicitly relative before handing the document to the strict parser.
pub fn parse_mod(path: &str, kind: &str) -> Result<Vec<Triple>, String> {
    parse_with(path, kind, |source| source.replace("<:", "<./:"))
}

fn parse_with(
    path: &str,
    kind: &str,
    transform: impl FnOnce(&str) -> String,
) -> Result<Vec<Triple>, String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("cannot read {path}: {error}"))?;
    let absolute_path = Path::new(path)
        .canonicalize()
        .map_err(|error| format!("cannot resolve {path}: {error}"))?;
    let encoded_path = percent_encode_path(&absolute_path.to_string_lossy());
    let parser = TurtleParser::new()
        .with_base_iri(format!("file://{encoded_path}"))
        .map_err(|_| format!("invalid {kind} base URI: {path}"))?;
    let source = transform(&source);
    let mut triples = Vec::new();
    for triple in parser.for_reader(source.as_bytes()) {
        let triple = triple.map_err(|error| format!("invalid {kind} Turtle {path}: {error}"))?;
        triples.push(Triple {
            subject: subject_key(&triple.subject),
            predicate: triple.predicate.as_str().to_string(),
            object: term_key(&triple.object),
        });
    }
    Ok(triples)
}

fn percent_encode_path(path: &str) -> String {
    let mut result = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':') {
            result.push(byte as char);
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}

pub fn object_for<'a>(triples: &'a [Triple], subject: &str, predicate: &str) -> Option<&'a str> {
    triples
        .iter()
        .find(|triple| triple.subject == subject && triple.predicate == predicate)
        .map(|triple| triple.object.as_str())
}

fn subject_key(subject: &NamedOrBlankNode) -> String {
    match subject {
        NamedOrBlankNode::NamedNode(node) => node.as_str().to_string(),
        NamedOrBlankNode::BlankNode(node) => format!("_:{}", node.as_str()),
    }
}

fn term_key(term: &Term) -> String {
    match term {
        Term::NamedNode(node) => node.as_str().to_string(),
        Term::BlankNode(node) => format!("_:{}", node.as_str()),
        Term::Literal(literal) => literal.value().to_string(),
    }
}
