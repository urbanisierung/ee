BINARY := ee
INSTALL_DIR := $(HOME)/.local/share/omarchy/bin

.PHONY: build release install clean

build:
	cargo build

release:
	cargo build --release
	cp target/release/$(BINARY) $(BINARY)

install: release
	mkdir -p $(INSTALL_DIR)
	cp target/release/$(BINARY) $(INSTALL_DIR)/$(BINARY)
	@echo "Installed to $(INSTALL_DIR)/$(BINARY)"

clean:
	cargo clean
	rm -f $(BINARY)
