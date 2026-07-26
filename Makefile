PREFIX  ?= $(HOME)/.local
BINDIR  := $(PREFIX)/bin
MENUDIR := $(PREFIX)/share/kio/servicemenus
BIN     := kde-wallpaper-import
MENU    := kde-wallpaper-importer.desktop

.PHONY: build test install uninstall clean

build:
	cargo build --release

test:
	cargo test

install: build
	install -Dm755 target/release/$(BIN) "$(BINDIR)/$(BIN)"
	install -d "$(MENUDIR)"
	sed 's|@BINARY@|$(BINDIR)/$(BIN)|g' share/kio/servicemenus/$(MENU).in > "$(MENUDIR)/$(MENU)"
	chmod 755 "$(MENUDIR)/$(MENU)"
	-kbuildsycoca6 --noincremental >/dev/null 2>&1

uninstall:
	rm -f "$(BINDIR)/$(BIN)" "$(MENUDIR)/$(MENU)"
	-kbuildsycoca6 --noincremental >/dev/null 2>&1

clean:
	cargo clean
