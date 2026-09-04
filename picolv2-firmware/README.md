# PicoLv2 Firmware

Firmware for the Raspberry Pi Pico 2, written in Rust /
[Embassy](https://github.com/embassy-rs/embassy).

To use it, you need to build it, use `picolv2image create` to combine it with a
plugin graph, then flash it to a Raspberry Pi Pico 2. See the
[picolv2image README](../picolv2-image/README.md) for more detailed
instructions.

## Build

```sh
cd picolv2-firmware
cargo build --release
```
