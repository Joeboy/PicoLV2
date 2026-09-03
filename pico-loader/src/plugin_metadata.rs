const MAX_PORTS: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PortKind {
    AudioInput,
    AudioOutput,
    ControlInput,
    AtomInput,
    Unknown,
}

#[derive(Clone, Copy)]
pub struct PortMetadata {
    pub index: u32,
    pub kind: PortKind,
    pub default: Option<f32>,
}

pub struct PluginMetadata {
    ports: [PortMetadata; MAX_PORTS],
    port_count: usize,
}

impl PluginMetadata {
    pub fn parse(bytes: &[u8]) -> Result<Self, ()> {
        let empty = PortMetadata { index: 0, kind: PortKind::Unknown, default: None };
        let mut metadata = Self { ports: [empty; MAX_PORTS], port_count: 0 };
        let mut cursor = Cursor::new(bytes);

        while let Some(token) = cursor.next() {
            if token == b"lv2:port" {
                if cursor.next() != Some(b"[") {
                    return Err(());
                }
            } else if token != b"[" {
                continue;
            }
            if metadata.port_count == MAX_PORTS {
                return Err(());
            }
            let mut port = empty;
            let mut saw_index = false;
            while let Some(predicate) = cursor.next() {
                if predicate == b"]" {
                    break;
                }
                if predicate == b";" || predicate == b"," {
                    continue;
                }
                if matches!(predicate, b"atom:AtomPort" | b"lv2:AudioPort" | b"lv2:ControlPort") {
                    port.kind = kind_for(port.kind, predicate);
                    continue;
                }
                let value = cursor.next().ok_or(())?;
                match predicate {
                    b"a" => port.kind = kind_for(port.kind, value),
                    b"lv2:index" => {
                        port.index = parse_u32(value).ok_or(())?;
                        saw_index = true;
                    }
                    b"lv2:default" => port.default = Some(parse_f32(value).ok_or(())?),
                    _ => {}
                }
            }
            if !saw_index {
                return Err(());
            }
            metadata.ports[metadata.port_count] = port;
            metadata.port_count += 1;
        }

        if metadata.port_count == 0 { return Err(()); }
        Ok(metadata)
    }

    pub fn port(&self, kind: PortKind, occurrence: usize) -> Option<PortMetadata> {
        let mut found = 0;
        for port in &self.ports[..self.port_count] {
            if port.kind == kind {
                if found == occurrence { return Some(*port); }
                found += 1;
            }
        }
        None
    }
}

fn kind_for(current: PortKind, value: &[u8]) -> PortKind {
    match value {
        b"lv2:InputPort" => PortKind::ControlInput,
        b"lv2:OutputPort" => PortKind::AudioOutput,
        b"atom:AtomPort" => PortKind::AtomInput,
        b"lv2:AudioPort" if current == PortKind::AudioOutput => PortKind::AudioOutput,
        b"lv2:AudioPort" => PortKind::AudioInput,
        b"lv2:ControlPort" => PortKind::ControlInput,
        _ => current,
    }
}

struct Cursor<'a> { bytes: &'a [u8], position: usize }

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self { Self { bytes, position: 0 } }

    fn next(&mut self) -> Option<&'a [u8]> {
        while self.position < self.bytes.len() {
            match self.bytes[self.position] {
                b' ' | b'\t' | b'\r' | b'\n' => self.position += 1,
                b'#' => while self.position < self.bytes.len() && self.bytes[self.position] != b'\n' { self.position += 1 },
                _ => break,
            }
        }
        if self.position == self.bytes.len() { return None; }
        let start = self.position;
        match self.bytes[self.position] {
            b'[' | b']' | b';' | b',' => { self.position += 1; Some(&self.bytes[start..self.position]) }
            b'"' => {
                self.position += 1;
                while self.position < self.bytes.len() {
                    let byte = self.bytes[self.position];
                    self.position += 1;
                    if byte == b'"' { break; }
                    if byte == b'\\' { self.position = self.position.saturating_add(1); }
                }
                Some(&self.bytes[start..self.position])
            }
            _ => {
                while self.position < self.bytes.len() && !matches!(self.bytes[self.position], b' ' | b'\t' | b'\r' | b'\n' | b'[' | b']' | b';' | b',') { self.position += 1; }
                Some(&self.bytes[start..self.position])
            }
        }
    }
}

fn parse_u32(bytes: &[u8]) -> Option<u32> {
    let mut value: u32 = 0;
    for byte in bytes {
        if !byte.is_ascii_digit() { return None; }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as u32)?;
    }
    Some(value)
}

fn parse_f32(bytes: &[u8]) -> Option<f32> {
    let mut value = 0.0;
    let mut divisor = 1.0;
    let mut fraction = false;
    for byte in bytes {
        if *byte == b'.' {
            if fraction { return None; }
            fraction = true;
            continue;
        }
        if !byte.is_ascii_digit() { return None; }
        if fraction { divisor *= 10.0; value += (*byte - b'0') as f32 / divisor; }
        else { value = value * 10.0 + (*byte - b'0') as f32; }
    }
    Some(value)
}