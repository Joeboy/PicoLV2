use core::ffi::{c_char, c_void};

pub const URID_MAP_URI: &[u8] = b"http://lv2plug.in/ns/ext/urid#map\0";
pub const ATOM_SEQUENCE_URI: &[u8] = b"http://lv2plug.in/ns/ext/atom#Sequence\0";
pub const MIDI_EVENT_URI: &[u8] = b"http://lv2plug.in/ns/ext/midi#MidiEvent\0";

pub const ATOM_SEQUENCE_URID: u32 = 1;
pub const MIDI_EVENT_URID: u32 = 2;

#[repr(C)]
pub struct Lv2Feature {
    pub uri: *const c_char,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct Lv2UridMap {
    pub handle: *mut c_void,
    pub map: extern "C" fn(*mut c_void, *const c_char) -> u32,
}

#[repr(C)]
pub struct Lv2Descriptor {
    pub uri: *const c_char,
    pub instantiate: extern "C" fn(
        descriptor: *const Lv2Descriptor,
        rate: f64,
        bundle_path: *const c_char,
        features: *const *const Lv2Feature,
    ) -> *mut c_void,
    pub connect_port: extern "C" fn(instance: *mut c_void, port: u32, data: *mut c_void),
    pub activate: extern "C" fn(instance: *mut c_void),
    pub run: extern "C" fn(instance: *mut c_void, n_samples: u32),
    pub deactivate: extern "C" fn(instance: *mut c_void),
    pub cleanup: extern "C" fn(instance: *mut c_void),
    pub extension_data: extern "C" fn(extension_data: *const c_char) -> *const c_void,
}
