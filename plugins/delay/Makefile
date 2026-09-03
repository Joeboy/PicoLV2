TARGET ?= pico
STRIP ?= 1

BUILD_DIR := build/$(TARGET)
SRC := src/plugin.c
OUT := $(BUILD_DIR)/plugin.so
BUNDLE_DIR := delay.lv2

ifeq ($(TARGET),pico)
CC := arm-none-eabi-gcc
READELF := arm-none-eabi-readelf
MCPU := -mcpu=cortex-m33 -mthumb -mfloat-abi=hard -mfpu=fpv5-sp-d16
CFLAGS := $(MCPU) -fPIC -ffreestanding -fno-builtin -Wall -Wextra -O2
LDFLAGS := -shared -nostdlib -Wl,-Bsymbolic -Wl,--no-undefined -Wl,--no-warnings
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

.PHONY: all pico linux bundle clean info

all: $(OUT)

pico:
	$(MAKE) TARGET=pico STRIP=$(STRIP)

linux:
	$(MAKE) TARGET=linux STRIP=$(STRIP)

bundle:
	$(MAKE) TARGET=linux STRIP=$(STRIP)
	cp build/linux/plugin.so $(BUNDLE_DIR)/plugin.so

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
