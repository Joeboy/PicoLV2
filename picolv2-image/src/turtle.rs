use std::{fs::File, io::BufReader, path::Path};

use rio_api::{
    model::{Literal, Subject, Term},
    parser::TriplesParser,
};
use rio_turtle::TurtleParser;

pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

pub struct Triple {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

pub fn parse(path: &str, kind: &str) -> Result<Vec<Triple>, String> {
    let file = File::open(path).map_err(|error| format!("cannot read {path}: {error}"))?;
    let absolute_path = Path::new(path)
        .canonicalize()
        .map_err(|error| format!("cannot resolve {path}: {error}"))?;
    let base = oxiri::Iri::parse(format!("file://{}", absolute_path.display()))
        .map_err(|_| format!("invalid {kind} base URI: {path}"))?;
    let mut triples = Vec::new();
    TurtleParser::new(BufReader::new(file), Some(base))
        .parse_all(&mut |triple| {
            triples.push(Triple {
                subject: subject_key(triple.subject),
                predicate: triple.predicate.iri.to_string(),
                object: term_key(triple.object),
            });
            Ok::<(), rio_turtle::TurtleError>(())
        })
        .map_err(|error| format!("invalid {kind} Turtle {path}: {error}"))?;
    Ok(triples)
}

pub fn object_for<'a>(triples: &'a [Triple], subject: &str, predicate: &str) -> Option<&'a str> {
    triples
        .iter()
        .find(|triple| triple.subject == subject && triple.predicate == predicate)
        .map(|triple| triple.object.as_str())
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
        Term::Literal(
            Literal::Simple { value }
            | Literal::LanguageTaggedString { value, .. }
            | Literal::Typed { value, .. },
        ) => value.to_string(),
        Term::Triple(_) => String::new(),
    }
}
