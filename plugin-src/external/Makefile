REPOSITORIES := ams-lv2 mda-lv2 swh-lv2 midifilter.lv2 tap-lv2 abGate

AMS_LV2_REPOSITORY := git@github.com:Joeboy/ams-lv2.git
MDA_LV2_REPOSITORY := git@github.com:Joeboy/mda-lv2.git
SWH_LV2_REPOSITORY := git@github.com:Joeboy/swh-lv2.git
MIDIFILTER_LV2_REPOSITORY := git@github.com:Joeboy/midifilter.lv2.git
TAP_LV2_REPOSITORY := git@github.com:Joeboy/tap-lv2.git
ABGATE_REPOSITORY := git@github.com:Joeboy/abGate.git

PICOLV2_BUNDLE_ROOT ?= $(CURDIR)/build/picolv2
MDA_EPIANO_RESAMPLING ?= 3
MDA_PIANO_RESAMPLING ?= 4
MIDIFILTER_PICO_STRIP := $(if $(filter 0,$(STRIP)),true,arm-none-eabi-strip)
MIDIFILTER_PC_STRIP := $(if $(filter 0,$(STRIP)),true,strip)

.PHONY: all fetch fetch-ams-lv2 fetch-mda-lv2 fetch-swh-lv2 fetch-midifilter-lv2 fetch-tap-lv2 fetch-abgate \
	ams-lv2-pico ams-lv2-pc mda-lv2-pico mda-lv2-pc swh-lv2-pico swh-lv2-pc \
	midifilter-lv2-pico midifilter-lv2-pc tap-lv2-pico tap-lv2-pc abgate-pico abgate-pc \
	pico pc bundle bundle-pico bundle-pc test clean

all: bundle

fetch-ams-lv2:
	@if test -d "$(CURDIR)/ams-lv2/.git"; then \
		git -C "$(CURDIR)/ams-lv2" pull --ff-only; \
	else \
		git clone "$(AMS_LV2_REPOSITORY)" "$(CURDIR)/ams-lv2"; \
	fi

fetch-mda-lv2:
	@if test -d "$(CURDIR)/mda-lv2/.git"; then \
		git -C "$(CURDIR)/mda-lv2" pull --ff-only; \
	else \
		git clone "$(MDA_LV2_REPOSITORY)" "$(CURDIR)/mda-lv2"; \
	fi

fetch-swh-lv2:
	@if test -d "$(CURDIR)/swh-lv2/.git"; then \
		git -C "$(CURDIR)/swh-lv2" pull --ff-only; \
	else \
		git clone "$(SWH_LV2_REPOSITORY)" "$(CURDIR)/swh-lv2"; \
	fi

fetch-midifilter-lv2:
	@if test -d "$(CURDIR)/midifilter.lv2/.git"; then \
		git -C "$(CURDIR)/midifilter.lv2" pull --ff-only; \
	else \
		git clone "$(MIDIFILTER_LV2_REPOSITORY)" "$(CURDIR)/midifilter.lv2"; \
	fi

fetch-tap-lv2:
	@if test -d "$(CURDIR)/tap-lv2/.git"; then \
		git -C "$(CURDIR)/tap-lv2" pull --ff-only; \
	else \
		git clone "$(TAP_LV2_REPOSITORY)" "$(CURDIR)/tap-lv2"; \
	fi

fetch-abgate:
	@if test -d "$(CURDIR)/abGate/.git"; then \
		git -C "$(CURDIR)/abGate" pull --ff-only; \
	else \
		git clone "$(ABGATE_REPOSITORY)" "$(CURDIR)/abGate"; \
	fi

fetch: fetch-ams-lv2 fetch-mda-lv2 fetch-swh-lv2 fetch-midifilter-lv2 fetch-tap-lv2 fetch-abgate

ams-lv2-pico: fetch-ams-lv2
	@cd "$(CURDIR)/ams-lv2" && \
		./waf configure --picolv2 --out=build/picolv2 \
			--lv2dir="$(PICOLV2_BUNDLE_ROOT)/pico" && \
		./waf --out=build/picolv2 build && \
		./waf --out=build/picolv2 install

ams-lv2-pc: fetch-ams-lv2
	@cd "$(CURDIR)/ams-lv2" && \
		./waf configure --out=build/pc \
			--lv2dir="$(PICOLV2_BUNDLE_ROOT)/pc" && \
		./waf --out=build/pc build && \
		./waf --out=build/pc install

mda-lv2-pico: fetch-mda-lv2
	@cd "$(CURDIR)/mda-lv2" && \
		./waf configure --picolv2 \
			--epiano-resampling="$(MDA_EPIANO_RESAMPLING)" \
			--piano-resampling="$(MDA_PIANO_RESAMPLING)" --out=build/picolv2 \
			--lv2dir="$(PICOLV2_BUNDLE_ROOT)/pico" && \
		./waf --out=build/picolv2 build && \
		./waf --out=build/picolv2 install

mda-lv2-pc: fetch-mda-lv2
	@cd "$(CURDIR)/mda-lv2" && \
		./waf configure --out=build/pc \
			--lv2dir="$(PICOLV2_BUNDLE_ROOT)/pc" && \
		./waf --out=build/pc build && \
		./waf --out=build/pc install

swh-lv2-pico: fetch-swh-lv2
	@cd "$(CURDIR)/swh-lv2" && \
		$(MAKE) PICOLV2=1 clean && \
		$(MAKE) PICOLV2=1 all && \
		$(MAKE) PICOLV2=1 install-really INSTALL_DIR_REALLY="$(PICOLV2_BUNDLE_ROOT)/pico"

swh-lv2-pc: fetch-swh-lv2
	@cd "$(CURDIR)/swh-lv2" && \
		$(MAKE) clean && \
		$(MAKE) all && \
		$(MAKE) install-really INSTALL_DIR_REALLY="$(PICOLV2_BUNDLE_ROOT)/pc"

midifilter-lv2-pico: fetch-midifilter-lv2
	@cd "$(CURDIR)/midifilter.lv2" && \
		$(MAKE) PICOLV2=1 STRIP=$(MIDIFILTER_PICO_STRIP) clean && \
		$(MAKE) PICOLV2=1 STRIP=$(MIDIFILTER_PICO_STRIP) all && \
		$(MAKE) PICOLV2=1 STRIP=$(MIDIFILTER_PICO_STRIP) install LV2DIR="$(PICOLV2_BUNDLE_ROOT)/pico"

midifilter-lv2-pc: fetch-midifilter-lv2
	@cd "$(CURDIR)/midifilter.lv2" && \
		$(MAKE) STRIP=$(MIDIFILTER_PC_STRIP) clean && \
		$(MAKE) STRIP=$(MIDIFILTER_PC_STRIP) all && \
		$(MAKE) STRIP=$(MIDIFILTER_PC_STRIP) install LV2DIR="$(PICOLV2_BUNDLE_ROOT)/pc"

tap-lv2-pico: fetch-tap-lv2
	@cd "$(CURDIR)/tap-lv2" && \
		$(MAKE) PICOLV2=1 clean && \
		$(MAKE) PICOLV2=1 all && \
		$(MAKE) PICOLV2=1 install INSTALL_PATH="$(PICOLV2_BUNDLE_ROOT)/pico"

tap-lv2-pc: fetch-tap-lv2
	@cd "$(CURDIR)/tap-lv2" && \
		$(MAKE) clean && \
		$(MAKE) all && \
		$(MAKE) install INSTALL_PATH="$(PICOLV2_BUNDLE_ROOT)/pc"

abgate-pico: fetch-abgate
	@cd "$(CURDIR)/abGate" && \
		$(MAKE) PICOLV2=1 clean && \
		$(MAKE) PICOLV2=1 all && \
		$(MAKE) PICOLV2=1 install INSTALL_DIR="$(PICOLV2_BUNDLE_ROOT)/pico"

abgate-pc: fetch-abgate
	@cd "$(CURDIR)/abGate" && \
		$(MAKE) clean && \
		$(MAKE) TARGET=gate.so all && \
		$(MAKE) TARGET=gate.so install INSTALL_DIR="$(PICOLV2_BUNDLE_ROOT)/pc"

pico: ams-lv2-pico mda-lv2-pico swh-lv2-pico midifilter-lv2-pico tap-lv2-pico abgate-pico

pc: ams-lv2-pc mda-lv2-pc swh-lv2-pc midifilter-lv2-pc tap-lv2-pc abgate-pc

bundle: bundle-pico bundle-pc

bundle-pico: pico

bundle-pc: pc

test:
	@echo "No external plugin tests are defined"

clean:
	@rm -rf "$(CURDIR)/ams-lv2/build/picolv2" \
		"$(CURDIR)/ams-lv2/build/pc" \
		"$(CURDIR)/mda-lv2/build/picolv2" \
		"$(CURDIR)/mda-lv2/build/pc"
	@if test -d "$(CURDIR)/swh-lv2"; then \
		$(MAKE) -C "$(CURDIR)/swh-lv2" clean; \
	fi
	@if test -d "$(CURDIR)/midifilter.lv2"; then \
		$(MAKE) -C "$(CURDIR)/midifilter.lv2" clean; \
	fi
	@if test -d "$(CURDIR)/tap-lv2"; then \
		$(MAKE) -C "$(CURDIR)/tap-lv2" clean; \
	fi
	@if test -d "$(CURDIR)/abGate"; then \
		$(MAKE) -C "$(CURDIR)/abGate" clean; \
	fi
