use core::ffi::c_void;
use crate::abi::{pad_size, Lv2AtomEvent, Lv2AtomSequence};

const VOICES: usize = 12;
const MAX_OUTPUT: f32 = 0.05;
const DELAY_LEN: usize = 96;
const BASE_DELAY: f32 = 40.0;
const MOD_DEPTH: f32 = 28.0;

#[derive(Clone, Copy)]
struct Voice {
    note: u8,
    gate: bool,
    age: u32,
    level: f32,
    target_level: f32,
    attack_increment: f32,
    osc1_phase: f32,
    osc1_increment: f32,
    osc2_phase: f32,
    osc2_increment: f32,
    osc3_phase: f32,
    osc3_increment: f32,
}

impl Voice {
    const fn new() -> Self { Self { note: 0, gate: false, age: 0, level: 0.0, target_level: 0.0, attack_increment: 0.0, osc1_phase: 0.0, osc1_increment: 0.0, osc2_phase: 0.0, osc2_increment: 0.0, osc3_phase: 0.0, osc3_increment: 0.0 } }
    fn active(&self) -> bool { self.gate || self.level > 0.00001 }
}

pub struct StringSynth {
    sample_rate: f32,
    midi: *const Lv2AtomSequence,
    output: *mut f32,
    sequence_urid: u32,
    midi_urid: u32,
    voices: [Voice; VOICES],
    age: u32,
    ensemble: f32,
    attack: f32,
    release: f32,
    brightness: f32,
    delay_buffer: [f32; DELAY_LEN],
    write_index: usize,
    lfo1_phase: f32,
    lfo2_phase: f32,
    lfo3_phase: f32,
    filter_state: f32,
}

impl StringSynth {
    pub const fn new() -> Self {
        Self {
            sample_rate: 48000.0, midi: core::ptr::null(), output: core::ptr::null_mut(),
            sequence_urid: 0, midi_urid: 0, voices: [Voice::new(); VOICES], age: 0,
            ensemble: 0.5, attack: 0.3, release: 0.5, brightness: 0.6,
            delay_buffer: [0.0; DELAY_LEN], write_index: 0,
            lfo1_phase: 0.0, lfo2_phase: 0.33, lfo3_phase: 0.66, filter_state: 0.0,
        }
    }
    pub fn initialise(&mut self, rate: f32, sequence: u32, midi: u32) {
        self.sample_rate = rate; self.sequence_urid = sequence; self.midi_urid = midi;
        self.midi = core::ptr::null(); self.output = core::ptr::null_mut();
        self.voices.fill(Voice::new()); self.age = 0;
        self.ensemble = 0.5; self.attack = 0.3; self.release = 0.5; self.brightness = 0.6;
        self.delay_buffer.fill(0.0); self.write_index = 0;
        self.lfo1_phase = 0.0; self.lfo2_phase = 0.33; self.lfo3_phase = 0.66; self.filter_state = 0.0;
    }
    pub fn connect_port(&mut self, port: u32, data: *mut c_void) { match port { 0 => self.midi = data.cast(), 1 => self.output = data.cast(), _ => {} } }
    pub fn activate(&mut self) { self.voices.fill(Voice::new()); self.delay_buffer.fill(0.0); self.filter_state = 0.0; }

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
        let attack_seconds = 0.02 + self.attack * 0.4;
        voice.attack_increment = velocity / (self.sample_rate * attack_seconds).max(1.0);
        let frequency = Self::frequency(note);
        voice.osc1_increment = frequency / self.sample_rate;
        voice.osc2_increment = frequency * 1.006 / self.sample_rate;
        voice.osc3_increment = frequency * 0.5 / self.sample_rate;
        voice.gate = true;
        voice.age = self.age;
        self.voices[index] = voice;
    }
    fn note_off(&mut self, note: u8) { for voice in &mut self.voices { if voice.note == note { voice.gate = false; } } }
    fn midi(&mut self, message: &[u8]) {
        match message[0] & 0xf0 {
            0x90 if message[2] != 0 => self.note_on(message[1], message[2]),
            0x80 | 0x90 => self.note_off(message[1]),
            0xb0 => match message[1] {
                21 => self.ensemble = f32::from(message[2]) / 127.0,
                22 => self.attack = f32::from(message[2]) / 127.0,
                23 => self.release = f32::from(message[2]) / 127.0,
                24 => self.brightness = f32::from(message[2]) / 127.0,
                120 | 123 => for voice in &mut self.voices { voice.gate = false },
                _ => {}
            },
            _ => {}
        }
    }

    fn read_delay(&self, delay: f32) -> f32 {
        let delay = delay.clamp(1.0, (DELAY_LEN - 2) as f32);
        let read_pos = self.write_index as f32 - delay + DELAY_LEN as f32;
        let base = read_pos as usize % DELAY_LEN;
        let frac = read_pos - (read_pos as usize) as f32;
        let next = (base + 1) % DELAY_LEN;
        self.delay_buffer[base] * (1.0 - frac) + self.delay_buffer[next] * frac
    }

    fn render(&mut self, start: u32, end: u32) {
        let release_decay = 0.9985 + self.release * 0.00145;
        let lfo1_increment = 0.63 / self.sample_rate;
        let lfo2_increment = 0.87 / self.sample_rate;
        let lfo3_increment = 1.19 / self.sample_rate;
        let filter_cutoff = 0.08 + self.brightness * 0.5;
        for frame in start..end {
            let mut mix = 0.0;
            for voice in &mut self.voices {
                if voice.active() {
                    if voice.gate && voice.level < voice.target_level {
                        voice.level = (voice.level + voice.attack_increment).min(voice.target_level);
                    }
                    let osc1 = saw(voice.osc1_phase);
                    let osc2 = saw(voice.osc2_phase);
                    let osc3 = square(voice.osc3_phase);
                    mix += voice.level * (osc1 * 0.35 + osc2 * 0.35 + osc3 * 0.3);
                    voice.osc1_phase += voice.osc1_increment;
                    if voice.osc1_phase >= 1.0 { voice.osc1_phase -= 1.0; }
                    voice.osc2_phase += voice.osc2_increment;
                    if voice.osc2_phase >= 1.0 { voice.osc2_phase -= 1.0; }
                    voice.osc3_phase += voice.osc3_increment;
                    if voice.osc3_phase >= 1.0 { voice.osc3_phase -= 1.0; }
                    voice.level *= if voice.gate { 1.0 } else { release_decay };
                    if !voice.gate && voice.level < 0.00001 { voice.level = 0.0; }
                }
            }
            let dry = mix * MAX_OUTPUT;
            self.delay_buffer[self.write_index] = dry;
            let tap1 = self.read_delay(BASE_DELAY + MOD_DEPTH * (1.0 + sine(self.lfo1_phase)) * 0.5);
            let tap2 = self.read_delay(BASE_DELAY + MOD_DEPTH * (1.0 + sine(self.lfo2_phase)) * 0.5);
            let tap3 = self.read_delay(BASE_DELAY + MOD_DEPTH * (1.0 + sine(self.lfo3_phase)) * 0.5);
            self.write_index = (self.write_index + 1) % DELAY_LEN;
            self.lfo1_phase += lfo1_increment; if self.lfo1_phase >= 1.0 { self.lfo1_phase -= 1.0; }
            self.lfo2_phase += lfo2_increment; if self.lfo2_phase >= 1.0 { self.lfo2_phase -= 1.0; }
            self.lfo3_phase += lfo3_increment; if self.lfo3_phase >= 1.0 { self.lfo3_phase -= 1.0; }
            let wet = (tap1 + tap2 + tap3) / 3.0;
            let wet_amount = self.ensemble * 0.5;
            let blended = dry * (1.0 - wet_amount) + wet * wet_amount;
            self.filter_state += filter_cutoff * (blended - self.filter_state);
            unsafe { self.output.add(frame as usize).write(self.filter_state); }
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

fn saw(phase: f32) -> f32 { 2.0 * phase - 1.0 }
fn square(phase: f32) -> f32 { if phase < 0.5 { 1.0 } else { -1.0 } }

fn sine(phase: f32) -> f32 {
    let mut x = phase * (2.0 * core::f32::consts::PI);
    if x > core::f32::consts::PI { x -= 2.0 * core::f32::consts::PI; }
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    x = x.abs();
    if x > 0.5 * core::f32::consts::PI { x = core::f32::consts::PI - x; }
    let x2 = x * x;
    sign * x * (1.0 - x2 / 6.0 + x2 * x2 / 120.0 - x2 * x2 * x2 / 5040.0)
}
