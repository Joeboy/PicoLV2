use core::ffi::{c_char, c_void};

mod atom_sequence;
mod host_abi;
mod runtime;

pub(super) use atom_sequence::{Lv2AtomSequence, Lv2AtomSequenceBody, MIDI_BLOCK_CAPACITY};
pub(super) use runtime::{PluginBinary, PluginInstance, features_ptr};

pub const URID_MAP_URI: &[u8] = b"http://lv2plug.in/ns/ext/urid#map\0";
pub const ATOM_SEQUENCE_URI: &[u8] = b"http://lv2plug.in/ns/ext/atom#Sequence\0";
pub const MIDI_EVENT_URI: &[u8] = b"http://lv2plug.in/ns/ext/midi#MidiEvent\0";
pub const ATOM_BLANK_URI: &[u8] = b"http://lv2plug.in/ns/ext/atom#Blank\0";
pub const ATOM_OBJECT_URI: &[u8] = b"http://lv2plug.in/ns/ext/atom#Object\0";
pub const ATOM_DOUBLE_URI: &[u8] = b"http://lv2plug.in/ns/ext/atom#Double\0";
pub const ATOM_FLOAT_URI: &[u8] = b"http://lv2plug.in/ns/ext/atom#Float\0";
pub const ATOM_INT_URI: &[u8] = b"http://lv2plug.in/ns/ext/atom#Int\0";
pub const ATOM_LONG_URI: &[u8] = b"http://lv2plug.in/ns/ext/atom#Long\0";
pub const TIME_POSITION_URI: &[u8] = b"http://lv2plug.in/ns/ext/time#Position\0";
pub const TIME_BAR_URI: &[u8] = b"http://lv2plug.in/ns/ext/time#bar\0";
pub const TIME_BAR_BEAT_URI: &[u8] = b"http://lv2plug.in/ns/ext/time#barBeat\0";
pub const TIME_BEATS_PER_BAR_URI: &[u8] = b"http://lv2plug.in/ns/ext/time#beatsPerBar\0";
pub const TIME_BEATS_PER_MINUTE_URI: &[u8] = b"http://lv2plug.in/ns/ext/time#beatsPerMinute\0";
pub const TIME_BEAT_UNIT_URI: &[u8] = b"http://lv2plug.in/ns/ext/time#beatUnit\0";
pub const TIME_FRAME_URI: &[u8] = b"http://lv2plug.in/ns/ext/time#frame\0";
pub const TIME_SPEED_URI: &[u8] = b"http://lv2plug.in/ns/ext/time#speed\0";

pub const ATOM_SEQUENCE_URID: u32 = 1;
pub const MIDI_EVENT_URID: u32 = 2;
pub const ATOM_BLANK_URID: u32 = 3;
pub const ATOM_OBJECT_URID: u32 = 4;
pub const ATOM_DOUBLE_URID: u32 = 5;
pub const ATOM_FLOAT_URID: u32 = 6;
pub const ATOM_INT_URID: u32 = 7;
pub const ATOM_LONG_URID: u32 = 8;
pub const TIME_POSITION_URID: u32 = 9;
pub const TIME_BAR_URID: u32 = 10;
pub const TIME_BAR_BEAT_URID: u32 = 11;
pub const TIME_BEATS_PER_BAR_URID: u32 = 12;
pub const TIME_BEATS_PER_MINUTE_URID: u32 = 13;
pub const TIME_BEAT_UNIT_URID: u32 = 14;
pub const TIME_FRAME_URID: u32 = 15;
pub const TIME_SPEED_URID: u32 = 16;

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
    pub activate: Option<extern "C" fn(instance: *mut c_void)>,
    pub run: extern "C" fn(instance: *mut c_void, n_samples: u32),
    pub deactivate: Option<extern "C" fn(instance: *mut c_void)>,
    pub cleanup: extern "C" fn(instance: *mut c_void),
    pub extension_data: Option<extern "C" fn(extension_data: *const c_char) -> *const c_void>,
}
