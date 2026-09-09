STRIP ?= 1
PLUGINS := note

.PHONY: all pico pc bundle bundle-pico bundle-pc test clean

all: pico pc

pico:
	@for plugin in $(PLUGINS); do \
		$(MAKE) -C $$plugin pico STRIP=$(STRIP) || exit 1; \
	done

pc:
	@for plugin in $(PLUGINS); do \
		$(MAKE) -C $$plugin pc STRIP=$(STRIP) || exit 1; \
	done

PICOLV2_BUNDLE_ROOT ?= $(CURDIR)/build/picolv2

bundle: bundle-pico bundle-pc

bundle-pico:
	@for plugin in $(PLUGINS); do \
		$(MAKE) -C $$plugin bundle TARGET=pico STRIP=$(STRIP) PICOLV2_PATH=$(PICOLV2_BUNDLE_ROOT)/pico || exit 1; \
	done

bundle-pc:
	@for plugin in $(PLUGINS); do \
		$(MAKE) -C $$plugin bundle TARGET=pc STRIP=$(STRIP) PICOLV2_PATH=$(PICOLV2_BUNDLE_ROOT)/pc || exit 1; \
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
