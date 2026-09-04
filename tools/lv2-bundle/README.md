# lv2-bundle

`lv2-bundle` packages LV2 plugins, then creates an image that contains both the
PicoLv2 (`pico-loader`) firmware and the plugins package. This image can then be
flashed to a Raspberry Pi Pico 2. See [here](https://github.com/Joeboy/PicoLV2)
if you're somehow reading this without any context.

To create a plugin chain and flash it to the Pico, the complete process is:

1. Create your plugin chain as an Ingen graph
2. Acquire or build copies of: the Pico firmware (`pico-loader`); your desired
   plugins (built for PicoLv2); and the `lv2-bundle` utility.
3. Convert the Pico firmware ELF to a raw binary.
4. Pack the plugins and combine them with the firmware into a flash image.
5. Flash the image onto the Pico using either the USB bootloader or a debug
   probe.

## 1. Create your plugin chain as an Ingen graph file

This is a bit of a TODO, I haven't actually tried using "real" Ingen files yet.
They can be created by [Ingen](https://gitlab.com/drobilla/ingen). Which doesn't
seem to have a proper homepage that I can find, but see
[this video](https://www.youtube.com/watch?v=eMj-q5adAZ4) to get an idea.
Basically it allows you to connect up LV2 plugins, listen to the results on your
computer, then export the plugin graph (ie. effects chain or synth or whatever)
as a file that looks something like [this](../../graph/main.ttl).

## 2. Acquire or build required bits

### Building from the repo

Start at the root of this repo.

#### Build the plugins

At the time of writing the only plugins that will work are the ones in this
repo:

```sh
cd plugins
make
cd ..
```

#### Build lv2-bundle

```sh
cd tools/lv2-bundle
cargo build --release
cd ../..
```

the rest of this README assumes lv2-bundle is on your PATH

#### Build pico-loader

```sh
cd pico-loader
cargo build --release
cd ..
```

## 3. Convert the Pico firmware ELF to a raw binary

The idea is that eventually I'll just ship the binary, but documenting this here
anyway.

To convert the built pico-loader ELF to a raw binary that can be flashed onto
the Pico:

```sh
rust-objcopy -O binary \
  pico-loader/target/thumbv8m.main-none-eabihf/release/pico-loader \
  pico-loader.bin
```

## 4. Pack the plugins and combine with the firmware

Pass one `--plugin` option for each plugin. Each option contains the plugin's
LV2 URI, Pico binary, and TTL metadata file:

```sh
lv2-bundle pack \
  --output pico-image.bin \
  --firmware pico-loader.bin \
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

`pico-image.bin` is a 2 MiB raw flash image. Firmware starts at `0x10000000`;
the bundle starts at `0x10180000`.

## 5. Flash the Pico

### Option 1 (AS YET UNTESTED BY ME!): Flash via USB / UF2

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

## Bonus 2: Inspect The Image

```sh
lv2-bundle info -i pico-image.bin
```

```text
image: pico-image.bin (2097152 bytes)
firmware: 152284 bytes (0x10000000..0x100252dc)
bundle: 524288 bytes (0x10180000..), format version 2
plugins: 3
  [0] https://joebutton.co.uk/lv2/tine-piano (binary 24476 bytes, metadata 738 bytes)
  [1] https://joebutton.co.uk/lv2/string-synth (binary 21632 bytes, metadata 744 bytes)
  [2] https://joebutton.co.uk/lv2/delay-poc (binary 17888 bytes, metadata 1183 bytes)
graph: 2 nodes, 1 edges
  node[0] https://joebutton.co.uk/lv2/tine-piano
  node[1] https://joebutton.co.uk/lv2/delay-poc
  edge[0] node[0]:0 -> node[1]:0
```
