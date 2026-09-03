STRIP ?= 1
PLUGINS := amp delay monosynth oxynth string-synth tine-piano

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
