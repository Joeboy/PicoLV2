# Oxynth LV2 POC

An LV2 instrument written in Rust and derived from the DSP and MIDI behaviour of
`oxynth-and-embassy/oxynth/src/synth.rs`.

The implementation is `#![no_std]`. Cargo builds a PIC Rust `staticlib` for each
target, then the platform C linker wraps that archive into a shared object: a
runtime-loadable ARM `ET_DYN` for Pico 2 or a conventional Linux LV2 plugin.

## Features

- 16 voices with oldest-voice stealing
- Sample-accurate LV2 Atom Sequence MIDI input
- Sine, square, sawtooth, and triangle oscillators
- Per-voice ADSR envelopes
- Per-voice state-variable low-pass filters
- MIDI velocity
- All Notes Off support

## MIDI controls

|      CC | Parameter                                  |
| ------: | ------------------------------------------ |
|      21 | Waveform: sine, square, sawtooth, triangle |
|      22 | Attack: 1 ms to 2 s                        |
|      23 | Decay: 1 ms to 2 s                         |
|      24 | Sustain: 0 to 1                            |
|      25 | Release: 1 ms to 3 s                       |
|      26 | Filter cutoff                              |
|      27 | Filter resonance                           |
| 120/123 | All notes off                              |

## Build

- `make` or `make pico`: build `build/pico/plugin.so`
- `make linux`: build `build/linux/plugin.so`
- `make bundle`: build and populate `oxynth.lv2/plugin.so`

The Pico artifact is ARM hard-float PIC, approximately 7 KiB, has no unresolved
symbols, and currently needs only the descriptor's eight `R_ARM_RELATIVE`
relocations.

The Pico loader currently embeds this plugin directly from
`picolv2-firmware/src/plugin_host.rs`. Change its `include_bytes!` path to switch to
a different compatible plugin.
