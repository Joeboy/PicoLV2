# PicoPlayer

## The Eventual, Far-Future Goal

A hardware device that can load arbitrary audio plugins. This could be a synth
or an effects unit. I'm targeting [LV2](https://lv2plug.in/) as the basis /
inspiration for the plugin format. There are lots of existing LV2 plugins, some
of which might work with minor modification. We'll see. In any event it seems
like a good starting point.

For the plugin idea to make sense, it'd be good to have a flashable "plugin" RAM
area. Or an SD card could work I guess. It'd be good to be able to chain
plugins.

## Right Now

I'm targeting the Raspberry Pi Pico 2 / RP2350 microcontroller as the host,
wired to a PCM5102 I2S module. In future I or somebody else might come up with
some suitable custom hardware. For now the Pico 2 is OK, although something
faster and with more RAM would be nice (especially for chains of multiple
plugins). Also for now I'm just including the plugin binary in the host, which
arguably defeats the purpose. I'm currently not doing anything with the LV2
metadata files on the Pico.

What exists so far is:

- `example-lv2/` - A simple / stupid LV2 synth plugin written in C, that can be
  either built as a "real" LV2 plugin for linux, or a binary that can be loaded
  (currently `include_bytes()`d) on the Pico. It accepts USB midi input and
  outputs mono audio.
- `pico-loader/` A Rust / [Embassy](https://embassy.dev/) project that can run
  the plugin.
- USB midi input and audio output for the Pico

Significantly _not_ done just yet:

- A proper README that might be useful to anybody other than me
- Audio input to the Pico (both hardware and software parts)
- Any kind of testing of "real" plugins (I suspect very few will work without a
  lot of work)
- Proper port usage, the host just assumes port 0 is USB input and port 1 is
  audio out.
