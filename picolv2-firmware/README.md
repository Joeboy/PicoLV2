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
picolv2-image create \
  --firmware-elf picolv2-firmware/target/thumbv8m.main-none-eabihf/release/picolv2-firmware \
  --ingen graph/main.ttl \
  --plugin https://joebutton.co.uk/lv2/tine-piano \
    plugins/tine-piano/build/pico/plugin.so \
    plugins/tine-piano/tine-piano.lv2/tine-piano.ttl \
  --plugin https://joebutton.co.uk/lv2/string-synth \
    plugins/string-synth/build/pico/plugin.so \
    plugins/string-synth/string-synth.lv2/string-synth.ttl \
  --plugin https://joebutton.co.uk/lv2/delay-poc \
    plugins/delay/build/pico/plugin.so \
    plugins/delay/delay.lv2/delay.ttl \
  --output pico-image.bin
```