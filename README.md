# ee

Interactive environment file picker — a single-binary Rust CLI.

## Usage

```sh
eval $(ee)
```

Presents a full-screen TUI for selecting files from `~/.config/envvars/`. On selection it prints `source <path>` to stdout so the calling shell can `eval` it.

## Keyboard

| Key | Action |
|-----|--------|
| Type | Filter list |
| Backspace | Delete last char |
| ↑ / ↓ | Move selection |
| Enter | Source selected file |
| Esc | Quit without output |

## Install

```sh
make install
```

Builds a release binary and copies it to `~/.local/share/omarchy/bin/ee`.

## Build

```sh
cargo build --release
# binary at target/release/ee
```

## Requirements

- Files are read from `~/.config/envvars/` (created automatically if missing).
- Most-recently-used order is persisted in `~/.config/envvars/.ee_history`.
