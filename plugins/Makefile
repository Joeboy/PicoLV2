STRIP ?= 1
PLUGINS := amp-lv2 delay-lv2 monosynth-lv2 oxynth-lv2 string-synth-lv2 tine-piano-lv2

.PHONY: all pico linux test clean

all: pico linux

pico:
	@for plugin in $(PLUGINS); do \
		$(MAKE) -C $$plugin pico STRIP=$(STRIP) || exit 1; \
	done

linux:
	@for plugin in $(PLUGINS); do \
		$(MAKE) -C $$plugin linux STRIP=$(STRIP) || exit 1; \
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
