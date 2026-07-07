#!/bin/sh
# gitgraph-tui installer
# Usage: curl -fsSL https://raw.githubusercontent.com/bjo4/gitgraph-tui/main/install.sh | sh
#
# Environment overrides:
#   GITGRAPH_VERSION      release tag to install (default: latest, e.g. v0.1.0)
#   GITGRAPH_INSTALL_DIR  install directory (default: ~/.local/bin)
set -eu

REPO="bjo4/gitgraph-tui"
INSTALL_DIR="${GITGRAPH_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${GITGRAPH_VERSION:-}"

say() { printf '%s\n' "$*"; }
err() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}
have() { command -v "$1" >/dev/null 2>&1; }

fetch() {
  # fetch <url> <dest>; non-zero on failure
  if have curl; then
    curl -fsL --proto '=https' --tlsv1.2 -o "$2" "$1"
  elif have wget; then
    wget -q -O "$2" "$1"
  else
    err "this installer needs curl or wget"
  fi
}

fallback_cargo() {
  say "falling back to building from source with cargo."
  if have cargo; then
    say "running: cargo install --git https://github.com/$REPO --locked"
    say "(this compiles the project and takes a few minutes)"
    cargo install --git "https://github.com/$REPO" --locked
    say "done. installed to \$CARGO_HOME/bin (usually ~/.cargo/bin)."
    exit 0
  fi
  err "cargo not found. Install Rust from https://rustup.rs and re-run, or download a release manually: https://github.com/$REPO/releases"
}

# --- detect platform ---
os=$(uname -s)
arch=$(uname -m)
case "$os" in
  Linux) os_part="unknown-linux-musl" ;;
  Darwin) os_part="apple-darwin" ;;
  *) os_part="" ;;
esac
case "$arch" in
  x86_64 | amd64) arch_part="x86_64" ;;
  aarch64 | arm64) arch_part="aarch64" ;;
  *) arch_part="" ;;
esac
if [ -z "$os_part" ] || [ -z "$arch_part" ]; then
  say "unsupported platform for prebuilt binaries: $os/$arch"
  fallback_cargo
fi
target="${arch_part}-${os_part}"

tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT

# --- resolve version ---
if [ -z "$VERSION" ]; then
  if ! fetch "https://api.github.com/repos/$REPO/releases/latest" "$tmpdir/latest.json"; then
    err "cannot query the latest release from the GitHub API. Set GITGRAPH_VERSION=vX.Y.Z and retry."
  fi
  VERSION=$(sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$tmpdir/latest.json" | head -n 1)
  [ -n "$VERSION" ] || err "could not determine the latest version. Set GITGRAPH_VERSION=vX.Y.Z and retry."
fi

say "gitgraph-tui installer"
say "  platform: $target"
say "  version:  $VERSION"

asset="gitgraph-tui-${VERSION}-${target}.tar.gz"
base="https://github.com/$REPO/releases/download/$VERSION"

say "  downloading $asset ..."
if ! fetch "$base/$asset" "$tmpdir/$asset"; then
  say "download failed — no prebuilt asset for $target in $VERSION?"
  fallback_cargo
fi
fetch "$base/$asset.sha256" "$tmpdir/$asset.sha256" || err "checksum file is missing from the release"

say "  verifying sha256 ..."
(
  cd "$tmpdir"
  if have sha256sum; then
    sha256sum -c "$asset.sha256" >/dev/null 2>&1
  elif have shasum; then
    shasum -a 256 -c "$asset.sha256" >/dev/null 2>&1
  else
    say "warning: sha256sum/shasum not found — skipping verification" >&2
  fi
) || err "checksum mismatch — aborting (corrupted or tampered download)"

tar -xzf "$tmpdir/$asset" -C "$tmpdir"
[ -f "$tmpdir/gitgraph-tui" ] || err "archive did not contain the gitgraph-tui binary"
mkdir -p "$INSTALL_DIR"
install -m 755 "$tmpdir/gitgraph-tui" "$INSTALL_DIR/gitgraph-tui"
say "  installed: $INSTALL_DIR/gitgraph-tui"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    say ""
    say "note: $INSTALL_DIR is not in your PATH. Add this to your shell profile:"
    say "  export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac
say ""
say "Run 'gitgraph-tui' inside any git repository. Tip: alias gg=gitgraph-tui"
