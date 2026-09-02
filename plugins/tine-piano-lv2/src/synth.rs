use core::ffi::c_void;
use crate::abi::{pad_size, Lv2AtomEvent, Lv2AtomSequence};

const VOICES: usize = 16;
const MAX_OUTPUT: f32 = 0.35;

#[derive(Clone, Copy)]
struct Voice {
    note: u8,
    level: f32,
    target_level: f32,
    attack_increment: f32,
    phase: f32,
    phase_increment: f32,
    tine_level: f32,
    tonebar_phase: f32,
    tonebar_phase_increment: f32,
    tonebar_level: f32,
    hammer_phase: f32,
    hammer_phase_increment: f32,
    hammer_level: f32,
    noise_state: u32,
    gate: bool,
    age: u32,
}

impl Voice {
    const fn new() -> Self { Self { note: 0, level: 0.0, target_level: 0.0, attack_increment: 0.0, phase: 0.0, phase_increment: 0.0, tine_level: 0.0, tonebar_phase: 0.0, tonebar_phase_increment: 0.0, tonebar_level: 0.0, hammer_phase: 0.0, hammer_phase_increment: 0.0, hammer_level: 0.0, noise_state: 0x1234_5678, gate: false, age: 0 } }
    fn active(&self) -> bool { self.gate || self.level > 0.00001 }
}

pub struct TinePiano {
    sample_rate: f32,
    midi: *const Lv2AtomSequence,
    output: *mut f32,
    sequence_urid: u32,
    midi_urid: u32,
    voices: [Voice; VOICES],
    age: u32,
    pickup: f32,
    stiffness: f32,
    damping: f32,
    tremolo: f32,
    tremolo_phase: f32,
}

impl TinePiano {
    pub const fn new() -> Self { Self { sample_rate: 48000.0, midi: core::ptr::null(), output: core::ptr::null_mut(), sequence_urid: 0, midi_urid: 0, voices: [Voice::new(); VOICES], age: 0, pickup: 0.45, stiffness: 0.5, damping: 0.5, tremolo: 0.15, tremolo_phase: 0.0 } }
    pub fn initialise(&mut self, rate: f32, sequence: u32, midi: u32) { self.sample_rate = rate; self.sequence_urid = sequence; self.midi_urid = midi; self.midi = core::ptr::null(); self.output = core::ptr::null_mut(); self.voices.fill(Voice::new()); self.age = 0; self.pickup = 0.45; self.stiffness = 0.5; self.damping = 0.5; self.tremolo = 0.15; self.tremolo_phase = 0.0; }
    pub fn connect_port(&mut self, port: u32, data: *mut c_void) { match port { 0 => self.midi = data.cast(), 1 => self.output = data.cast(), _ => {} } }
    pub fn activate(&mut self) { self.voices.fill(Voice::new()); }

    fn frequency(note: u8) -> f32 {
        const RATIOS: [f32; 12] = [
            1.0, 1.0594631, 1.122462, 1.1892071, 1.259921, 1.3348398,
            1.4142135, 1.498307, 1.587401, 1.6817929, 1.7817974, 1.8877486,
        ];
        let mut octave = i32::from(note) / 12 - 5;
        let mut frequency = 261.62555 * RATIOS[usize::from(note % 12)];
        while octave > 0 { frequency *= 2.0; octave -= 1; }
        while octave < 0 { frequency *= 0.5; octave += 1; }
        frequency
    }
    fn choose_voice(&self) -> usize {
        self.voices.iter().position(|voice| !voice.gate).unwrap_or_else(|| {
            self.voices.iter().enumerate().min_by_key(|(_, voice)| voice.age).map(|(index, _)| index).unwrap_or(0)
        })
    }
    fn note_on(&mut self, note: u8, velocity: u8) {
        let index = self.choose_voice(); self.age = self.age.wrapping_add(1);
        let velocity = f32::from(velocity) / 127.0;
        let mut voice = Voice::new();
        voice.note = note;
        voice.target_level = velocity;
        voice.attack_increment = velocity / (self.sample_rate * 0.003).max(1.0);
        let frequency = Self::frequency(note);
        voice.phase_increment = frequency / self.sample_rate;
        voice.tonebar_phase_increment = frequency * (1.48 + self.stiffness * 0.12) / self.sample_rate;
        voice.hammer_phase_increment = frequency * (4.5 + self.stiffness * 1.5) / self.sample_rate;
        voice.tine_level = velocity;
        voice.tonebar_level = velocity * (0.07 + self.stiffness * 0.16);
        voice.hammer_level = velocity * (0.006 + self.stiffness * 0.009);
        voice.noise_state = u32::from(note).wrapping_mul(0x9e37_79b9).wrapping_add(self.age);
        voice.gate = true;
        voice.age = self.age;
        self.voices[index] = voice;
    }
    fn note_off(&mut self, note: u8) { for voice in &mut self.voices { if voice.note == note { voice.gate = false; } } }
    fn midi(&mut self, message: &[u8]) { match message[0] & 0xf0 { 0x90 if message[2] != 0 => self.note_on(message[1], message[2]), 0x80 | 0x90 => self.note_off(message[1]), 0xb0 => match message[1] { 21 => self.pickup = f32::from(message[2]) / 127.0, 22 => self.stiffness = f32::from(message[2]) / 127.0, 23 => self.damping = f32::from(message[2]) / 127.0, 24 => self.tremolo = f32::from(message[2]) / 127.0 * 0.5, 120 | 123 => for voice in &mut self.voices { voice.gate = false }, _ => {} }, _ => {} } }

    fn render(&mut self, start: u32, end: u32) {
        let release_decay = 0.99994 + (1.0 - self.damping) * 0.00004;
        for frame in start..end {
            let mut mix = 0.0;
            for voice in &mut self.voices {
                if voice.active() {
                    let phase = voice.phase;
                    let fundamental = sine(phase);
                    if voice.gate && voice.level < voice.target_level {
                        voice.level = (voice.level + voice.attack_increment).min(voice.target_level);
                    }
                    let tonebar = sine(voice.tonebar_phase);
                    let hammer = sine(voice.hammer_phase);
                    voice.noise_state ^= voice.noise_state << 13;
                    voice.noise_state ^= voice.noise_state >> 17;
                    voice.noise_state ^= voice.noise_state << 5;
                    let hammer_noise = (voice.noise_state as f32 / 2_147_483_648.0) - 1.0;
                    mix += voice.level * (fundamental * voice.tine_level + tonebar * voice.tonebar_level)
                        + (hammer + hammer_noise * 0.35) * voice.hammer_level;
                    voice.phase += voice.phase_increment;
                    if voice.phase >= 1.0 { voice.phase -= 1.0; }
                    voice.tonebar_phase += voice.tonebar_phase_increment;
                    if voice.tonebar_phase >= 1.0 { voice.tonebar_phase -= 1.0; }
                    voice.hammer_phase += voice.hammer_phase_increment;
                    if voice.hammer_phase >= 1.0 { voice.hammer_phase -= 1.0; }
                    voice.tine_level *= if voice.gate { 0.9999995 } else { 0.99994 };
                    voice.tonebar_level *= if voice.gate { 0.999997 } else { 0.99995 };
                    voice.hammer_level *= 0.99935;
                    voice.level *= if voice.gate { 0.999995 } else { release_decay };
                    if voice.level < 0.00001 { voice.level = 0.0; voice.gate = false; }
                }
            }
            let tremolo = 1.0 - self.tremolo * 0.3 * (1.0 + sine(self.tremolo_phase));
            self.tremolo_phase += 5.2 / self.sample_rate;
            if self.tremolo_phase >= 1.0 { self.tremolo_phase -= 1.0; }
            let input = mix * MAX_OUTPUT * tremolo;
            let output = input * (1.0 + self.pickup * input) / (1.0 + self.pickup * input.abs());
            unsafe { self.output.add(frame as usize).write(output); }
        }
    }

    pub unsafe fn run(&mut self, count: u32) {
        if self.output.is_null() { return; }
        let mut offset = 0; let input = self.midi;
        if !input.is_null() && unsafe { (*input).atom.atom_type == self.sequence_urid && (*input).atom.size >= 8 } {
            let mut pointer = unsafe { (&raw const (*input).body).add(1).cast::<u8>() };
            let end = unsafe { (&raw const (*input).body).cast::<u8>().add((*input).atom.size as usize) };
            while unsafe { pointer.add(core::mem::size_of::<Lv2AtomEvent>()) <= end } {
                let event = pointer.cast::<Lv2AtomEvent>(); let size = core::mem::size_of::<Lv2AtomEvent>() + pad_size(unsafe { (*event).body.size }) as usize;
                if unsafe { pointer.add(size) > end } { break; }
                if unsafe { (*event).body.atom_type == self.midi_urid && (*event).body.size >= 3 } {
                    let frame = unsafe { (*event).time.frames }.clamp(0, i64::from(count)) as u32; self.render(offset, frame);
                    let message = unsafe { core::slice::from_raw_parts(event.add(1).cast::<u8>(), 3) }; self.midi(message); offset = frame;
                }
                pointer = unsafe { pointer.add(size) };
            }
        }
        self.render(offset, count);
    }
}

fn sine(phase: f32) -> f32 {
    let mut x = phase * (2.0 * core::f32::consts::PI);
    if x > core::f32::consts::PI { x -= 2.0 * core::f32::consts::PI; }
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    x = x.abs();
    if x > 0.5 * core::f32::consts::PI { x = core::f32::consts::PI - x; }
    let x2 = x * x;
    sign * x * (1.0 - x2 / 6.0 + x2 * x2 / 120.0 - x2 * x2 * x2 / 5040.0)
}
