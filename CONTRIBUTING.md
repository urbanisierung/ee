# Contributing

Thanks for considering a contribution to `ee` / `eee` / `rr`.

## Getting set up

```sh
git clone https://github.com/urbanisierung/ee.git
cd ee
cargo build
```

## Before opening a PR

```sh
make check   # cargo fmt --check + clippy + test + build
```

Individually: `make fmt`, `make lint`, `make test`. CI runs the same checks
(`.github/workflows/ci.yml`) on every push and PR.

## Guidelines

- Keep it a single crate with three thin binaries — don't split into more
  crates or add a plugin system.
- Keep dependencies minimal; `crossterm` is the only one today.
- Keep the release profile (`opt-level = "z"`, `lto`, `strip`) — small
  binaries are a feature, not an accident.
- Pure logic (parsing, filtering, sorting) should stay unit-testable without
  a terminal — see `src/lib.rs` and the `#[cfg(test)]` modules in each
  binary for the existing pattern.
- No new abstractions for a single call site; three similar lines beat a
  premature helper.

## Releasing

Releases are tag-triggered (`.github/workflows/release.yml` on `v*` tags) and
build/upload binaries for Linux (gnu + musl, x86_64 + aarch64) and macOS
(x86_64 + aarch64). The tag's version must match `Cargo.toml`'s `version`, or
the release workflow fails fast.

To cut a release:

1. Bump `version` in `Cargo.toml`.
2. Update `CHANGELOG.md`.
3. Tag and push: `git tag vX.Y.Z && git push origin vX.Y.Z`.
