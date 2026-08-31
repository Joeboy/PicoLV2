# Build the Rust Oxynth LV2 plugin for Pico 2 (default) or Linux.
TARGET ?= pico

ifeq ($(TARGET),pico)
CARGO_TARGET := thumbv8m.main-none-eabihf
RUST_ARCHIVE := target/$(CARGO_TARGET)/release/liboxynth_lv2.a
LINKER := arm-none-eabi-gcc
READELF := arm-none-eabi-readelf
MCPU := -mcpu=cortex-m33 -mthumb -mfloat-abi=hard -mfpu=fpv5-sp-d16
LDFLAGS := $(MCPU) -shared -nostdlib -Wl,-Bsymbolic -Wl,--no-undefined -Wl,--gc-sections -Wl,-z,noexecstack
LDLIBS := -lgcc
else ifeq ($(TARGET),linux)
CARGO_TARGET := x86_64-unknown-linux-gnu
RUST_ARCHIVE := target/$(CARGO_TARGET)/release/liboxynth_lv2.a
LINKER := gcc
READELF := readelf
LDFLAGS := -shared -Wl,-Bsymbolic -Wl,--no-undefined -Wl,--gc-sections -Wl,-z,noexecstack
LDLIBS := -ldl -lpthread -lm
else
$(error Unknown TARGET '$(TARGET)', expected 'pico' or 'linux')
endif

BUILD_DIR := build/$(TARGET)
OUT := $(BUILD_DIR)/plugin.so
BUNDLE_DIR := oxynth.lv2

.PHONY: all pico linux bundle clean info

all: $(OUT)

pico:
	$(MAKE) TARGET=pico

linux:
	$(MAKE) TARGET=linux

bundle:
	$(MAKE) TARGET=linux
	cp build/linux/plugin.so $(BUNDLE_DIR)/plugin.so

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

$(RUST_ARCHIVE): Cargo.toml rust-toolchain.toml src/lib.rs src/abi.rs src/synth.rs
	cargo build --release --target $(CARGO_TARGET)

$(OUT): $(RUST_ARCHIVE) | $(BUILD_DIR)
	$(LINKER) $(LDFLAGS) -Wl,-u,lv2_descriptor -o $(OUT) $(RUST_ARCHIVE) $(LDLIBS)

info: $(OUT)
	$(READELF) -h $(OUT)
	$(READELF) -d $(OUT)
	$(READELF) -s $(OUT)

clean:
	rm -rf build
	rm -f $(BUNDLE_DIR)/plugin.so
	cargo clean
