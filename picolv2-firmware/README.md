# PicoLv2 Firmware

Firmware for the Raspberry Pi Pico 2, written in Rust /
[Embassy](https://github.com/embassy-rs/embassy).

## Build

```sh
cd picolv2-firmware
cargo build --release
```

## Logging and diagnostics

The default log level is `info`, which includes informational, warning, and
error messages. Override `DEFMT_LOG` to select `off`, `error`, `warn`, `info`,
`debug`, or `trace`. The `debug` and `trace` levels also enable heap statistics,
plugin timing, and audio xrun measurements.

```sh
cd picolv2-firmware
DEFMT_LOG=warn cargo build --release
DEFMT_LOG=debug cargo build --release
```

## Usage

To use it, you need to build it, use `picolv2-image create` to combine it with a
plugin graph, then flash it to a Raspberry Pi Pico 2. See the
[picolv2-image README](../picolv2-image/README.md) for more detailed
instructions.
