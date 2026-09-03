#![no_std]

pub const MAGIC: &[u8; 8] = b"PICO LV2";
pub const VERSION: u32 = 2;
pub const FLASH_ADDRESS: usize = 0x1018_0000;
pub const MAX_SIZE: usize = 512 * 1024;
pub const GRAPH_MAGIC: &[u8; 8] = b"PICO GRP";
pub const GRAPH_VERSION: u32 = 1;
const HEADER_SIZE: usize = 20;
const RECORD_SIZE: usize = 12;

#[cfg(test)]
extern crate std;

pub struct Bundle<'a> {
    bytes: &'a [u8],
    count: u32,
    graph_length: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BundleError {
    Header,
    Truncated,
    GraphLength,
    Graph,
    NotFound,
}

pub struct Entry<'a> {
    pub uri: &'a [u8],
    pub binary: &'a [u8],
    pub metadata: &'a [u8],
}

pub struct Graph<'a> {
    bytes: &'a [u8],
    pub node_count: u16,
    pub edge_count: u16,
}

#[derive(Clone, Copy)]
pub struct Node<'a> {
    pub uri: &'a [u8],
}

#[derive(Clone, Copy)]
pub struct Edge {
    pub source_node: u16,
    pub source_port: u8,
    pub destination_node: u16,
    pub destination_port: u8,
}

impl<'a> Bundle<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, BundleError> {
        if bytes.len() < HEADER_SIZE || &bytes[..8] != MAGIC || read_u32(bytes, 8) != Some(VERSION)
        {
            return Err(BundleError::Header);
        }
        let count = read_u32(bytes, 12).ok_or(BundleError::Header)?;
        let graph_length = read_u32(bytes, 16).ok_or(BundleError::Header)? as usize;
        let bundle = Self {
            bytes,
            count,
            graph_length,
        };
        for index in 0..count {
            bundle.entry(index)?;
        }
        if bundle.graph_bytes()?.len() != graph_length {
            return Err(BundleError::GraphLength);
        }
        Ok(bundle)
    }

    pub fn find(&self, uri: &[u8]) -> Result<Entry<'a>, BundleError> {
        for index in 0..self.count {
            let entry = self.entry(index)?;
            if entry.uri == uri {
                return Ok(entry);
            }
        }
        Err(BundleError::NotFound)
    }

    pub fn graph(&self) -> Result<Graph<'a>, BundleError> {
        Graph::parse(self.graph_bytes()?).map_err(|_| BundleError::Graph)
    }

    fn graph_bytes(&self) -> Result<&'a [u8], BundleError> {
        let mut offset = HEADER_SIZE;
        for index in 0..self.count {
            let uri_length = read_u16(self.bytes, offset).ok_or(BundleError::Truncated)? as usize;
            let binary_length =
                read_u32(self.bytes, offset + 4).ok_or(BundleError::Truncated)? as usize;
            let metadata_length =
                read_u32(self.bytes, offset + 8).ok_or(BundleError::Truncated)? as usize;
            offset = offset
                .checked_add(RECORD_SIZE)
                .ok_or(BundleError::Truncated)?;
            offset = offset
                .checked_add(uri_length)
                .ok_or(BundleError::Truncated)?
                .checked_add(binary_length)
                .ok_or(BundleError::Truncated)?
                .checked_add(metadata_length)
                .ok_or(BundleError::Truncated)?;
            let _ = index;
        }
        let end = offset
            .checked_add(self.graph_length)
            .ok_or(BundleError::GraphLength)?;
        self.bytes.get(offset..end).ok_or(BundleError::GraphLength)
    }

    fn entry(&self, requested_index: u32) -> Result<Entry<'a>, BundleError> {
        let mut offset = HEADER_SIZE;
        for index in 0..self.count {
            let uri_length = read_u16(self.bytes, offset).ok_or(BundleError::Truncated)? as usize;
            let binary_length =
                read_u32(self.bytes, offset + 4).ok_or(BundleError::Truncated)? as usize;
            let metadata_length =
                read_u32(self.bytes, offset + 8).ok_or(BundleError::Truncated)? as usize;
            offset = offset
                .checked_add(RECORD_SIZE)
                .ok_or(BundleError::Truncated)?;
            let uri_end = offset
                .checked_add(uri_length)
                .ok_or(BundleError::Truncated)?;
            let binary_end = uri_end
                .checked_add(binary_length)
                .ok_or(BundleError::Truncated)?;
            let metadata_end = binary_end
                .checked_add(metadata_length)
                .ok_or(BundleError::Truncated)?;
            if metadata_end > self.bytes.len() {
                return Err(BundleError::Truncated);
            }
            if index == requested_index {
                return Ok(Entry {
                    uri: &self.bytes[offset..uri_end],
                    binary: &self.bytes[uri_end..binary_end],
                    metadata: &self.bytes[binary_end..metadata_end],
                });
            }
            offset = metadata_end;
        }
        Err(BundleError::Truncated)
    }
}

impl<'a> Graph<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ()> {
        if bytes.len() < 16
            || &bytes[..8] != GRAPH_MAGIC
            || read_u32(bytes, 8) != Some(GRAPH_VERSION)
        {
            return Err(());
        }
        let graph = Self {
            bytes,
            node_count: read_u16(bytes, 12).ok_or(())?,
            edge_count: read_u16(bytes, 14).ok_or(())?,
        };
        let mut offset = 16usize;
        for _ in 0..graph.node_count {
            let uri_length = read_u16(bytes, offset).ok_or(())? as usize;
            offset = offset
                .checked_add(4)
                .ok_or(())?
                .checked_add(uri_length)
                .ok_or(())?;
        }
        offset = offset
            .checked_add(graph.edge_count as usize * 8)
            .ok_or(())?;
        if offset != bytes.len() {
            return Err(());
        }
        Ok(graph)
    }

    pub fn node(&self, requested_index: u16) -> Result<Node<'a>, ()> {
        let mut offset = 16usize;
        for index in 0..self.node_count {
            let uri_length = read_u16(self.bytes, offset).ok_or(())? as usize;
            let uri_start = offset.checked_add(4).ok_or(())?;
            let uri_end = uri_start.checked_add(uri_length).ok_or(())?;
            if index == requested_index {
                return Ok(Node {
                    uri: &self.bytes[uri_start..uri_end],
                });
            }
            offset = uri_end;
        }
        Err(())
    }

    pub fn edge(&self, requested_index: u16) -> Result<Edge, ()> {
        let mut offset = 16usize;
        for _ in 0..self.node_count {
            let uri_length = read_u16(self.bytes, offset).ok_or(())? as usize;
            offset = offset
                .checked_add(4)
                .ok_or(())?
                .checked_add(uri_length)
                .ok_or(())?;
        }
        let edge_offset = offset.checked_add(requested_index as usize * 8).ok_or(())?;
        Ok(Edge {
            source_node: read_u16(self.bytes, edge_offset).ok_or(())?,
            source_port: *self.bytes.get(edge_offset + 2).ok_or(())?,
            destination_node: read_u16(self.bytes, edge_offset + 4).ok_or(())?,
            destination_port: *self.bytes.get(edge_offset + 6).ok_or(())?,
        })
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes([
        *bytes.get(offset)?,
        *bytes.get(offset + 1)?,
    ]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *bytes.get(offset)?,
        *bytes.get(offset + 1)?,
        *bytes.get(offset + 2)?,
        *bytes.get(offset + 3)?,
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    #[test]
    fn parses_entries() {
        let mut bytes = Vec::from(*MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&3u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(b"uriBITTL");
        bytes.extend_from_slice(GRAPH_MAGIC);
        bytes.extend_from_slice(&GRAPH_VERSION.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());

        let entry = Bundle::parse(&bytes).unwrap().find(b"uri").unwrap();
        assert_eq!(entry.binary, b"BI");
        assert_eq!(entry.metadata, b"TTL");
    }

    #[test]
    fn rejects_truncated_entries() {
        let mut bytes = Vec::from(*MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        assert!(Bundle::parse(&bytes).is_err());
    }

    #[test]
    fn accepts_flash_padding_after_graph() {
        let mut bytes = Vec::from(*MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(GRAPH_MAGIC);
        bytes.extend_from_slice(&GRAPH_VERSION.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&[0xff; 128]);

        let bundle = Bundle::parse(&bytes).unwrap();
        assert_eq!(bundle.graph().unwrap().node_count, 0);
    }

    #[test]
    fn parses_edge_fields() {
        let mut bytes = Vec::from(*GRAPH_MAGIC);
        bytes.extend_from_slice(&GRAPH_VERSION.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        for uri in [b"source".as_slice(), b"destination".as_slice()] {
            bytes.extend_from_slice(&(uri.len() as u16).to_le_bytes());
            bytes.extend_from_slice(&[0, 0]);
            bytes.extend_from_slice(uri);
        }
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);

        let graph = Graph::parse(&bytes).unwrap();
        let edge = graph.edge(0).unwrap();
        assert_eq!(edge.source_node, 0);
        assert_eq!(edge.destination_node, 1);
        assert_eq!(edge.source_port, 0);
        assert_eq!(edge.destination_port, 0);
    }
}
