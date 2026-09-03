# lv2-bundle-format

`lv2-bundle-format` defines the flash-resident PicoLV2 plugin bundle format. It
is a `#![no_std]` library so the same reader can be used by the Pico firmware
and by host-side tools.

## Bundle contents

A bundle contains a little-endian header followed by a sequence of entries:

- magic: `PICO LV2`
- format version: `1`
- entry count
- for each entry: URI length, binary length, metadata length, URI, binary, and
  TTL metadata

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

This is an internal format for PicoLV2. It currently stores raw plugin binaries
and raw TTL files. I'm making it up as I go along so don't expect this to be
stable.
