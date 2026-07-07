# Project Plan: Professionalize the `ee` / `eee` / `rr` tools

This document is an implementation plan to hand to Sonnet. It describes **what
to build and why**, with enough specificity that each task can be picked up
independently. Nothing here is implemented yet.

## Context (read this first)

The repo is a single Rust crate that produces **three separate binaries** from
`src/`:

| Binary | Source | What it does | How it's invoked |
|--------|--------|--------------|------------------|
| `ee`  | `src/main.rs` | TUI picker over `~/.config/envvars/*`; prints `source <path>` to stdout | `eval $(ee)` |
| `eee` | `src/eee.rs`  | TUI picker over repos at `~/github.com/<org>/<repo>`; prints `cd <path>` to stdout; has usage report + sort modes | `eval "$(eee)"` (needs shell wrapper) |
| `rr`  | `src/rr.rs`   | Per-directory command shortcuts (set/run/list/rm/edit/report); runs commands via `$SHELL -c` | `rr`, `rr <name>`, `rr set …` |

Critical implementation detail that affects install script + docs: **`ee` and
`eee` do not change the shell themselves** — they print a command to stdout that
the *calling shell* must `eval`. So installation isn't just "put a binary on
PATH"; the user also needs shell integration (an alias/function). `rr` runs
directly and needs no wrapper.

Current state: `crossterm` is the only dependency. `Makefile` builds all three
and `make install` copies them to a hardcoded `~/.local/share/omarchy/bin`
(omarchy-specific — not portable). There is **no LICENSE, no `.github/`, no CI,
no releases, and no website.** `eee` is undocumented in the README.

Design constraints to preserve throughout:
- Keep it a single crate / three thin binaries. Don't over-engineer.
- Keep the release profile as-is (`opt-level="z"`, `lto`, `strip`) — small binaries are a feature.
- Binaries must stay dependency-light and cross-platform where the TUI allows
  (crossterm supports Linux/macOS/Windows; the shell-eval integration is
  POSIX-shell + fish; Windows is best-effort / out of scope for v1).

---

## Deliverable 1 — Release engineering (GitHub Actions → binaries)

Goal: tagging a version produces downloadable, ready-to-run binaries for the
common desktop targets, and every push/PR is validated by CI.

### 1a. CI workflow — `.github/workflows/ci.yml`
Triggers: `push` to `main` and `pull_request`.
Jobs (matrix over stable Rust; consider also MSRV once one is chosen):
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release`
- `cargo test` (see Deliverable 5 — tests need to exist first; until then this is a no-op but wire it up)
Cache `~/.cargo` and `target/` (e.g. `Swatinem/rust-cache`). Keep it fast.

### 1b. Release workflow — `.github/workflows/release.yml`
Trigger: push of a tag matching `v*` (and optionally `workflow_dispatch`).
Build a matrix and upload artifacts to a GitHub Release. Target triples:
- `x86_64-unknown-linux-gnu`
- `x86_64-unknown-linux-musl` (static — best for the install script)
- `aarch64-unknown-linux-gnu` (and/or musl)
- `x86_64-apple-darwin`
- `aarch64-apple-darwin` (Apple Silicon)

Steps per target:
1. Install the Rust target (use `dtolnay/rust-toolchain`); for musl/cross use `cross` or the musl toolchain.
2. `cargo build --release --target <triple>`.
3. **Bundle all three binaries together** into one archive per target
   (`ee`, `eee`, `rr`) — e.g. `ee-tools-<version>-<triple>.tar.gz`. They're a
   suite; users want all three.
4. Generate a `SHA256SUMS` file per archive (the install script will verify it).
5. Upload archives + checksums to the Release (`softprops/action-gh-release`
   or `gh release upload`).

Decisions to surface to the user before/while implementing:
- **Versioning**: `Cargo.toml` is at `0.1.0`. Confirm the tag scheme (`vX.Y.Z`)
  and whether the workflow should assert the tag matches the Cargo version.
- **Naming**: confirm archive naming convention (used verbatim by the install script).

### 1c. Optionally: `release-plz` or `cargo-dist`
`cargo-dist` can generate the entire release workflow **and** a matching
`install.sh` from `Cargo.toml`, which would satisfy much of Deliverable 2 for
free. Evaluate it first; if adopted, Deliverable 2 becomes "customize the
generated installer to add shell integration." If not adopted, hand-roll as
below. Recommend evaluating `cargo-dist` before hand-rolling.

---

## Deliverable 2 — Install script

Goal: `curl -fsSL https://<site>/install.sh | sh` installs all three binaries
**and** sets up shell integration for `ee`/`eee`.

### 2a. `install.sh` (POSIX sh)
Behavior:
1. Detect OS + arch → map to a release target triple.
2. Resolve the latest release (GitHub API `releases/latest`) or honor a pinned
   `EE_VERSION` env var.
3. Download the matching `.tar.gz` + `SHA256SUMS`; **verify the checksum**; abort on mismatch.
4. Install binaries to a sensible, non-hardcoded dir:
   - default `~/.local/bin` (XDG-friendly), overridable via `EE_INSTALL_DIR`/`--bin-dir`.
   - Warn if the dir isn't on `PATH` and print the line to add.
5. **Shell integration** (this is the part `make install` currently omits):
   - `ee` needs an alias/function; `eee` needs a function because `eval "$(eee)"`
     must run in the current shell. Provide a snippet and either append it to the
     user's rc file (with confirmation / `--no-shell` opt-out) or print it for the
     user to paste. Support bash, zsh, and fish (fish syntax differs).
   - Suggested integration:
     ```sh
     ee()  { eval "$(command ee-bin "$@")"; }   # or keep `ee` as the binary + `alias se='eval $(ee)'`
     eee() { eval "$(command eee-bin "$@")"; }
     ```
     Decide whether the binary itself installs as `ee`/`eee` and the shell
     function wraps a differently-named binary, or the function shadows the
     binary via `command`. **Recommendation:** ship binaries named `ee`/`eee`,
     have the shell function call them via `command ee` to avoid recursion. Pin
     the exact scheme and keep it identical across install.sh, README, and website.
6. Idempotent: re-running upgrades in place and doesn't duplicate rc-file lines
   (guard with a marker comment block).
7. Clear success output: what was installed, where, and "restart your shell / `source` your rc".

### 2b. Uninstall
Provide `install.sh --uninstall` (or a short note in docs) that removes the
binaries and the marked rc-file block.

### 2c. Rework the `Makefile`
- Drop the hardcoded omarchy path; default `INSTALL_DIR` to `~/.local/bin`,
  overridable. Keep `make install` working for from-source users.
- Add `make fmt`, `make lint`, `make test` mirroring CI so contributors can run
  the same checks locally.

---

## Deliverable 3 — Website / landing page

Goal: a single, self-contained static landing page documenting **what / why /
how**, plus copy-paste install. Hosted on **GitHub Pages**.

### 3a. Tech choice
Recommend a **single static `index.html`** (hand-written HTML + inline CSS, no
build step) living in `docs/` or a `site/` dir, published via GitHub Pages. It's
a 3-tool CLI suite — a full SSG (Astro/Zola/Docusaurus) is overkill. Keep it
dependency-free so it's trivial to maintain and deploys with a Pages workflow.
(If the user prefers a themed generator later, Zola or Astro are the fallbacks.)

### 3b. Page content / sections
1. **Hero**: name, one-line pitch ("Three tiny terminal tools for env files,
   repo hopping, and per-directory commands"), the `curl … | sh` install line,
   link to GitHub.
2. **Why** (the motivation for each tool — the problem it solves):
   - `ee` — stop hand-`source`-ing env files; fuzzy-pick and load them.
   - `eee` — jump to any cloned repo under `~/github.com` without `cd` gymnastics.
   - `rr` — remember the right command per project (`pnpm dev`, etc.) and run it with a keystroke.
3. **What** — a short per-tool feature list + the keybindings table (reuse
   README's `ee` table; add equivalents for `eee` and `rr`'s picker).
4. **How** — install (script + `cargo install`/from-source + shell integration
   snippet), then a "Getting started" per tool with real examples (mirror the
   README's `rr` examples). **Must document the shell-eval requirement for
   `ee`/`eee` prominently** — it's the #1 gotcha.
5. **Config & storage** — where each tool keeps its files
   (`~/.config/envvars`, `~/.config/eee`, `~/.config/rr`, `~/github.com`), so
   users know what's on disk.
6. Footer: license, repo link, version.

### 3c. Publishing workflow — `.github/workflows/pages.yml`
Deploy `docs/` (or the built site) to GitHub Pages on push to `main`. If it's
plain HTML, this is just `upload-pages-artifact` + `deploy-pages`. Note for the
user: **GitHub Pages must be enabled in repo settings** (Settings → Pages →
source = GitHub Actions) — the workflow can't do that itself; call this out.

### 3d. Design note
Keep it clean and terminal-flavored (monospace accents, dark theme, asciinema
or a static "screenshot" of each TUI). Optional but high-impact: record short
`asciinema`/`vhs` (charmbracelet/vhs) demos of each tool and embed them —
`vhs` produces GIFs from a script and is reproducible in CI.

---

## Deliverable 4 — Repo professionalism / hygiene

Small items that collectively make the repo look maintained:
1. **LICENSE** — none exists. Ask the user which (MIT vs. dual MIT/Apache-2.0 is
   the Rust-ecosystem norm). Add `license = "…"` to `Cargo.toml` and the file(s).
2. **README overhaul**:
   - Add a top-level suite intro covering all three tools (currently `eee` is missing entirely).
   - Add the `curl | sh` install as the primary path; keep from-source as secondary.
   - Badges: CI status, latest release, license.
   - Link to the website.
3. **`Cargo.toml` metadata**: `description`, `repository`, `homepage`,
   `license`, `keywords`, `categories`, `authors`, `readme`. Needed for a
   possible `cargo install` / crates.io path.
4. **`cargo install` support**: document `cargo install --git <repo>` as an
   install option (works today; just needs mentioning). Decide whether to
   publish to crates.io (naming: `ee` may be taken — check; may need a suite
   crate name).
5. **Community files**: `CONTRIBUTING.md` (how to build/test/run the checks),
   `.github/ISSUE_TEMPLATE/` (bug + feature), `.github/pull_request_template.md`,
   a `CHANGELOG.md` (Keep a Changelog format), and `rustfmt.toml` /
   `clippy` config if any non-defaults are wanted.
6. **`dependabot.yml`** for `cargo` and `github-actions` update PRs.

---

## Deliverable 5 — Tests (prerequisite for meaningful CI)

There are currently no tests. The TUI event loops are hard to test, but the
**pure logic is very testable** and should be extracted/covered so CI's
`cargo test` is real:
- `rr`: config parsing/serialization (`path⇥name⇥command`), ancestor-directory
  lookup (walk-up resolution), count tracking, arg passthrough.
- `ee`/`eee`: filtering/fuzzy-match ranking, history/MRU ordering, counts, the
  3-day usage-log windowing in `eee`.
- Refactor these into functions (or a small `lib.rs` / shared module) so they're
  unit-testable without a terminal. This also de-duplicates logic currently
  copied across the three `main`s (history/counts loading looks near-identical).

Scope note: this is the largest optional item. If the user wants to ship the
website + releases fast, mark tests as a fast-follow and let CI run
fmt/clippy/build now, adding `cargo test` coverage incrementally.

---

## Suggested sequencing

1. **Deliverable 4** (license, Cargo metadata, README, Makefile fixes) — cheap, unblocks everything.
2. **Deliverable 1** (CI first, then release workflow) — gives you artifacts to install.
3. **Deliverable 2** (install script) — depends on release artifact naming from #1.
4. **Deliverable 3** (website) — can proceed in parallel; needs the final install command from #2.
5. **Deliverable 5** (tests) — parallel / fast-follow; upgrades CI from build-only to real.

## Open questions to confirm with the user before building

- License choice (MIT vs dual MIT/Apache-2.0)?
- Publish to crates.io, or GitHub-releases + `cargo install --git` only? (`ee` crate name may be taken.)
- Windows support in scope for v1, or Linux + macOS only?
- Adopt `cargo-dist` (auto-generates release workflow + installer) or hand-roll?
- Shell-integration install: auto-append to rc files (with opt-out) or print-and-let-user-paste?
- Website hosting confirmed as GitHub Pages, plain HTML? (vs. a generator.)
- Record `vhs`/asciinema demos for the site, or static screenshots for v1?
