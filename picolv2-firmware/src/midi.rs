use heapless::spsc::Queue;
use static_cell::StaticCell;

use crate::lv2::{
    ATOM_FLOAT_URID, ATOM_INT_URID, ATOM_LONG_URID, ATOM_OBJECT_URID, ATOM_SEQUENCE_URID,
    MIDI_EVENT_URID, TIME_BAR_BEAT_URID, TIME_BAR_URID, TIME_BEAT_UNIT_URID,
    TIME_BEATS_PER_BAR_URID, TIME_BEATS_PER_MINUTE_URID, TIME_FRAME_URID, TIME_POSITION_URID,
    TIME_SPEED_URID,
};

pub const MIDI_QUEUE_SIZE: usize = 256;
pub const MIDI_BLOCK_CAPACITY: usize = 64;
pub const ATOM_SEQUENCE_CAPACITY: usize = 2048;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MidiEvent {
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
    pub _reserved: u8,
    pub timestamp_micros: u64,
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
pub struct Lv2AtomSequence {
    pub atom: Lv2Atom,
    pub body: Lv2AtomSequenceBody,
    // u64 gives the event area the 8-byte alignment required by LV2 atoms.
    pub data: [u64; ATOM_SEQUENCE_CAPACITY / 8],
}

impl Lv2AtomSequence {
    pub const fn empty() -> Self {
        Self {
            atom: Lv2Atom {
                size: core::mem::size_of::<Lv2AtomSequenceBody>() as u32,
                atom_type: ATOM_SEQUENCE_URID,
            },
            body: Lv2AtomSequenceBody { unit: 0, pad: 0 },
            data: [0; ATOM_SEQUENCE_CAPACITY / 8],
        }
    }

    pub fn clear(&mut self) {
        self.atom.size = core::mem::size_of::<Lv2AtomSequenceBody>() as u32;
        self.atom.atom_type = ATOM_SEQUENCE_URID;
        self.body = Lv2AtomSequenceBody { unit: 0, pad: 0 };
    }

    pub fn set_capacity(&mut self) {
        self.atom.size =
            (core::mem::size_of::<Lv2AtomSequenceBody>() + ATOM_SEQUENCE_CAPACITY) as u32;
    }

    fn push_event(&mut self, frame: i64, atom_type: u32, payload: &[u8]) -> bool {
        let padded_payload = (payload.len() + 7) & !7;
        let event_size = 16 + padded_payload;
        let used = self.atom.size as usize - core::mem::size_of::<Lv2AtomSequenceBody>();
        if used + event_size > ATOM_SEQUENCE_CAPACITY {
            return false;
        }
        let destination = unsafe {
            core::slice::from_raw_parts_mut(
                self.data.as_mut_ptr() as *mut u8,
                ATOM_SEQUENCE_CAPACITY,
            )
        };
        destination[used..used + 8].copy_from_slice(&frame.to_ne_bytes());
        destination[used + 8..used + 12].copy_from_slice(&(payload.len() as u32).to_ne_bytes());
        destination[used + 12..used + 16].copy_from_slice(&atom_type.to_ne_bytes());
        destination[used + 16..used + 16 + payload.len()].copy_from_slice(payload);
        destination[used + 16 + payload.len()..used + event_size].fill(0);
        self.atom.size += event_size as u32;
        true
    }

    pub fn push_midi(&mut self, frame: i64, message: [u8; 3]) -> bool {
        self.push_event(frame, MIDI_EVENT_URID, &message)
    }

    pub fn push_position(&mut self, absolute_frame: u64, sample_rate: u32, bpm: f32) -> bool {
        let beats = absolute_frame as f64 * bpm as f64 / (sample_rate as f64 * 60.0);
        // The transport frame is unsigned, so truncation is floor here and
        // avoids pulling another floating-point helper into the firmware.
        let bar = (beats / 4.0) as i64;
        let bar_beat = (beats - bar as f64 * 4.0) as f32;
        let mut object = [0u8; 176];
        let mut used = 0usize;

        object[used..used + 4].copy_from_slice(&0u32.to_ne_bytes());
        object[used + 4..used + 8].copy_from_slice(&TIME_POSITION_URID.to_ne_bytes());
        used += 8;

        fn property(buffer: &mut [u8], used: &mut usize, key: u32, atom_type: u32, value: &[u8]) {
            buffer[*used..*used + 4].copy_from_slice(&key.to_ne_bytes());
            buffer[*used + 4..*used + 8].copy_from_slice(&0u32.to_ne_bytes());
            buffer[*used + 8..*used + 12].copy_from_slice(&(value.len() as u32).to_ne_bytes());
            buffer[*used + 12..*used + 16].copy_from_slice(&atom_type.to_ne_bytes());
            buffer[*used + 16..*used + 16 + value.len()].copy_from_slice(value);
            let property_size = 16 + ((value.len() + 7) & !7);
            buffer[*used + 16 + value.len()..*used + property_size].fill(0);
            *used += property_size;
        }

        property(
            &mut object,
            &mut used,
            TIME_FRAME_URID,
            ATOM_LONG_URID,
            &(absolute_frame as i64).to_ne_bytes(),
        );
        property(
            &mut object,
            &mut used,
            TIME_SPEED_URID,
            ATOM_FLOAT_URID,
            &1.0f32.to_ne_bytes(),
        );
        property(
            &mut object,
            &mut used,
            TIME_BAR_URID,
            ATOM_LONG_URID,
            &bar.to_ne_bytes(),
        );
        property(
            &mut object,
            &mut used,
            TIME_BAR_BEAT_URID,
            ATOM_FLOAT_URID,
            &bar_beat.to_ne_bytes(),
        );
        property(
            &mut object,
            &mut used,
            TIME_BEATS_PER_BAR_URID,
            ATOM_FLOAT_URID,
            &4.0f32.to_ne_bytes(),
        );
        property(
            &mut object,
            &mut used,
            TIME_BEATS_PER_MINUTE_URID,
            ATOM_FLOAT_URID,
            &bpm.to_ne_bytes(),
        );
        property(
            &mut object,
            &mut used,
            TIME_BEAT_UNIT_URID,
            ATOM_INT_URID,
            &4i32.to_ne_bytes(),
        );
        self.push_event(0, ATOM_OBJECT_URID, &object[..used])
    }
}

pub static MIDI_QUEUE: StaticCell<Queue<MidiEvent, MIDI_QUEUE_SIZE>> = StaticCell::new();
