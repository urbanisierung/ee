# ee tools

[![CI](https://github.com/urbanisierung/ee/actions/workflows/ci.yml/badge.svg)](https://github.com/urbanisierung/ee/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/urbanisierung/ee)](https://github.com/urbanisierung/ee/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Three tiny, single-binary terminal tools: fuzzy-pick and load env files (`ee`),
jump straight into any cloned repo (`eee`), and run the right command for
whatever directory you're in (`rr`). No config files to hand-edit, no shell
plugin frameworks — just three small Rust binaries.

**[Website & docs →](https://ee.u11g.com)**

## Install

```sh
curl -fsSL https://ee.u11g.com/install.sh | sh
```

This downloads the latest release for your platform (Linux or macOS),
installs `ee`, `eee`, and `rr` to `~/.local/bin`, and adds shell functions to
your rc file so `ee`/`eee` work immediately — see [Why the shell
integration?](#why-the-shell-integration) below.

Flags: `--no-shell` (skip rc-file editing and print the snippet instead),
`--bin-dir DIR` (custom install location), `--uninstall`. Pin a version with
`EE_VERSION=v0.2.0`.

### From source

```sh
git clone https://github.com/urbanisierung/ee.git
cd ee
make install                 # builds and copies to ~/.local/bin
```

or, without cloning:

```sh
cargo install --git https://github.com/urbanisierung/ee
```

Either way, `cargo install`/`make install` only installs the binaries — you
still need the shell functions below for `ee`/`eee`.

### Shell integration

Add to `~/.bashrc` / `~/.zshrc`:

```sh
ee()  { eval "$(command ee  "$@")"; }
eee() { eval "$(command eee "$@")"; }
```

or to `~/.config/fish/config.fish`:

```fish
function ee;  eval (command ee  $argv); end
function eee; eval (command eee $argv); end
```

`rr` needs no wrapper — it runs directly.

#### Why the shell integration?

`ee` and `eee` pick a file/repo interactively and then need your *current
shell* to `cd` into a directory or `source` a file. A child process can't do
that to its parent shell — it's an OS-level restriction, not a limitation of
these tools. So each binary prints the command it wants run (`source <path>`
or `cd <path>`) to stdout, and the wrapper function above evaluates it in your
shell. Once it's set up you just type `ee` / `eee` like any other command; the
`eval` is invisible. (This is also exactly why `eval $(ee)` or
`eval "$(eee)"` show up if you skip the wrapper — that's the raw, unwrapped
form.)

## `ee` — env file picker

Fuzzy-pick a file from `~/.config/envvars/` and load it into your shell.

```sh
ee
```

| Key | Action |
|-----|--------|
| Type | Filter list |
| Backspace | Delete last char |
| ↑ / ↓ | Move selection |
| → | Preview file contents |
| Enter | Source selected file |
| Ctrl-E | Open selected file in `$EDITOR` |
| Ctrl-R | Show a usage report (most-sourced files) |
| Esc | Quit without output |

Files are stored as plain files under `~/.config/envvars/` — put your
`.env`-style files there and `ee` picks them up. Most-recently-used order and
usage counts are tracked automatically.

## `eee` — repo picker

Fuzzy-pick a repo from `~/github.com/<org>/<repo>` and `cd` straight into it.

```sh
eee
```

| Key | Action |
|-----|--------|
| Type | Filter list |
| Backspace | Delete last char |
| ↑ / ↓ | Move selection |
| Enter | `cd` into selected repo |
| Ctrl-R | Show a usage report |
| Ctrl-E | Toggle sort: recency (last 3 days) vs. alphabetical |
| Esc | Quit without output |

Clone your repos under `~/github.com/<org>/<repo>` (two levels deep) and
`eee` discovers them automatically — no registration step.

## `rr` — per-directory command shortcuts

Configure a command once in a project, then run it from that directory by
typing `rr`. Each directory can hold a default plus any number of named
commands.

```sh
rr                       # run the default command for this directory
rr <name>                # run a named command, e.g. `rr build`
rr <name> [args…]        # run it with extra args, e.g. `rr test --watch`
rr set <command…>        # set/replace the default command for this directory
rr set <name> -- <cmd…>  # set/replace a named command
rr rm [name]             # remove the default (or named) command
rr rm --all              # remove every command for this directory
rr list [--all]          # list commands for this dir (or all dirs)
rr edit                  # open the config file in $EDITOR
rr report                # show a usage report
rr help                  # show help
```

Example:

```sh
cd ~/work/api    && rr set pnpm dev
cd ~/work/worker && rr set pnpm run start:force
cd ~/work/api    && rr          # runs: pnpm dev
```

Running bare `rr` in a directory that has named commands but no default opens
a fuzzy-filter picker (type to filter, ↑/↓ to move, Enter to run, Esc to
cancel).

Behavior notes:
- Commands run through `$SHELL -c`, so pipes, `&&`, and env expansion work.
- The command's exit code is propagated.
- Lookups walk up to the nearest ancestor directory that has commands
  configured, so a config at a repo/monorepo root applies to every
  subdirectory. `rr set` always writes to the current directory.
- Commands run from the directory they were configured in.

## Config & storage

| Tool | Data | Location |
|------|------|----------|
| `ee`  | env files, MRU history, usage counts | `~/.config/envvars/` |
| `eee` | history, usage counts | `~/.config/eee/` |
| `eee` | repos it picks from | `~/github.com/<org>/<repo>` |
| `rr`  | commands, usage counts | `~/.config/rr/` |

Nothing is written outside these directories.

## Build

```sh
cargo build --release
# binaries at target/release/{ee,eee,rr}
```

## Development

```sh
make fmt    # cargo fmt
make lint   # cargo fmt --check + clippy
make test   # cargo test
make check  # lint + test + build
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for more.

## License

[MIT](LICENSE)
