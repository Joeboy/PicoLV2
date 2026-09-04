# PicoLV2

## The goal

This is an attempt to get audio plugins working on a Raspberry Pi Pico 2, and
eventually other more suitable hardware. I'm using [LV2](https://lv2plug.in/) as
the basis for the plugin format. The Pico host is written in Rust /
[Embassy](https://github.com/embassy-rs/embassy). Plugins can be written in
whatever, as long as they adhere to the
[LV2 standard](https://lv2plug.in/ns/lv2core). C, C++ and Rust are obvious
options.

## LV2

I can't claim to be an expert on plugin formats, but LV2 seems very open and
extensible, and I kind of know it already.

There are lots of LV2 plugins out there. I'm hoping that at least some of the
simpler effects plugins might just work with minimal changes. I haven't tried
any third party plugins yet, but I _have_ created LV2 plugins that work both in
Linux and on the Pico.

## The Hardware Part

I'm targeting the Raspberry Pi Pico 2 / RP2350 microcontroller as the host,
wired to a PCM5102 I2S audio out module. See
[here](https://github.com/Joeboy/oxynth) for hardware instructions. It's fairly
cheap and easy if you want to play with it.

I need to try wiring it up to something with an I2S audio input, so I can try
using it for effects as well as synthesis.

In future I or somebody else might come up with some suitable custom hardware.
For now the Pico 2 is OK at least for a Proof of Concept. It'd be nice to have
more MiPS, more RAM, knobs, onboard audio IO...

If anybody else wants to work on the hardware part I'm very open to collaborate.

## Project Status

What exists so far is:

- Several LV2 [plugins](./plugins/) that can be built or run for Linux or the
  Pico 2. Some in Rust, some in C. I don't claim they're particularly good but
  they're handy for testing.
- [picolv2-firmware](./picolv2-firmware/) - A Rust / [Embassy](https://embassy.dev/)
  firmware project that can run the plugins on the Pico.
- [picolv2-image](./picolv2-image/) - A tool for bundling plugins with the
  firmware, into an image that can be flashed onto the Pico
- USB midi input and audio output for the Pico

## TODO

- Audio input to the Pico (both hardware and software parts). Synths are nice
  but being able to do effects is the real goal.
- Experiment with existing LV2 plugins. Hopefully some will work with minimal
  porting, but we'll see.
- Connections. Some way of connecting up a
  [DAG](https://en.wikipedia.org/wiki/Directed_acyclic_graph) of plugins
- At some point I'm going to have to figure out what to do about controls. Maybe
  eventually there will be hardware with knobs. Or for now we could wire it up
  to MIDI controller messages?
- Latency is currently poor, 50ms or something. I don't care much yet as it's
  still an experiment, but it could probably be improved by reducing buffer
  sizes etc.

## AI declaration

This project was assisted by AI tools (mostly github copilot with auto-models).
