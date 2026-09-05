# picolv2-image

`picolv2-image` creates an image that can be flashed onto a Raspberry Pi Pico 2,
containing:

- The [picolv2-firmware](../picolv2-firmware/) firmware
- An Ingen graph representing a plugin chain
- The plugins used by the plugin chain

See [here](https://github.com/Joeboy/PicoLV2) if you're somehow reading this
without any context.

To create a plugin chain and flash it to the Pico, the complete process is:

1. Create your plugin chain as an Ingen graph
2. Acquire or build copies of: the Pico firmware (`picolv2-firmware`); your
   desired plugins (built for PicoLv2); and the `picolv2-image` utility.
3. Create a flash image from the plugins and firmware ELF.
4. Flash the image onto the Pico using either the USB bootloader or a debug
   probe.

## 1. Create your plugin chain as an Ingen graph file

Ingen graph bundles (`.ingen` directories) can be created by
[Ingen](https://gitlab.com/drobilla/ingen). I can't find a proper homepage for
Ingen, but see [this video](https://www.youtube.com/watch?v=eMj-q5adAZ4) to get
an idea. Basically it allows you to connect up LV2 plugins, listen to the
results on your computer, then export the plugin graph bundle (like
[graphs/tine-piano-plus-delay.ingen](../graphs/tine-piano-plus-delay.ingen)).

You should be able to use it to create things like guitar effects chains and
modular synthesizers.

You need to use plugins that have been built for PicoLv2, of which there are
currently very few (just the [ones in this repo](../plugins/)).

## 2. Acquire or build required bits

### Building from the repo

Start at the root of this repo.

#### Build picolv2-firmware

```sh
cd picolv2-firmware
cargo build --release
cd ..
```

#### Build picolv2-image utility

```sh
cd picolv2-image
cargo build --release
cd ..
```

the rest of this README assumes `picolv2-image` is on your PATH.

#### Build the plugins

At the time of writing the only plugins that will work are the ones in this
repo:

```sh
cd plugins
make bundle
cd ..
```

This creates plugin bundles under `plugins/build/picolv2/linux` and
`plugins/build/picolv2/pico`.

Individual plugins can also be installed directly under `PICOLV2_PATH`:

```sh
PICOLV2_PATH=$PWD/plugins/build/picolv2/pico \
  make -C plugins/tine-piano install-pico
PICOLV2_PATH=$PWD/plugins/build/picolv2/linux \
  make -C plugins/tine-piano install
```

## 2.5 Convert the picolv2-firmware ELF to binary (optional)

You probably don't need to do this as `picolv2-image` can now convert the raw
ELF file itself. In case you want to do it manually for some reason:

```sh
rust-objcopy -O binary \
  picolv2-firmware/target/thumbv8m.main-none-eabihf/release/picolv2-firmware \
  picolv2-firmware.bin
```

## 3. Create the flash image

```sh
PICOLV2_PATH=plugins/build/picolv2/pico \
picolv2-image create \
  --firmware-elf picolv2-firmware/target/thumbv8m.main-none-eabihf/release/picolv2-firmware \
  --ingen graphs/tine-piano-plus-delay.ingen \
  --output pico-image.bin
```

The plugin URIs included in the bundle are inferred from the ingen file. There
must be a valid PicoLV2 bundle under PICOLV2_PATH for each plugin used.

## 4. Flash the Pico

### Option 1 (AS YET UNTESTED BY ME!): Flash via USB / UF2

Convert the combined raw image to UF2:

```sh
picolv2-image uf2 \
  --input pico-image.bin \
  --output pico-image.uf2
```

Then put the Pico 2 into its USB bootloader mode and copy `pico-image.uf2` to
the mounted `RPI-RP2` drive.

## Option 2: Flash using a debug probe and probe-rs

```sh
probe-rs download \
  --chip RP235x \
  --binary-format bin \
  --base-address 0x10000000 \
  --verify \
  pico-image.bin && \
sleep 3 && \
probe-rs reset --chip RP235x
```

Plugin URIs must be unique and are resolved from `PICOLV2_PATH`; each bundle's
`manifest.ttl` supplies the binary and the matching `rdfs:seeAlso` declaration
locates the plugin TTL. Plugin metadata is parsed during image creation and
stored as compact port records; invalid or unsupported port metadata fails the
command. The bundle has a 512 KiB maximum size. `--ingen` accepts an Ingen graph
bundle directory (e.g. `graphs/tine-piano-plus-delay.ingen`), reading its
`manifest.ttl` to locate and parse the graph.

## Bonus: debugging with a debug probe and probe-rs

```sh
probe-rs attach \
  --chip RP235x \
  --rtt-scan-memory \
  picolv2-firmware/target/thumbv8m.main-none-eabihf/release/picolv2-firmware
```

## Bonus 2: Inspect The Image

```sh
picolv2-image info -i pico-image.bin
```

```text
image: pico-image.bin (2097152 bytes)
firmware: 152284 bytes (0x10000000..0x100252dc)
bundle: 524288 bytes (0x10180000..), format version 2
plugins: 3
  [0] https://joebutton.co.uk/lv2/tine-piano (binary 24476 bytes, metadata 40 bytes)
  [1] https://joebutton.co.uk/lv2/string-synth (binary 21632 bytes, metadata 40 bytes)
  [2] https://joebutton.co.uk/lv2/delay-poc (binary 17888 bytes, metadata 76 bytes)
graph: 2 nodes, 1 edges
  node[0] https://joebutton.co.uk/lv2/tine-piano
  node[1] https://joebutton.co.uk/lv2/delay-poc
  edge[0] node[0]:0 -> node[1]:0
```
