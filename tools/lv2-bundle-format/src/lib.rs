#![no_std]

pub const MAGIC: &[u8; 8] = b"PICO LV2";
pub const VERSION: u32 = 1;
pub const FLASH_ADDRESS: usize = 0x1018_0000;
pub const MAX_SIZE: usize = 512 * 1024;
const HEADER_SIZE: usize = 16;
const RECORD_SIZE: usize = 12;

#[cfg(test)]
extern crate std;

pub struct Bundle<'a> {
    bytes: &'a [u8],
    count: u32,
}

pub struct Entry<'a> {
    pub uri: &'a [u8],
    pub binary: &'a [u8],
    pub metadata: &'a [u8],
}

impl<'a> Bundle<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ()> {
        if bytes.len() < HEADER_SIZE || &bytes[..8] != MAGIC || read_u32(bytes, 8) != Some(VERSION)
        {
            return Err(());
        }
        let count = read_u32(bytes, 12).ok_or(())?;
        let bundle = Self { bytes, count };
        for index in 0..count {
            bundle.entry(index)?;
        }
        Ok(bundle)
    }

    pub fn find(&self, uri: &[u8]) -> Result<Entry<'a>, ()> {
        for index in 0..self.count {
            let entry = self.entry(index)?;
            if entry.uri == uri {
                return Ok(entry);
            }
        }
        Err(())
    }

    fn entry(&self, requested_index: u32) -> Result<Entry<'a>, ()> {
        let mut offset = HEADER_SIZE;
        for index in 0..self.count {
            let uri_length = read_u16(self.bytes, offset).ok_or(())? as usize;
            let binary_length = read_u32(self.bytes, offset + 4).ok_or(())? as usize;
            let metadata_length = read_u32(self.bytes, offset + 8).ok_or(())? as usize;
            offset = offset.checked_add(RECORD_SIZE).ok_or(())?;
            let uri_end = offset.checked_add(uri_length).ok_or(())?;
            let binary_end = uri_end.checked_add(binary_length).ok_or(())?;
            let metadata_end = binary_end.checked_add(metadata_length).ok_or(())?;
            if metadata_end > self.bytes.len() {
                return Err(());
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
        Err(())
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
        bytes.extend_from_slice(&3u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(b"uriBITTL");

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
}
