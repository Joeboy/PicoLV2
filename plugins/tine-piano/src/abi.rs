use core::ffi::{c_char, c_void};

pub type Lv2Handle = *mut c_void;
pub const PLUGIN_URI: &[u8] = b"https://joebutton.co.uk/lv2/tine-piano\0";
pub const URID_MAP_URI: &[u8] = b"http://lv2plug.in/ns/ext/urid#map\0";
pub const ATOM_SEQUENCE_URI: &[u8] = b"http://lv2plug.in/ns/ext/atom#Sequence\0";
pub const MIDI_EVENT_URI: &[u8] = b"http://lv2plug.in/ns/ext/midi#MidiEvent\0";

#[repr(C)] pub struct Lv2Feature { pub uri: *const c_char, pub data: *mut c_void }
#[repr(C)] pub struct Lv2UridMap { pub handle: *mut c_void, pub map: extern "C" fn(*mut c_void, *const c_char) -> u32 }
#[repr(C)] pub struct Lv2Descriptor {
    pub uri: *const c_char,
    pub instantiate: extern "C" fn(*const Lv2Descriptor, f64, *const c_char, *const *const Lv2Feature) -> Lv2Handle,
    pub connect_port: extern "C" fn(Lv2Handle, u32, *mut c_void),
    pub activate: extern "C" fn(Lv2Handle),
    pub run: extern "C" fn(Lv2Handle, u32),
    pub deactivate: extern "C" fn(Lv2Handle),
    pub cleanup: extern "C" fn(Lv2Handle),
    pub extension_data: extern "C" fn(*const c_char) -> *const c_void,
}
unsafe impl Sync for Lv2Descriptor {}
#[repr(C)] pub struct Lv2Atom { pub size: u32, pub atom_type: u32 }
#[repr(C)] pub struct Lv2AtomSequenceBody { pub unit: u32, pub pad: u32 }
#[repr(C)] pub struct Lv2AtomSequence { pub atom: Lv2Atom, pub body: Lv2AtomSequenceBody }
#[repr(C)] pub union Lv2AtomEventTime { pub frames: i64, pub beats: f64 }
#[repr(C)] pub struct Lv2AtomEvent { pub time: Lv2AtomEventTime, pub body: Lv2Atom }
#[inline] pub const fn pad_size(size: u32) -> u32 { (size + 7) & !7 }