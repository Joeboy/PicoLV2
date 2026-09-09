# String Synth LV2 POC

An allocation-free `#![no_std]` LV2 instrument prototype for the Pico 2 and
PC, inspired by classic string ensemble machines (Solina / ARP String
Ensemble). It supports 12 voices; each voice mixes two detuned sawtooth
oscillators and a sub-octave pulse layer through a slow string-swell envelope.
The mixed output passes through a three-tap modulated delay ensemble/chorus
effect and a one-pole tone filter.

MIDI CC 21 controls ensemble depth, CC 22 attack time, CC 23 release time, and
CC 24 filter brightness. The plugin uses the same Atom Sequence ABI as the
other plugins in this repository.

Build with `make` for Pico, `make pc` for a native shared object, or
`make bundle` for a PC LV2 bundle.
