# PicoLV2

## LV2 plugins and Ingen patches on the Raspberry Pi Pico 2

My attempt to get audio plugins, and groups of audio plugins, working on the
Raspberry Pi Pico 2. The Pico host is written in Rust /
[Embassy](https://github.com/embassy-rs/embassy). I'm using
[LV2](https://lv2plug.in/) as the plugin format. Plugins can be written in
whatever, as long as they adhere to the
[LV2 standard](https://lv2plug.in/ns/lv2core). C, C++ and Rust are the obvious
options. Patches are in [ingen](https://gitlab.com/drobilla/ingen)'s bundle
format.

## Table of contents

- [PicoLV2](#picolv2)
  - [LV2 plugins and Ingen patches on the Raspberry Pi Pico 2](#lv2-plugins-and-ingen-patches-on-the-raspberry-pi-pico-2)
  - [Table of contents](#table-of-contents)
  - [The Pico 2](#the-pico-2)
  - [Hardware wireup](#hardware-wireup)
  - [LV2 plugins](#lv2-plugins)
  - [Ingen](#ingen)
  - [Project Status](#project-status)
    - [TODO](#todo)
  - [Usage](#usage)
  - [Caveats and limitations](#caveats-and-limitations)
  - [AI declaration](#ai-declaration)

## The Pico 2

The Raspberry Pi Pico 2 is a $5 microcontroller board, from the people who
brought you the $35 Raspberry Pi computer in 2012.

![Raspberry Pi Pico 2](docs/images/raspberry-pi-pico-2.jpg)

It runs (un-overclocked) at 150MHz. Which one the one hand is ~3000 cycles per
sample at 48KHz, which seems like enough to get some stuff done. On the other
hand it's a lot less than you get even on a "very slow" modern PC.

It has 2MB of Flash RAM and 520KB of SRAM. For anything realtime we have to use
the SRAM, so that's a bit of a limitation. Eg. a 2.5s mono audio buffer takes up
480KB on its own.

I guess there's a lot we won't be able to do, but also a lot we will. It's a $5
device, there will be compromises.

Note that I'm targeting the Pico 2, even if I just say "the Pico" for brevity.
The original Pico is limited in ways I think might make this a bit impractical,
eg. no hardware floats. Again, a Pico 2 costs $5.

## Hardware wireup

To get audio out of the Pico, I'm using a PCM5102 I2S DAC board. I should write
this up here properly but for now see
[this other project](https://github.com/Joeboy/oxynth) for details of how to
hook it up. Small amount of soldering involved.

![alt text](docs/images/wireup.png)

There's currently no audio in, which frustratingly means it can't be used for
guitar effects or whatever. At some point I'll get around to hooking it up to an
ADC as well as the DAC.

I like the idea that one day it might run on something a bit more suitable, like
a beefier ARM board with integrated audio IO. I'm open to collab on this!

## LV2 plugins

I can't claim to be an expert on plugin formats, but [LV2](https://lv2plug.in/)
seems very open and extensible, and I kind of know it already.

There are lots of LV2 plugins out there. I've made exploratory efforts at
porting some of them and it seems quite promising. The mda piano / epiano were a
bit problematic, I had to downsample the samples to get them to fit. The ams
vco3 had to be optimized to run quickly enough, which theoretically could affect
the sound (although I doubt it's noticeable). I haven't actually tested most of
what I ported.

Some plugins will be basically impossible to get working, in particular:

- anything that needs more than 520Kb of SRAM, ie. long samples or delay lines
- anything that can't be made to run fast enough
- anything that doesn't make its source code available

There's also an open question - if I fork a plugin and make it work with
PicoLV2, should I use the same URID? It's _fundamentally_ the same plugin, but
typically URIDs are tied to domain names owned by their developers. Not yet sure
what the right answer is.

## Ingen

The "patch" format is Ingen.

![An Ingen patch](docs/images/ingen-example.png)

I'm not 100% sure if that's the right choice. The format seems cool, but Ingen
itself is a bit painful to use. I should probably check out MOD pedalboard.

In some cases a patch could just be a single LV2 plugin connected up to the
input and output. It could also (at least in theory) be a full-on modular synth
patch with lots of nodes.

## Project Status

It's still quite new but things seem to be working pretty well. You can hook up
LV2 plugins using Ingen and flash the graph to your Pico with the
[firmware](./picolv2-firmware/) to play / hear the result.

### TODO

- Port more plugins
- More patches. Both modular synths and effects chains.
- Better docs.
- Audio input to the Pico (both hardware and software parts). Synths are nice
  but being able to do effects is the real goal.
- At some point I'm going to have to figure out what to do about controls. Maybe
  eventually there will be hardware with knobs. Or for now we could wire it up
  to MIDI controller messages?
- Once we have controls, we should maybe have some way to save state to flash
  RAM. Should check out the LV2
  [state extension](https://lv2plug.in/ns/ext/state).
- At some point I should make there be downloadable binaries for the firmware,
  `picolv2-image` and the plugins. I guess Github actions.
- General testing. Lots of testing.
- Add overclocking support. I haven't really felt a need for it during my
  initial exploratory phase, but it'll obviously arise at some poin.

## Usage

I need to redo the documentation a bit, for now the best place to look is the
[picolv2-image README](./picolv2-image).

## Caveats and limitations

Before you get too excited:

- This is still at an early stage of development and there may be frustrations,
  particularly for beginners. Sorry about that.
- I'm currently assuming Linux, for the PC-side parts. It _might_ work on macOS
  or Windows if you have the dev toolchain setup. Somebody would have to try it
  and let me know. Obviously the pico binaries themselves don't care what OS
  your computer runs.
- The Pico 2 is puny compared to regular computers, so a lot of plugins probably
  won't run properly.
- Currently no audio in, and getting audio out requires some (relatively easy)
  soldering.
- Don't expect to be able to download and use regular LV2 plugins on the Pico.
  They need to be built specially for PicoLV2. As of now there's just the few
  plugins ported by me. Other plugins will require an amount of work to get
  working with PicoLV2.

## AI declaration

This project was assisted by AI tools (mostly github copilot with auto-models).
