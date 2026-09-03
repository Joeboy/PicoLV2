# TinePiano LV2 POC

An allocation-free `#![no_std]` LV2 instrument prototype for the Pico 2 and
Linux. It supports 16 voices; each voice combines four damped modal resonators
with velocity-shaped hammer excitation, a nonlinear pickup and speaker output
stage, release damping, and tremolo.

MIDI CC 21 controls pickup distance, CC 22 tine stiffness, CC 23 damping, and CC
24 tremolo depth. The plugin uses the same Atom Sequence ABI as the other
plugins in this repository.

Build with `make` for Pico, `make linux` for a native shared object, or
`make bundle` for a Linux LV2 bundle.
