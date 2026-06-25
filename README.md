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

---

# gg

Per-directory command shortcuts. Configure a command once in a project, then run it from that directory by typing `gg`. Each directory can hold a default plus any number of named commands.

## Usage

```sh
gg                       # run the default command for this directory
gg <name>                # run a named command, e.g. `gg build`
gg <name> [args…]        # run it with extra args, e.g. `gg test --watch`
gg set <command…>        # set/replace the default command for this directory
gg set <name> -- <cmd…>  # set/replace a named command
gg rm [name]             # remove the default (or named) command
gg rm --all              # remove every command for this directory
gg list [--all]          # list commands for this dir (or all dirs)
gg edit                  # open the config file in $EDITOR
gg report                # show a usage report
gg help                  # show help
```

Example:

```sh
cd ~/work/api    && gg set pnpm dev
cd ~/work/worker && gg set pnpm run start:force
cd ~/work/api    && gg          # runs: pnpm dev
```

Running bare `gg` in a directory that has named commands but no default opens a fuzzy-filter picker (type to filter, ↑/↓ to move, Enter to run, Esc to cancel).

## Behavior

- Commands run through `$SHELL -c`, so pipes, `&&`, and env expansion work.
- The command's exit code is propagated.
- Lookups walk up to the nearest ancestor directory that has commands configured, so a config at a repo/monorepo root applies to every subdirectory. `gg set` always writes to the current directory.
- Commands run from the directory they were configured in.
- Setting a command replaces any existing one in the same slot.

## Storage

- Commands live in `~/.config/gg/commands` (`path⇥name⇥command`, empty name = default), keyed by the absolute directory path.
- Run counts live in `~/.config/gg/counts`.
