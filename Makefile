BINARIES := ee eee rr
INSTALL_DIR ?= $(HOME)/.local/bin

.PHONY: build release install clean fmt lint test check

build:
	cargo build

release:
	cargo build --release
	$(foreach bin,$(BINARIES),cp target/release/$(bin) $(bin);)

install: release
	mkdir -p $(INSTALL_DIR)
	$(foreach bin,$(BINARIES),cp target/release/$(bin) $(INSTALL_DIR)/$(bin);)
	@echo "Installed to $(INSTALL_DIR)"
	@echo "Note: this only installs the binaries. For the ee/eee shell"
	@echo "integration (so you don't have to type 'eval \$$(ee)'), use"
	@echo "install.sh instead — see the README."

fmt:
	cargo fmt

lint:
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings

test:
	cargo test

check: lint test build

clean:
	cargo clean
	$(foreach bin,$(BINARIES),rm -f $(bin);)
