# picolv2-image-format

`picolv2-image-format` defines the flash-resident PicoLV2 plugin bundle format. It
is a `#![no_std]` library so the same reader can be used by the Pico firmware
and by host-side tools.

## Bundle contents

A bundle contains a little-endian header followed by a sequence of entries:

- magic: `PICO LV2`
- format version: `2`
- entry count
- for each entry: URI length, binary length, metadata length, URI, binary, and
  compact plugin metadata
- a compiled plugin graph containing nodes, edges, audio outputs, parameter
  overrides, and MIDI CC bindings

Graph format version 4 stores each MIDI binding as a target node and LV2 port,
a zero-based MIDI channel, controller number, minimum and maximum values, and
flags for logarithmic, integer, toggle, or trigger mapping. The reader rejects
graph data written for any other format version.

The library does not allocate or copy entry data. `Bundle::parse` validates the
header and all entry bounds, and `Bundle::find` returns borrowed slices for a
matching URI.

## Flash layout

The current firmware reserves the following region:

- flash base: `0x10000000`
- bundle address: `0x10180000`
- reserved bundle size: 512 KiB
- current image size: 2 MiB

The address and size are exported as `FLASH_ADDRESS` and `MAX_SIZE`.

## Compatibility

This is an internal format for PicoLV2. I'm making it up as I go along so don't
expect this to be stable.
