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

## Usage

!!!TODO!!!

```sh
PICOLV2_PATH=plugins/build/picolv2/pico \
picolv2-image create \
  --firmware-elf picolv2-firmware/target/thumbv8m.main-none-eabihf/release/picolv2-firmware \
  --ingen graph/main.ttl \
  --plugin https://joebutton.co.uk/lv2/tine-piano \
  --plugin https://joebutton.co.uk/lv2/string-synth \
  --plugin https://joebutton.co.uk/lv2/delay-poc \
  --output pico-image.bin
```