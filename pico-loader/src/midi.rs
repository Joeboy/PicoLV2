use heapless::spsc::Queue;
use static_cell::StaticCell;

use crate::lv2::{ATOM_SEQUENCE_URID, MIDI_EVENT_URID};

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

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Lv2Atom {
    pub size: u32,
    pub atom_type: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Lv2AtomSequenceBody {
    pub unit: u32,
    pub pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Lv2MidiEvent {
    pub frame: i64,
    pub body: Lv2Atom,
    pub message: [u8; 3],
    pub padding: [u8; 5],
}

impl Lv2MidiEvent {
    pub const EMPTY: Self = Self {
        frame: 0,
        body: Lv2Atom {
            size: 3,
            atom_type: MIDI_EVENT_URID,
        },
        message: [0; 3],
        padding: [0; 5],
    };
}

#[repr(C)]
pub struct Lv2MidiSequence {
    pub atom: Lv2Atom,
    pub body: Lv2AtomSequenceBody,
    pub events: [Lv2MidiEvent; MIDI_BLOCK_CAPACITY],
}

impl Lv2MidiSequence {
    pub const fn empty() -> Self {
        Self {
            atom: Lv2Atom {
                size: core::mem::size_of::<Lv2AtomSequenceBody>() as u32,
                atom_type: ATOM_SEQUENCE_URID,
            },
            body: Lv2AtomSequenceBody { unit: 0, pad: 0 },
            events: [Lv2MidiEvent::EMPTY; MIDI_BLOCK_CAPACITY],
        }
    }

    pub fn set_event_count(&mut self, event_count: usize) {
        self.atom.size = (core::mem::size_of::<Lv2AtomSequenceBody>()
            + event_count * core::mem::size_of::<Lv2MidiEvent>()) as u32;
    }
}

pub static MIDI_QUEUE: StaticCell<Queue<MidiEvent, MIDI_QUEUE_SIZE>> = StaticCell::new();
