TARGET ?= pico
STRIP ?= 1

BUILD_DIR := build/$(TARGET)
SRC := src/plugin.c
OUT := $(BUILD_DIR)/plugin.so
BUNDLE_SOURCE := delay.lv2
PICOLV2_PATH ?= build/picolv2/$(TARGET)
BUNDLE_DIR := $(PICOLV2_PATH)/delay.lv2

ifeq ($(TARGET),pico)
CC := arm-none-eabi-gcc
READELF := arm-none-eabi-readelf
MCPU := -mcpu=cortex-m33 -mthumb -mfloat-abi=hard -mfpu=fpv5-sp-d16
CFLAGS := $(MCPU) -fPIC -ffreestanding -fno-builtin -Wall -Wextra -O2
# malloc/calloc/free/realloc resolve picolv2_alloc/picolv2_dealloc against
# picolv2-firmware's tracked heap at load time, so they stay undefined here.
LDFLAGS := -shared -nostdlib -Wl,-Bsymbolic -Wl,-z,undefs -Wl,-z,max-page-size=0x1000 -Wl,--no-warnings
LDLIBS := -lgcc -Wl,--start-group -lc -lm -lnosys -Wl,--end-group
EXTRA_SRC := ../pico-alloc.c
else ifeq ($(TARGET),linux)
CC := gcc
READELF := readelf
CFLAGS := -fPIC -Wall -Wextra -O2
LDFLAGS := -shared -Wl,-Bsymbolic -Wl,--no-undefined
LDLIBS :=
EXTRA_SRC :=
else
$(error Unknown TARGET '$(TARGET)', expected 'pico' or 'linux')
endif

ifneq ($(STRIP),0)
LDFLAGS += -Wl,--strip-debug
endif

.PHONY: all pico linux bundle install install-pico clean info

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

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

$(OUT): $(SRC) $(EXTRA_SRC) | $(BUILD_DIR)
	$(CC) $(CFLAGS) $(LDFLAGS) -o $(OUT) $(SRC) $(EXTRA_SRC) $(LDLIBS)

info: $(OUT)
	$(READELF) -h $(OUT)
	$(READELF) -d $(OUT)
	$(READELF) -s $(OUT)

clean:
	rm -rf build
	rm -f $(BUNDLE_DIR)/plugin.so
