# Project Plan: Professionalize the `ee` / `eee` / `rr` tools

This document is an implementation plan to hand to Sonnet. It describes **what
to build and why**, with enough specificity that each task can be picked up
independently. Nothing here is implemented yet.

## Decisions (confirmed by the user — do not re-ask)

- **License**: MIT.
- **Distribution**: GitHub Releases only — **no crates.io**. `cargo install --git <repo>` is the from-source path.
- **Platforms**: Linux + macOS only. **Windows is out of scope for v1.**
- **Release tooling**: **hand-roll** the release workflow + install script (rationale in 1c).
- **Install script**: **auto-edits the user's rc files** by default (with a `--no-shell` opt-out and an idempotent marker block).
- **Website host**: **Cloudflare Pages** (not GitHub Pages). The user will
  prepare the CF project/page name and the required secrets (e.g.
  `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID`) — the workflow consumes them.
- **Demos**: static screenshots generated with **`vhs`** (charmbracelet/vhs).

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
the *calling shell* must `eval` (a hard OS constraint: a child process can't
change its parent shell's cwd/env). So installation isn't just "put a binary on
PATH"; the user also needs shell integration (a wrapper function). **The install
script fully automates this** by writing shadowing shell functions to the user's
rc files, so after `curl | sh` the user just types `ee`/`eee` — no manual `eval`.
See Deliverable 2a step 5 for the mechanism. `rr` runs directly and needs no wrapper.

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

### 1c. Release tooling decision: hand-roll (not cargo-dist)
`cargo-dist` was considered. **Decision: hand-roll**, because:
- The installer's core value here is **shell integration + rc-file editing**
  (see Deliverable 2), which cargo-dist's generated installer does not do — we'd
  override most of it anyway.
- Scope is small and fixed (3 bundled binaries, GitHub-releases-only, no
  Windows, no crates.io), so a hand-written matrix in `release.yml` is short and
  fully under our control over the archive layout.

Keep the hand-rolled `release.yml` from 1b and the hand-rolled `install.sh` from
Deliverable 2 as the single source of truth for naming.

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
     must run in the current shell. **Default behavior: auto-append the snippet to
     the user's detected rc file(s)** (bash → `~/.bashrc`, zsh → `~/.zshrc`, fish →
     `~/.config/fish/config.fish`). Provide a `--no-shell` flag to skip and print
     the snippet instead. Detect the active shell (`$SHELL` + which rc files
     exist) and write to all that apply. Fish syntax differs — emit fish-native
     functions there.
   - **Integration mechanism (this is what removes the `eval` gotcha entirely).**
     A child process cannot mutate its parent shell's `cwd`/env — that's an OS
     constraint, which is *why* `ee`/`eee` print a command for the shell to run.
     The fix is a shell function that **shadows** the same-named binary and
     `eval`s its output, using `command` to reach the real binary without
     recursing:
     ```sh
     ee()  { eval "$(command ee  "$@")"; }
     eee() { eval "$(command eee "$@")"; }
     ```
     Fish equivalent:
     ```fish
     function ee;  eval (command ee  $argv); end
     function eee; eval (command eee $argv); end
     ```
     **Recommendation:** ship binaries named `ee`/`eee`; the function calls
     `command ee` to avoid recursion. After the script writes these, the user
     just types `ee`/`eee` and it works — no manual `eval` ever. Pin this exact
     scheme identically across `install.sh`, README, and website.
   - **Residual gotcha only for non-script installs.** Users who take
     `cargo install --git` or copy the binary manually don't get the functions,
     so the docs must show the snippet above for them. The `curl | sh` path
     eliminates the gotcha completely.
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
how**, plus copy-paste install. Hosted on **Cloudflare Pages**.

### 3a. Tech choice
Recommend a **single static `index.html`** (hand-written HTML + inline CSS, no
build step) living in a `site/` dir. It's a 3-tool CLI suite — a full SSG
(Astro/Zola/Docusaurus) is overkill. Keep it dependency-free so it's trivial to
maintain; the `site/` dir is published directly to Cloudflare Pages (no build
command needed). Screenshots (from `vhs`, see 3d) live alongside as static
assets. (If the user prefers a themed generator later, Astro is the fallback.)

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

### 3c. Publishing workflow — `.github/workflows/pages.yml` (Cloudflare Pages)
Deploy the `site/` dir to **Cloudflare Pages** on push to `main`. Use
`cloudflare/wrangler-action` (`wrangler pages deploy site/ --project-name=<name>`)
or the Cloudflare Pages GitHub integration.
- Since `site/` is plain static HTML there is **no build step** — just deploy the dir.
- Consumes secrets **the user will provide**: `CLOUDFLARE_API_TOKEN` and
  `CLOUDFLARE_ACCOUNT_ID` (repo secrets), plus the CF **project name** the user
  creates. Reference them by name; do not hardcode. Add these to the PR
  description as required setup so the user knows what to wire up.
- Alternative (simpler, no GH Actions): connect the repo directly in the
  Cloudflare Pages dashboard with build output = `site/`. Mention this as an
  option; if the user goes that route, `pages.yml` is unnecessary.

### 3d. Design note + `vhs` screenshots
Keep it clean and terminal-flavored (monospace accents, dark theme). Generate a
**static screenshot** of each TUI (`ee`, `eee`, `rr`'s picker) using
**`vhs`** (charmbracelet/vhs): write a `.tape` script per tool that drives the
TUI and outputs a PNG (`Output demo.png`). Commit the `.tape` scripts under
`site/demos/` (or `assets/`) so the screenshots are reproducible, and embed the
generated PNGs in the page. Static images (not GIFs) per the user's preference.

---

## Deliverable 4 — Repo professionalism / hygiene

Small items that collectively make the repo look maintained:
1. **LICENSE** — none exists. Add an **MIT** `LICENSE` file and set
   `license = "MIT"` in `Cargo.toml`.
2. **README overhaul**:
   - Add a top-level suite intro covering all three tools (currently `eee` is missing entirely).
   - Add the `curl | sh` install as the primary path; keep from-source as secondary.
   - Badges: CI status, latest release, license.
   - Link to the website.
3. **`Cargo.toml` metadata**: `description`, `repository`, `homepage`,
   `license = "MIT"`, `keywords`, `categories`, `authors`, `readme`. Good hygiene
   and supports `cargo install --git`.
4. **`cargo install` support**: document `cargo install --git <repo>` as the
   from-source install option (works today; just needs mentioning). **No
   crates.io publish** — GitHub Releases + `--git` only.
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

## Remaining open questions (minor — can be decided during implementation)

All major decisions are locked in the **Decisions** section at the top. Small
items left to the implementer's judgement:

- Exact release tag/version scheme (`vX.Y.Z`) and whether `release.yml` should
  assert the tag matches the `Cargo.toml` version. (Recommend: yes.)
- Whether to include `aarch64` Linux in the release matrix or ship x86_64 Linux
  (gnu + musl) + both macOS arches only for v1.
- Author name/email for `Cargo.toml` `authors` and the MIT copyright line.

## User setup checklist (things only the user can do)

- Create the **Cloudflare Pages** project and note its **project name**.
- Add repo secrets: **`CLOUDFLARE_API_TOKEN`**, **`CLOUDFLARE_ACCOUNT_ID`**
  (unless using the CF dashboard git integration instead of `pages.yml`).
