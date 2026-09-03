use core::ffi::c_void;

use crate::abi::{Lv2AtomEvent, Lv2AtomSequence, pad_size};

const N_VOICES: usize = 16;
const PI: f32 = core::f32::consts::PI;
const MAX_AMPLITUDE: f32 = 12000.0 / 32767.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Waveform {
    Sine,
    Square,
    Sawtooth,
    Triangle,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EnvelopeStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Copy)]
struct Voice {
    note: u8,
    frequency: f32,
    target_amplitude: f32,
    envelope: f32,
    gate: bool,
    phase: f32,
    age: u32,
    stage: EnvelopeStage,
    attack_increment: f32,
    decay_increment: f32,
    sustain_level: f32,
    release_increment: f32,
    filter_bandpass: f32,
    filter_lowpass: f32,
}

impl Voice {
    const fn new() -> Self {
        Self {
            note: 0,
            frequency: 0.0,
            target_amplitude: 0.0,
            envelope: 0.0,
            gate: false,
            phase: 0.0,
            age: 0,
            stage: EnvelopeStage::Idle,
            attack_increment: 0.0,
            decay_increment: 0.0,
            sustain_level: 1.0,
            release_increment: 0.0,
            filter_bandpass: 0.0,
            filter_lowpass: 0.0,
        }
    }

    fn active(&self) -> bool {
        self.stage != EnvelopeStage::Idle || self.envelope > 0.000001
    }
}

pub struct Synth {
    sample_rate: f32,
    midi_input: *const Lv2AtomSequence,
    output: *mut f32,
    atom_sequence_urid: u32,
    midi_event_urid: u32,
    voices: [Voice; N_VOICES],
    age_counter: u32,
    waveform: Waveform,
    attack_seconds: f32,
    decay_seconds: f32,
    sustain_level: f32,
    release_seconds: f32,
    filter_cutoff: f32,
    filter_resonance: f32,
}

impl Synth {
    pub const fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            midi_input: core::ptr::null(),
            output: core::ptr::null_mut(),
            atom_sequence_urid: 0,
            midi_event_urid: 0,
            voices: [Voice::new(); N_VOICES],
            age_counter: 0,
            waveform: Waveform::Sine,
            attack_seconds: 0.005,
            decay_seconds: 0.050,
            sustain_level: 0.2,
            release_seconds: 0.500,
            filter_cutoff: 0.5,
            filter_resonance: 0.5,
        }
    }

    pub fn initialise(&mut self, sample_rate: f32, sequence_urid: u32, midi_urid: u32) {
        self.sample_rate = sample_rate;
        self.atom_sequence_urid = sequence_urid;
        self.midi_event_urid = midi_urid;
        self.midi_input = core::ptr::null();
        self.output = core::ptr::null_mut();
        self.age_counter = 0;
        self.waveform = Waveform::Sine;
        self.attack_seconds = 0.005;
        self.decay_seconds = 0.050;
        self.sustain_level = 0.2;
        self.release_seconds = 0.500;
        self.filter_cutoff = 0.5;
        self.filter_resonance = 0.5;
        self.voices.fill(Voice::new());
    }

    pub fn connect_port(&mut self, port: u32, data: *mut c_void) {
        match port {
            0 => self.midi_input = data.cast(),
            1 => self.output = data.cast(),
            _ => {}
        }
    }

    pub fn activate(&mut self) {
        for voice in &mut self.voices {
            *voice = Voice::new();
        }
    }

    fn midi_note_frequency(note: u8) -> f32 {
        const RATIOS: [f32; 12] = [
            1.0, 1.059463094, 1.122462048, 1.189207115, 1.259921050, 1.334839854,
            1.414213562, 1.498307077, 1.587401052, 1.681792831, 1.781797436, 1.887748625,
        ];
        let mut octave = i32::from(note) / 12 - 5;
        let mut frequency = 261.625565 * RATIOS[usize::from(note % 12)];
        while octave > 0 {
            frequency *= 2.0;
            octave -= 1;
        }
        while octave < 0 {
            frequency *= 0.5;
            octave += 1;
        }
        frequency
    }

    fn allocate_voice(&self) -> usize {
        if let Some(index) = self.voices.iter().position(|voice| !voice.active()) {
            return index;
        }
        self.voices
            .iter()
            .enumerate()
            .min_by_key(|(_, voice)| voice.age)
            .map(|(index, _)| index)
            .unwrap_or(0)
    }

    fn start_voice(&mut self, note: u8, velocity: u8) {
        let index = self.allocate_voice();
        self.age_counter = self.age_counter.wrapping_add(1);
        let target = f32::from(velocity) / 127.0;
        let attack_samples = (self.attack_seconds * self.sample_rate).max(1.0);
        let decay_samples = (self.decay_seconds * self.sample_rate).max(1.0);
        self.voices[index] = Voice {
            note,
            frequency: Self::midi_note_frequency(note),
            target_amplitude: target,
            envelope: self.voices[index].envelope.max(0.0),
            gate: true,
            phase: self.voices[index].phase,
            age: self.age_counter,
            stage: EnvelopeStage::Attack,
            attack_increment: target / attack_samples,
            decay_increment: (target - self.sustain_level * target) / decay_samples,
            sustain_level: self.sustain_level,
            release_increment: 0.0,
            filter_bandpass: self.voices[index].filter_bandpass,
            filter_lowpass: self.voices[index].filter_lowpass,
        };
    }

    fn release_voice(voice: &mut Voice, sample_rate: f32, release_seconds: f32) {
        let samples = (release_seconds * sample_rate).max(1.0);
        voice.gate = false;
        voice.release_increment = voice.envelope / samples;
        voice.stage = EnvelopeStage::Release;
    }

    fn handle_midi(&mut self, message: &[u8]) {
        let kind = message[0] & 0xf0;
        match kind {
            0x90 if message[2] != 0 => self.start_voice(message[1], message[2]),
            0x80 | 0x90 => {
                for voice in &mut self.voices {
                    if voice.note == message[1] && voice.gate {
                        Self::release_voice(voice, self.sample_rate, self.release_seconds);
                    }
                }
            }
            0xb0 => match message[1] {
                21 => {
                    self.waveform = match message[2] / 32 {
                        0 => Waveform::Sine,
                        1 => Waveform::Square,
                        2 => Waveform::Sawtooth,
                        _ => Waveform::Triangle,
                    }
                }
                22 => self.attack_seconds = 0.001 + f32::from(message[2]) / 127.0 * 1.999,
                23 => self.decay_seconds = 0.001 + f32::from(message[2]) / 127.0 * 1.999,
                24 => self.sustain_level = f32::from(message[2]) / 127.0,
                25 => self.release_seconds = 0.001 + f32::from(message[2]) / 127.0 * 2.999,
                26 => self.filter_cutoff = f32::from(message[2]) / 127.0,
                27 => self.filter_resonance = f32::from(message[2]) / 127.0 * 4.0,
                120 | 123 => {
                    for voice in &mut self.voices {
                        if voice.gate {
                            Self::release_voice(voice, self.sample_rate, self.release_seconds);
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn sine(phase: f32) -> f32 {
        let x = phase * (2.0 * PI) - PI;
        let x2 = x * x;
        x * (1.0 - x2 / 6.0 + x2 * x2 / 120.0 - x2 * x2 * x2 / 5040.0)
    }

    fn oscillator(waveform: Waveform, phase: f32) -> f32 {
        match waveform {
            Waveform::Sine => Self::sine(phase),
            Waveform::Square => if phase < 0.5 { 1.0 } else { -1.0 },
            Waveform::Sawtooth => 2.0 * phase - 1.0,
            Waveform::Triangle => if phase < 0.5 { 4.0 * phase - 1.0 } else { 3.0 - 4.0 * phase },
        }
    }

    fn render(&mut self, start: u32, end: u32) {
        let filter_f = (self.filter_cutoff * 0.5 * PI).min(1.5);
        let filter_q = (1.0 - self.filter_resonance * 0.24).max(0.05);
        for frame in start..end {
            let mut mix = 0.0;
            for voice in &mut self.voices {
                match voice.stage {
                    EnvelopeStage::Idle => {}
                    EnvelopeStage::Attack => {
                        voice.envelope += voice.attack_increment;
                        if voice.envelope >= voice.target_amplitude {
                            voice.envelope = voice.target_amplitude;
                            voice.stage = EnvelopeStage::Decay;
                        }
                    }
                    EnvelopeStage::Decay => {
                        let sustain = voice.sustain_level * voice.target_amplitude;
                        voice.envelope -= voice.decay_increment;
                        if voice.envelope <= sustain {
                            voice.envelope = sustain;
                            voice.stage = EnvelopeStage::Sustain;
                        }
                    }
                    EnvelopeStage::Sustain => {}
                    EnvelopeStage::Release => {
                        voice.envelope -= voice.release_increment;
                        if voice.envelope <= 0.0 {
                            voice.envelope = 0.0;
                            voice.stage = EnvelopeStage::Idle;
                        }
                    }
                }

                voice.phase += voice.frequency / self.sample_rate;
                if voice.phase >= 1.0 {
                    voice.phase -= 1.0;
                }
                if voice.envelope > 0.0 {
                    let sample = Self::oscillator(self.waveform, voice.phase);
                    let lowpass = voice.filter_lowpass + filter_f * voice.filter_bandpass;
                    let highpass = sample - lowpass - filter_q * voice.filter_bandpass;
                    voice.filter_bandpass = filter_f * highpass + voice.filter_bandpass;
                    voice.filter_lowpass = lowpass;
                    mix += lowpass * voice.envelope;
                }
            }
            unsafe { self.output.add(frame as usize).write(MAX_AMPLITUDE * mix / N_VOICES as f32) };
        }
    }

    pub unsafe fn run(&mut self, sample_count: u32) {
        if self.output.is_null() {
            return;
        }
        let mut offset = 0;
        let input = self.midi_input;
        if !input.is_null()
            && unsafe { (*input).atom.atom_type == self.atom_sequence_urid }
            && unsafe { (*input).atom.size >= size_of::<crate::abi::Lv2AtomSequenceBody>() as u32 }
        {
            let mut event_ptr = unsafe { (&raw const (*input).body).add(1).cast::<u8>() };
            let end = unsafe { (&raw const (*input).body).cast::<u8>().add((*input).atom.size as usize) };
            while unsafe { event_ptr.add(size_of::<Lv2AtomEvent>()) <= end } {
                let event = event_ptr.cast::<Lv2AtomEvent>();
                let event_size = size_of::<Lv2AtomEvent>() + pad_size(unsafe { (*event).body.size }) as usize;
                if unsafe { event_ptr.add(event_size) > end } {
                    break;
                }
                if unsafe { (*event).body.atom_type == self.midi_event_urid && (*event).body.size >= 3 } {
                    let frame = unsafe { (*event).time.frames }.clamp(0, i64::from(sample_count)) as u32;
                    let frame = frame.max(offset);
                    self.render(offset, frame);
                    let message = unsafe { core::slice::from_raw_parts(event.add(1).cast::<u8>(), 3) };
                    self.handle_midi(message);
                    offset = frame;
                }
                event_ptr = unsafe { event_ptr.add(event_size) };
            }
        }
        self.render(offset, sample_count);
    }
}
