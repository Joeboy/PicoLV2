# PicoLv2 Firmware

Firmware for the Raspberry Pi Pico 2, written in Rust /
[Embassy](https://github.com/embassy-rs/embassy).

## Build

```sh
cd picolv2-firmware
cargo build --release
```

## Build with diagnostics

Since the Pico2 is a bit underpowered, it might sometimes be useful to see which
plugins are eating a lot of cycles.

```sh
cd picolv2-firmware
cargo build --release --features perf-diagnostics
```

## Usage

To use it, you need to build it, use `picolv2-image create` to combine it with a
plugin graph, then flash it to a Raspberry Pi Pico 2. See the
[picolv2image README](../picolv2-image/README.md) for more detailed
instructions.
