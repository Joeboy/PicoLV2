# lv2-bundle

`lv2-bundle` packages LV2 plugins, then creates an image that contains both the
PicoLv2 (`pico-loader`) firmware and the plugins package. This image can then be
flashed to a Raspberry Pi Pico 2. See [here](https://github.com/Joeboy/PicoLV2)
if you're somehow reading this without any context.

The complete process is:

1. Acquire or build copies of the Pico firmware (`pico-loader`), your desired
   plugins and the `lv2-bundle` utility. For the latter just do
   `cargo build --release` in the `lv2-bundle` folder.
2. Pack plugins into a bundle.
3. Build the Pico firmware ELF and convert to a raw binary.
4. Combine the firmware and bundle.
5. Flash the combined image using either the USB bootloader or a debug probe.

## 1. Acquire or build required bits

## 2. Pack the plugins

Pass one `--plugin` option for each plugin. Each option contains the plugin's
LV2 URI, Pico binary, and TTL metadata file:

```sh
lv2-bundle pack \
  --output plugins.bundle \
  --ingen graph/main.ttl \
  --plugin https://joebutton.co.uk/lv2/tine-piano \
    plugins/tine-piano/build/pico/plugin.so \
    plugins/tine-piano/tine-piano.lv2/tine-piano.ttl \
  --plugin https://joebutton.co.uk/lv2/string-synth \
    plugins/string-synth/build/pico/plugin.so \
    plugins/string-synth/string-synth.lv2/string-synth.ttl \
  --plugin https://joebutton.co.uk/lv2/delay-poc \
    plugins/delay/build/pico/plugin.so \
    plugins/delay/delay.lv2/delay.ttl
```

Plugin URIs must be unique. Binary and metadata files are stored unchanged. The
bundle has a 512 KiB maximum size. `--ingen` accepts Ingen's serialized Turtle
graph (`main.ttl`), reading `ingen:Block`, `lv2:prototype`, and
`ingen:Arc`/`ingen:tail`/`ingen:head` statements. The packer converts this to
the compact `PICO GRP` payload used on the Pico. Blocks are emitted in the
Turtle order and must already be topologically ordered. The current host uses
the first audio input/output on each block and renders the highest-index sink;
control-port arcs and multi-port routing are not yet supported.

## 3. Build the Pico firmware ELF and convert to a raw binary

The idea is that eventually I'll just ship the binary, but documenting this here
anyway.

Create the pico-loader ELF file by running this From the `pico-loader` folder:

```sh
cargo build --release
```

Then we need to convert it to a binary that can be flashed onto the Pico:

```sh
rust-objcopy -O binary \
  pico-loader/target/thumbv8m.main-none-eabihf/release/pico-loader \
  pico-loader.bin
```

## 4. Combine firmware and bundle

```sh
lv2-bundle combine \
  --firmware pico-loader.bin \
  --bundle plugins.bundle \
  --output pico-image.bin
```

`pico-image.bin` is a 2 MiB raw flash image. Firmware starts at `0x10000000`;
the bundle starts at `0x10180000`.

## Flash the Pico

### Option 1 (UNTESTED!): Flash via USB / UF2

Convert the combined raw image to UF2:

```sh
lv2-bundle uf2 \
  --input pico-image.bin \
  --output pico-image.uf2
```

Then put the Pico 2 into its USB bootloader mode and copy `pico-image.uf2` to
the mounted `RPI-RP2` drive.

## Option 2: Flash using a debug probe

```sh
probe-rs download \
  --chip RP235x \
  --binary-format bin \
  --base-address 0x10000000 \
  --verify \
  pico-image.bin && \
probe-rs reset --chip RP235x
```

## Bonus: debugging with a debug probe

```sh
probe-rs attach \
  --chip RP235x \
  --rtt-scan-memory \
  pico-loader/target/thumbv8m.main-none-eabihf/release/pico-loader
```
