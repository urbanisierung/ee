#!/bin/sh
# Install ee, eee, and rr — https://github.com/urbanisierung/ee
#
#   curl -fsSL https://<site>/install.sh | sh
#
# Downloads a release archive from GitHub, verifies its checksum, installs
# the three binaries, and (unless --no-shell) wires up shell functions so
# `ee` and `eee` work without a manual `eval` (see README for why that's
# needed). Linux and macOS only.
#
# Env vars:
#   EE_VERSION      pin a release tag, e.g. v0.2.0 (default: latest)
#   EE_INSTALL_DIR  install directory (default: ~/.local/bin)
#
# Flags:
#   --no-shell       skip rc-file editing; print the integration snippet instead
#   --bin-dir DIR    same as EE_INSTALL_DIR
#   --uninstall      remove the binaries and the shell-integration block
#   -h, --help       show this help

set -eu

REPO="urbanisierung/ee"
BINARIES="ee eee rr"
MARKER_BEGIN="# >>> ee-tools (ee/eee) shell integration >>>"
MARKER_END="# <<< ee-tools (ee/eee) shell integration <<<"

INSTALL_DIR="${EE_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${EE_VERSION:-}"
NO_SHELL=0
UNINSTALL=0

log() { printf '%s\n' "$*"; }
err() { printf 'install.sh: %s\n' "$*" >&2; }
die() {
  err "$*"
  exit 1
}

usage() {
  cat <<'EOF'
Usage: install.sh [--no-shell] [--bin-dir DIR] [--uninstall]

Installs the ee/eee/rr binaries and wires up shell integration so `ee`
and `eee` work without a manual `eval`.

Env vars:
  EE_VERSION      pin a release tag (e.g. v0.2.0); default: latest
  EE_INSTALL_DIR  install directory; default: ~/.local/bin
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --no-shell)
      NO_SHELL=1
      ;;
    --bin-dir)
      shift
      [ $# -gt 0 ] || die "--bin-dir requires an argument"
      INSTALL_DIR="$1"
      ;;
    --uninstall)
      UNINSTALL=1
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1 (see --help)"
      ;;
  esac
  shift
done

need() {
  command -v "$1" >/dev/null 2>&1 || die "'$1' is required but not found"
}

fetch() {
  # fetch <url> <outfile>
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$1" -o "$2"
  elif command -v wget >/dev/null 2>&1; then
    wget -q "$1" -O "$2"
  else
    die "curl or wget is required"
  fi
}

fetch_stdout() {
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O - "$1"
  else
    die "curl or wget is required"
  fi
}

detect_target() {
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Linux)
      case "$arch" in
        x86_64 | amd64) echo "x86_64-unknown-linux-musl" ;;
        aarch64 | arm64) echo "aarch64-unknown-linux-gnu" ;;
        *) die "unsupported Linux architecture: $arch" ;;
      esac
      ;;
    Darwin)
      case "$arch" in
        x86_64) echo "x86_64-apple-darwin" ;;
        arm64) echo "aarch64-apple-darwin" ;;
        *) die "unsupported macOS architecture: $arch" ;;
      esac
      ;;
    *)
      die "unsupported OS: $os (only Linux and macOS are supported; on Windows, use WSL)"
      ;;
  esac
}

resolve_version() {
  if [ -n "$VERSION" ]; then
    case "$VERSION" in
      v*) echo "$VERSION" ;;
      *) echo "v$VERSION" ;;
    esac
    return
  fi
  tag=$(fetch_stdout "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep -m1 '"tag_name"' \
    | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')
  [ -n "$tag" ] || die "could not resolve the latest release; set EE_VERSION to pin one"
  echo "$tag"
}

sha256_check() {
  # sha256_check <file> <sumfile>
  file="$1"
  sumfile="$2"
  dir=$(dirname "$file")
  sum_base=$(basename "$sumfile")
  if command -v sha256sum >/dev/null 2>&1; then
    (cd "$dir" && sha256sum -c "$sum_base") >/dev/null \
      || die "checksum verification failed for $(basename "$file")"
  elif command -v shasum >/dev/null 2>&1; then
    (cd "$dir" && shasum -a 256 -c "$sum_base") >/dev/null \
      || die "checksum verification failed for $(basename "$file")"
  else
    die "no sha256sum/shasum found — cannot verify download integrity"
  fi
}

print_snippet() {
  cat <<'EOF'

  # bash/zsh — add to ~/.bashrc or ~/.zshrc:
  ee()  { eval "$(command ee  "$@")"; }
  eee() { eval "$(command eee "$@")"; }

  # fish — add to ~/.config/fish/config.fish:
  function ee;  eval (command ee  $argv); end
  function eee; eval (command eee $argv); end

EOF
}

# rc files to touch when *installing* integration: any that already exist,
# or (on a fresh machine with none of them yet) the one matching $SHELL.
rc_files_for_install() {
  files=""
  if [ -f "$HOME/.bashrc" ]; then files="$files $HOME/.bashrc"; fi
  if [ -f "$HOME/.zshrc" ]; then files="$files $HOME/.zshrc"; fi
  if [ -f "$HOME/.config/fish/config.fish" ]; then files="$files $HOME/.config/fish/config.fish"; fi

  if [ -z "$files" ]; then
    shell_name=$(basename "${SHELL:-sh}")
    case "$shell_name" in
      zsh) files="$HOME/.zshrc" ;;
      fish) files="$HOME/.config/fish/config.fish" ;;
      *) files="$HOME/.bashrc" ;;
    esac
  fi
  echo "$files"
}

# rc files to touch when *uninstalling*: only ones that actually exist.
rc_files_existing() {
  files=""
  for f in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.config/fish/config.fish"; do
    if [ -f "$f" ]; then files="$files $f"; fi
  done
  echo "$files"
}

append_block_posix() {
  file="$1"
  if grep -qF "$MARKER_BEGIN" "$file" 2>/dev/null; then
    log "  already configured: $file"
    return
  fi
  mkdir -p "$(dirname "$file")"
  {
    printf '\n%s\n' "$MARKER_BEGIN"
    printf 'ee()  { eval "$(command ee  "$@")"; }\n'
    printf 'eee() { eval "$(command eee "$@")"; }\n'
    printf '%s\n' "$MARKER_END"
  } >>"$file"
  log "  updated: $file"
}

append_block_fish() {
  file="$1"
  if grep -qF "$MARKER_BEGIN" "$file" 2>/dev/null; then
    log "  already configured: $file"
    return
  fi
  mkdir -p "$(dirname "$file")"
  {
    printf '\n%s\n' "$MARKER_BEGIN"
    printf 'function ee;  eval (command ee  $argv); end\n'
    printf 'function eee; eval (command eee $argv); end\n'
    printf '%s\n' "$MARKER_END"
  } >>"$file"
  log "  updated: $file"
}

remove_block() {
  file="$1"
  if ! grep -qF "$MARKER_BEGIN" "$file" 2>/dev/null; then
    return
  fi
  tmp=$(mktemp)
  awk -v begin="$MARKER_BEGIN" -v end="$MARKER_END" '
    $0 == begin { skip=1 }
    skip != 1 { print }
    $0 == end { skip=0 }
  ' "$file" >"$tmp"
  mv "$tmp" "$file"
  log "  removed integration block from: $file"
}

setup_shell_integration() {
  log ""
  log "Setting up shell integration for ee/eee..."
  for f in $(rc_files_for_install); do
    case "$f" in
      */config.fish) append_block_fish "$f" ;;
      *) append_block_posix "$f" ;;
    esac
  done
}

do_install() {
  need tar
  need uname

  target=$(detect_target)
  version=$(resolve_version)
  version_num="${version#v}"
  name="ee-tools-${version_num}-${target}"
  archive="${name}.tar.gz"
  sumfile="${archive}.sha256"
  base_url="https://github.com/${REPO}/releases/download/${version}"

  tmpdir=$(mktemp -d)
  trap 'rm -rf "$tmpdir"' EXIT

  log "Downloading ${archive} (${version})..."
  fetch "${base_url}/${archive}" "${tmpdir}/${archive}"
  fetch "${base_url}/${sumfile}" "${tmpdir}/${sumfile}"
  sha256_check "${tmpdir}/${archive}" "${tmpdir}/${sumfile}"

  log "Extracting..."
  tar -xzf "${tmpdir}/${archive}" -C "$tmpdir"

  mkdir -p "$INSTALL_DIR"
  for bin in $BINARIES; do
    cp "${tmpdir}/${name}/${bin}" "${INSTALL_DIR}/${bin}"
    chmod +x "${INSTALL_DIR}/${bin}"
  done

  log "Installed ee, eee, rr to ${INSTALL_DIR}"

  case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
      log ""
      err "warning: ${INSTALL_DIR} is not on your PATH"
      log "  add this to your shell rc file:"
      log "    export PATH=\"${INSTALL_DIR}:\$PATH\""
      ;;
  esac

  if [ "$NO_SHELL" -eq 1 ]; then
    log ""
    log "Shell integration skipped (--no-shell). Add this to your shell rc file"
    log "so 'ee'/'eee' work without a manual eval:"
    print_snippet
  else
    setup_shell_integration
  fi

  log ""
  log "Done. Restart your shell (or run: exec \$SHELL) and try: ee / eee / rr"
}

do_uninstall() {
  for bin in $BINARIES; do
    if [ -f "${INSTALL_DIR}/${bin}" ]; then
      rm -f "${INSTALL_DIR}/${bin}"
      log "removed ${INSTALL_DIR}/${bin}"
    fi
  done

  for f in $(rc_files_existing); do
    remove_block "$f"
  done

  log "Uninstalled."
}

main() {
  if [ "$UNINSTALL" -eq 1 ]; then
    do_uninstall
    exit 0
  fi
  do_install
}

main
