use heapless::spsc::Queue;
use static_cell::StaticCell;

pub const MIDI_QUEUE_SIZE: usize = 256;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MidiEvent {
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
    pub _reserved: u8,
    pub timestamp_micros: u64,
}

pub static MIDI_QUEUE: StaticCell<Queue<MidiEvent, MIDI_QUEUE_SIZE>> = StaticCell::new();
