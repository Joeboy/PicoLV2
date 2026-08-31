use heapless::spsc::Queue;
use static_cell::StaticCell;

pub const MIDI_QUEUE_SIZE: usize = 256;
pub const MIDI_BLOCK_CAPACITY: usize = 64;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MidiEvent {
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
    pub _reserved: u8,
}

impl MidiEvent {
    pub const EMPTY: Self = Self {
        status: 0,
        data1: 0,
        data2: 0,
        _reserved: 0,
    };
}

#[repr(C)]
pub struct MidiEventBlock {
    pub events: *const MidiEvent,
    pub event_count: u32,
}

pub static MIDI_QUEUE: StaticCell<Queue<MidiEvent, MIDI_QUEUE_SIZE>> = StaticCell::new();
