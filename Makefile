BINARIES := ee eee
INSTALL_DIR := $(HOME)/.local/share/omarchy/bin

.PHONY: build release install clean

build:
	cargo build

release:
	cargo build --release
	$(foreach bin,$(BINARIES),cp target/release/$(bin) $(bin);)

install: release
	mkdir -p $(INSTALL_DIR)
	$(foreach bin,$(BINARIES),cp target/release/$(bin) $(INSTALL_DIR)/$(bin);)
	@echo "Installed to $(INSTALL_DIR)"

clean:
	cargo clean
	$(foreach bin,$(BINARIES),rm -f $(bin);)
