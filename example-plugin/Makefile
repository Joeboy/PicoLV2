CC := arm-none-eabi-gcc
READELF := arm-none-eabi-readelf

# Matches the Pico 2 (RP2350), Cortex-M33 with hardware FP, thumbv8m.main.
MCPU := -mcpu=cortex-m33 -mthumb -mfloat-abi=hard -mfpu=fpv5-sp-d16

CFLAGS := $(MCPU) -fPIC -ffreestanding -fno-builtin -Wall -Wextra -O2
LDFLAGS := -shared -nostdlib -Wl,-Bsymbolic -Wl,--no-undefined

BUILD_DIR := build
SRC := src/plugin.c
OUT := $(BUILD_DIR)/plugin.so

.PHONY: all clean info

all: $(OUT)

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

$(OUT): $(SRC) | $(BUILD_DIR)
	$(CC) $(CFLAGS) $(LDFLAGS) -o $(OUT) $(SRC)

info: $(OUT)
	$(READELF) -h $(OUT)
	$(READELF) -d $(OUT)
	$(READELF) -s $(OUT)

clean:
	rm -rf $(BUILD_DIR)
