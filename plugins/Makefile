STRIP ?= 1
PLUGINS := amp delay monosynth oxynth string-synth tine-piano

.PHONY: all pico linux bundle bundle-pico bundle-linux test clean

all: pico linux

pico:
	@for plugin in $(PLUGINS); do \
		$(MAKE) -C $$plugin pico STRIP=$(STRIP) || exit 1; \
	done

linux:
	@for plugin in $(PLUGINS); do \
		$(MAKE) -C $$plugin linux STRIP=$(STRIP) || exit 1; \
	done

PICOLV2_BUNDLE_ROOT ?= $(CURDIR)/build/picolv2

bundle: bundle-pico bundle-linux

bundle-pico:
	@for plugin in $(PLUGINS); do \
		$(MAKE) -C $$plugin bundle TARGET=pico STRIP=$(STRIP) PICOLV2_PATH=$(PICOLV2_BUNDLE_ROOT)/pico || exit 1; \
	done

bundle-linux:
	@for plugin in $(PLUGINS); do \
		$(MAKE) -C $$plugin bundle TARGET=linux STRIP=$(STRIP) PICOLV2_PATH=$(PICOLV2_BUNDLE_ROOT)/linux || exit 1; \
	done

test:
	@for plugin in $(PLUGINS); do \
		if grep -q "^test:" $$plugin/Makefile 2>/dev/null; then \
			$(MAKE) -C $$plugin test STRIP=$(STRIP) || exit 1; \
		fi; \
	done

clean:
	@for plugin in $(PLUGINS); do \
		$(MAKE) -C $$plugin clean || exit 1; \
	done
