STRIP ?= 1
PLUGIN_CATEGORIES := internal external throwaway

.PHONY: all pico pc bundle bundle-pico bundle-pc install test clean

all: bundle

pico:
	@for category in $(PLUGIN_CATEGORIES); do \
		$(MAKE) -C $$category pico STRIP=$(STRIP) || exit 1; \
	done

pc:
	@for category in $(PLUGIN_CATEGORIES); do \
		$(MAKE) -C $$category pc STRIP=$(STRIP) || exit 1; \
	done

PICOLV2_BUNDLE_ROOT ?= $(CURDIR)/build/picolv2

bundle: bundle-pico bundle-pc

bundle-pico:
	@for category in $(PLUGIN_CATEGORIES); do \
		$(MAKE) -C $$category bundle-pico STRIP=$(STRIP) PICOLV2_BUNDLE_ROOT=$(PICOLV2_BUNDLE_ROOT) || exit 1; \
	done

bundle-pc:
	@for category in $(PLUGIN_CATEGORIES); do \
		$(MAKE) -C $$category bundle-pc STRIP=$(STRIP) PICOLV2_BUNDLE_ROOT=$(PICOLV2_BUNDLE_ROOT) || exit 1; \
	done

PICOLV2_PLUGINS_ROOT ?= $(CURDIR)/../plugins

install: bundle
	mkdir -p $(PICOLV2_PLUGINS_ROOT)/pc $(PICOLV2_PLUGINS_ROOT)/pico
	cp -a $(PICOLV2_BUNDLE_ROOT)/pc/. $(PICOLV2_PLUGINS_ROOT)/pc/
	cp -a $(PICOLV2_BUNDLE_ROOT)/pico/. $(PICOLV2_PLUGINS_ROOT)/pico/

test:
	@for category in $(PLUGIN_CATEGORIES); do \
		$(MAKE) -C $$category test STRIP=$(STRIP) || exit 1; \
	done

clean:
	@for category in $(PLUGIN_CATEGORIES); do \
		$(MAKE) -C $$category clean || exit 1; \
	done
	rm -rf $(PICOLV2_BUNDLE_ROOT)
