ROOT_DIR:=$(shell dirname $(realpath $(firstword $(MAKEFILE_LIST))))
VERSION=$(shell cat $(ROOT_DIR)/VERSION)
VERSION_MAJOR=$(shell echo $(VERSION) | cut -f1 -d.)
VERSION_MINOR=$(shell echo $(VERSION) | cut -f2 -d.)
VERSION_MICRO=$(shell echo $(VERSION) | cut -f3 -d.)
CLIB_SO_DEV=libbinkc.so
CLIB_SO_FULL=$(CLIB_SO_DEV).$(VERSION)
CLIB_HEADER=binkc.h
CLIB_SO_DEV_RELEASE=../target/release/$(CLIB_SO_DEV)
CLIB_SO_DEV_DEBUG=../target/debug/$(CLIB_SO_DEV)
CLIB_PKG_CONFIG=binkc.pc
PREFIX ?= /usr/local

#outdir is used by COPR as well: https://docs.pagure.org/copr.copr/user_documentation.html
outdir ?= $(ROOT_DIR)

CPU_BITS = $(shell getconf LONG_BIT)
ifeq ($(CPU_BITS), 32)
    LIBDIR ?= $(PREFIX)/lib
else
    LIBDIR ?= $(PREFIX)/lib$(CPU_BITS)
endif

INCLUDE_DIR ?= $(PREFIX)/include
PKG_CONFIG_LIBDIR ?= $(LIBDIR)/pkgconfig

SKIP_VENDOR_CREATION ?=0
RELEASE ?=0

.PHONY: debug
debug: $(CLIB_HEADER) $(CLIB_SO_DEV_DEBUG)
	mv $(CLIB_HEADER) ../target/debug/$(shell basename $(CLIB_HEADER))
	mv $(CLIB_SO_DEV_DEBUG) ../target/debug/$(CLIB_SO_FULL)
	ln -sfv $(CLIB_SO_FULL) ../target/debug/$(CLIB_SO_DEV)

$(CLIB_SO_DEV_RELEASE):
	cd .. && cargo build --all --release

$(CLIB_SO_DEV_DEBUG):
	cd .. && cargo build --all

clib: $(CLIB_HEADER) $(CLIB_SO_DEV_RELEASE) $(CLIB_PKG_CONFIG)
	mv $(CLIB_HEADER) ../target/release/$(shell basename $(CLIB_HEADER))
	mv $(CLIB_PKG_CONFIG) ../target/release/$(shell basename $(CLIB_PKG_CONFIG))
	mv $(CLIB_SO_DEV_RELEASE) ../target/release/$(CLIB_SO_FULL)
	ln -sfv $(CLIB_SO_FULL) ../target/release/$(CLIB_SO_DEV)

.PHONY: $(CLIB_HEADER)
$(CLIB_HEADER): $(CLIB_HEADER).in
	cp $(CLIB_HEADER).in $(CLIB_HEADER)
	sed -i -e 's/@_VERSION_MAJOR@/$(VERSION_MAJOR)/' \
		$(CLIB_HEADER)
	sed -i -e 's/@_VERSION_MINOR@/$(VERSION_MINOR)/' \
		$(CLIB_HEADER)
	sed -i -e 's/@_VERSION_MICRO@/$(VERSION_MICRO)/' \
		$(CLIB_HEADER)

.PHONY: $(CLIB_PKG_CONFIG)
$(CLIB_PKG_CONFIG): $(CLIB_PKG_CONFIG).in
	cp $(CLIB_PKG_CONFIG).in $(CLIB_PKG_CONFIG)
	sed -i -e 's|@VERSION@|$(VERSION)|' $(CLIB_PKG_CONFIG)
	sed -i -e 's|@PREFIX@|$(PREFIX)|' $(CLIB_PKG_CONFIG)
	sed -i -e 's|@LIBDIR@|$(LIBDIR)|' $(CLIB_PKG_CONFIG)
	sed -i -e 's|@INCLUDE_DIR@|$(INCLUDE_DIR)|' $(CLIB_PKG_CONFIG)

.PHONY: clib_check
clib_check: $(CLIB_SO_DEV_DEBUG) $(CLIB_HEADER)
	$(eval TMPDIR := $(shell mktemp -d))
	cp $(CLIB_SO_DEV_DEBUG) $(TMPDIR)/$(CLIB_SO_FULL)
	ln -sfv $(CLIB_SO_FULL) $(TMPDIR)/$(CLIB_SO_DEV)
	mv $(CLIB_HEADER) $(TMPDIR)/$(shell basename $(CLIB_HEADER))
	cc -g -Wall -Wextra -L$(TMPDIR) -I$(TMPDIR) \
		-o $(TMPDIR)/binkc_test clib/tests/binkc_test.c -lbinkc
	LD_LIBRARY_PATH=$(TMPDIR) \
		valgrind --trace-children=yes --leak-check=full \
		--error-exitcode=1 \
		$(TMPDIR)/binkc_test 1>/dev/null
	rm -rf $(TMPDIR)

rust_check:
	cd .. && cargo test -- --show-output;
	cd clib

check: rust_check clib_check

run_test: debug
	cc -g -Wall -Wextra -L../target/debug -I../target/debug \
		-o ../target/debug/binkc_test ./tests/binkc_test.c -lbinkc
	LD_LIBRARY_PATH=../target/debug \
		../target/debug/binkc_test

clean:
	- cd .. && cargo clean
	- rm -f target/debug/$(CLIB_SO_MAN)
	- rm -f target/debug/$(CLIB_SO_FULL)
	- rm -f $(CLIB_HEADER)

install: $(CLI_EXEC_RELEASE) clib
	install -p -v -D -m755 $(CLI_EXEC_RELEASE) \
		$(DESTDIR)$(PREFIX)/bin/$(CLI_EXEC)
	install -p -D -m755 $(CLIB_SO_DEV_RELEASE) \
		$(DESTDIR)$(LIBDIR)/$(CLIB_SO_FULL)
	ln -sfv $(CLIB_SO_FULL) $(DESTDIR)$(LIBDIR)/$(CLIB_SO_MAN)
	ln -sfv $(CLIB_SO_FULL) $(DESTDIR)$(LIBDIR)/$(CLIB_SO_DEV)
	install -p -v -D -m644 $(CLIB_HEADER) \
		$(DESTDIR)$(INCLUDE_DIR)/$(shell basename $(CLIB_HEADER))
	install -p -v -D -m644 $(CLIB_PKG_CONFIG) \
		$(DESTDIR)$(PKG_CONFIG_LIBDIR)/$(shell basename $(CLIB_PKG_CONFIG))

uninstall:
	- rm -fv $(DESTDIR)$(PREFIX)/bin/$(CLI_EXEC)
	- rm -fv $(DESTDIR)$(LIBDIR)/$(CLIB_SO_DEV)
	- rm -fv $(DESTDIR)$(LIBDIR)/$(CLIB_SO_MAN)
	- rm -fv $(DESTDIR)$(LIBDIR)/$(CLIB_SO_FULL)
	- rm -fv $(DESTDIR)$(INCLUDE_DIR)/$(shell basename $(CLIB_HEADER))
	- rm -fv $(DESTDIR)$(INCLUDE_DIR)/$(shell basename $(CLIB_PKG_CONFIG))

