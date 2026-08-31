# Oxynth LV2 POC

An LV2 instrument derived from the DSP and MIDI behaviour of
`oxynth-and-embassy/oxynth/src/synth.rs`.

The implementation is freestanding C so the same source can be built as a
runtime-loadable ARM ELF object for the Pico 2 or as a conventional Linux LV2
plugin.

## Features

- 16 voices with oldest-voice stealing
- Sample-accurate LV2 Atom Sequence MIDI input
- Sine, square, sawtooth, and triangle oscillators
- Per-voice ADSR envelopes
- Per-voice state-variable low-pass filters
- MIDI velocity
- All Notes Off support

## MIDI controls

| CC | Parameter |
|---:|---|
| 21 | Waveform: sine, square, sawtooth, triangle |
| 22 | Attack: 1 ms to 2 s |
| 23 | Decay: 1 ms to 2 s |
| 24 | Sustain: 0 to 1 |
| 25 | Release: 1 ms to 3 s |
| 26 | Filter cutoff |
| 27 | Filter resonance |
| 120/123 | All notes off |

## Build

- `make` or `make pico`: build `build/pico/plugin.so`
- `make linux`: build `build/linux/plugin.so`
- `make bundle`: build and populate `oxynth.lv2/plugin.so`

The Pico loader currently embeds this plugin directly from
`pico-loader/src/plugin_host.rs`. Change its `include_bytes!` path to switch to
a different compatible plugin.
