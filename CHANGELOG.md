# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- MIT license.
- CI workflow (fmt, clippy, build, test) and tag-triggered release workflow
  producing Linux (gnu/musl) and macOS (x86_64/aarch64) binary archives.
- `install.sh`: downloads the right release archive, verifies its checksum,
  installs the binaries, and wires up shell integration for `ee`/`eee`.
- Landing page with install instructions and per-tool docs, deployed to
  Cloudflare Pages.
- Unit tests for the shared parsing/filtering/sorting logic in all three
  binaries (`src/lib.rs` + per-binary `#[cfg(test)]` modules).
- `CONTRIBUTING.md`, issue templates, PR template, `dependabot.yml`.

### Changed
- README rewritten to document all three tools (including `eee`, previously
  undocumented) and lead with the `curl | sh` install path.
- `Makefile`: `make install` now defaults to `~/.local/bin` instead of a
  hardcoded omarchy-specific path; added `fmt`/`lint`/`test`/`check` targets.

## [0.1.0]

Initial version of `ee`, `eee`, and `rr`.
