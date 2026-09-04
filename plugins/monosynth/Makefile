# Build for the Pico 2 (default) with `make`, or for a native Linux host
# with `make TARGET=linux` - useful for fast host-side testing of the plugin
# logic without needing hardware.
TARGET ?= pico
STRIP ?= 1

ifeq ($(TARGET),pico)
CC := arm-none-eabi-gcc
READELF := arm-none-eabi-readelf
# Matches the Pico 2 (RP2350), Cortex-M33 with hardware FP, thumbv8m.main.
MCPU := -mcpu=cortex-m33 -mthumb -mfloat-abi=hard -mfpu=fpv5-sp-d16
CFLAGS := $(MCPU) -fPIC -ffreestanding -fno-builtin -Wall -Wextra -O2
LDFLAGS := -shared -nostdlib -Wl,-Bsymbolic -Wl,--no-undefined -Wl,-z,max-page-size=0x1000 -Wl,--no-warnings
# -lgcc: the plugin does double-precision math (the LV2 `instantiate` ABI
# passes `sample_rate` as a double), but this FPU is single-precision only
# (fpv5-sp-d16), so double ops need libgcc's soft-float routines. Verified
# these link in as position-independent, relocation-free code. Must come
# after the object file on the link line so `ld` sees the undefined symbols
# it needs to satisfy before searching the archive.
LDLIBS := -lgcc -lc -lnosys
else ifeq ($(TARGET),linux)
CC := gcc
READELF := readelf
CFLAGS := -fPIC -Wall -Wextra -O2
LDFLAGS := -shared -Wl,-Bsymbolic -Wl,--no-undefined
LDLIBS :=
else
$(error Unknown TARGET '$(TARGET)', expected 'pico' or 'linux')
endif

ifneq ($(STRIP),0)
LDFLAGS += -Wl,--strip-debug
endif

BUILD_DIR := build/$(TARGET)
SRC := src/plugin.c
OUT := $(BUILD_DIR)/plugin.so

# LV2 bundle directory: manifest.ttl + monosynth.ttl describe the plugin
# for real LV2 hosts (e.g. jalv, https://gitlab.com/drobilla/jalv); the
# Linux binary is copied in alongside them since LV2 hosts expect a plugin's
# binary to live in its own bundle directory. Point `LV2_PATH` at the parent
# of this repo's monosynth directory (or symlink the bundle into ~/.lv2/)
# to make hosts like jalv find it: `jalv https://joebutton.co.uk/lv2/monosynth-poc`.
BUNDLE_SOURCE := monosynth.lv2
PICOLV2_PATH ?= build/picolv2/$(TARGET)
BUNDLE_DIR := $(PICOLV2_PATH)/monosynth.lv2

.PHONY: all pico linux bundle install install-pico test clean info

all: $(OUT)

pico:
	$(MAKE) TARGET=pico STRIP=$(STRIP)

linux:
	$(MAKE) TARGET=linux STRIP=$(STRIP)

bundle:
	$(MAKE) TARGET=$(TARGET) STRIP=$(STRIP)
	mkdir -p $(BUNDLE_DIR)
	cp $(BUNDLE_SOURCE)/*.ttl $(BUNDLE_DIR)/
	cp $(OUT) $(BUNDLE_DIR)/plugin.so

install:
	$(MAKE) TARGET=linux STRIP=$(STRIP) bundle

install-pico:
	$(MAKE) TARGET=pico STRIP=$(STRIP) bundle

SMOKE_TEST := ../../tools/lv2-smoke-test/smoke.c

test: bundle $(SMOKE_TEST)
	cc -std=c11 -Wall -Wextra -Werror -o build/linux/smoke $(SMOKE_TEST) -ldl -lm
	LV2_PATH="$(CURDIR)" ./build/linux/smoke --expect-frequency 440 ./build/linux/plugin.so

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

$(OUT): $(SRC) | $(BUILD_DIR)
	$(CC) $(CFLAGS) $(LDFLAGS) -o $(OUT) $(SRC) $(LDLIBS)

info: $(OUT)
	$(READELF) -h $(OUT)
	$(READELF) -d $(OUT)
	$(READELF) -s $(OUT)

clean:
	rm -rf build
	rm -f $(BUNDLE_DIR)/plugin.so

