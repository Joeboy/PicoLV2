REPOSITORIES := ams-lv2 mda-lv2 swh-lv2 midifilter.lv2 tap-lv2 abGate caps-lv2 LibreArp

AMS_LV2_REPOSITORY := git@github.com:Joeboy/ams-lv2.git
MDA_LV2_REPOSITORY := git@github.com:Joeboy/mda-lv2.git
SWH_LV2_REPOSITORY := git@github.com:Joeboy/swh-lv2.git
MIDIFILTER_LV2_REPOSITORY := git@github.com:Joeboy/midifilter.lv2.git
TAP_LV2_REPOSITORY := git@github.com:Joeboy/tap-lv2.git
ABGATE_REPOSITORY := git@github.com:Joeboy/abGate.git
CAPS_LV2_REPOSITORY := git@github.com:Joeboy/caps-lv2.git
LIBREARP_REPOSITORY := git@github.com:Joeboy/LibreArp.git

PICOLV2_BUNDLE_ROOT ?= $(CURDIR)/build/picolv2
MDA_EPIANO_RESAMPLING ?= 3
MDA_PIANO_RESAMPLING ?= 4
MIDIFILTER_PICO_STRIP := $(if $(filter 0,$(STRIP)),true,arm-none-eabi-strip)
MIDIFILTER_PC_STRIP := $(if $(filter 0,$(STRIP)),true,strip)

.PHONY: all fetch fetch-ams-lv2 fetch-mda-lv2 fetch-swh-lv2 fetch-midifilter-lv2 fetch-tap-lv2 fetch-abgate fetch-caps-lv2 fetch-librearp \
	ams-lv2-pico ams-lv2-pc mda-lv2-pico mda-lv2-pc swh-lv2-pico swh-lv2-pc \
	midifilter-lv2-pico midifilter-lv2-pc tap-lv2-pico tap-lv2-pc abgate-pico abgate-pc \
	caps-lv2-pico caps-lv2-pc librearp-pico librearp-pc \
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

fetch-caps-lv2:
	@if test -d "$(CURDIR)/caps-lv2/.git"; then \
		git -C "$(CURDIR)/caps-lv2" pull --ff-only; \
	else \
		git clone "$(CAPS_LV2_REPOSITORY)" "$(CURDIR)/caps-lv2"; \
	fi

fetch-librearp:
	@if test -d "$(CURDIR)/LibreArp/.git"; then \
		git -C "$(CURDIR)/LibreArp" switch lv2-pico; \
		git -C "$(CURDIR)/LibreArp" pull --ff-only; \
		git -C "$(CURDIR)/LibreArp" submodule update --init --recursive; \
	else \
		git clone --branch lv2-pico --recurse-submodules "$(LIBREARP_REPOSITORY)" "$(CURDIR)/LibreArp"; \
	fi

fetch: fetch-ams-lv2 fetch-mda-lv2 fetch-swh-lv2 fetch-midifilter-lv2 fetch-tap-lv2 fetch-abgate fetch-caps-lv2 fetch-librearp

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

caps-lv2-pico: fetch-caps-lv2
	@cd "$(CURDIR)/caps-lv2" && \
		$(MAKE) PICOLV2=1 clean && \
		$(MAKE) PICOLV2=1 all && \
		$(MAKE) PICOLV2=1 install LV2_DEST="$(PICOLV2_BUNDLE_ROOT)/pico"

caps-lv2-pc: fetch-caps-lv2
	@cd "$(CURDIR)/caps-lv2" && \
		$(MAKE) clean && \
		$(MAKE) all && \
		$(MAKE) install LV2_DEST="$(PICOLV2_BUNDLE_ROOT)/pc"

librearp-pico: fetch-librearp
	@cd "$(CURDIR)/LibreArp" && \
		$(MAKE) -f Makefile.picolv2 clean && \
		$(MAKE) -f Makefile.picolv2 all && \
		$(MAKE) -f Makefile.picolv2 install LV2DIR="$(PICOLV2_BUNDLE_ROOT)/pico"

librearp-pc: fetch-librearp
	@cd "$(CURDIR)/LibreArp" && \
		cmake -S . -B build/pc -DCMAKE_BUILD_TYPE=Release && \
		cmake --build build/pc --parallel && \
		mkdir -p "$(PICOLV2_BUNDLE_ROOT)/pc" && \
		cp -a build/pc/LibreArp_artefacts/Release/LV2/LibreArp.lv2 "$(PICOLV2_BUNDLE_ROOT)/pc/"

pico: ams-lv2-pico mda-lv2-pico swh-lv2-pico midifilter-lv2-pico tap-lv2-pico abgate-pico caps-lv2-pico librearp-pico

pc: ams-lv2-pc mda-lv2-pc swh-lv2-pc midifilter-lv2-pc tap-lv2-pc abgate-pc caps-lv2-pc librearp-pc

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
	@if test -d "$(CURDIR)/caps-lv2"; then \
		$(MAKE) -C "$(CURDIR)/caps-lv2" clean; \
	fi
	@if test -d "$(CURDIR)/LibreArp"; then \
		$(MAKE) -C "$(CURDIR)/LibreArp" -f Makefile.picolv2 clean; \
		rm -rf "$(CURDIR)/LibreArp/build/pc"; \
	fi
