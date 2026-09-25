#!/usr/bin/env bash
set -euo pipefail

# Install ccbox and wire it into Claude Code's statusLine and hooks: the
# prebuilt binary from the latest release, SHA-256 verified, or a cargo build
# when there is none for this platform or CCBOX_BUILD_FROM_SOURCE=1. Works from
# a local checkout (./install.sh) and piped from curl (curl -fsSL <url> | bash).

REPO_URL="${CCBOX_REPO_URL:-https://github.com/tom-ha/ccbox.git}"
REPO_BRANCH="${CCBOX_REPO_BRANCH:-main}"
RELEASES_URL="${CCBOX_RELEASES_URL:-https://api.github.com/repos/tom-ha/ccbox}"
RELEASES_URL="${RELEASES_URL%/}"
BIN_DIR="${CCBOX_BIN_DIR:-${CARGO_HOME:-$HOME/.cargo}/bin}"

err() { printf 'error: %s\n' "$*" >&2; }

# Resolve a local source directory only when BASH_SOURCE[0] points at a real
# file on disk next to a Cargo.toml. Under `curl | bash` BASH_SOURCE[0] is
# empty, so this guard skips straight to the git branch instead of accidentally
# picking up an unrelated Cargo.toml in the current working directory.
LOCAL_SOURCE=""
if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
  CANDIDATE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" 2>/dev/null && pwd || true)"
  if [[ -n "$CANDIDATE" && -f "$CANDIDATE/Cargo.toml" ]]; then
    LOCAL_SOURCE="$CANDIDATE"
  fi
fi

host_triple() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  if [[ "$os" == Darwin && "$arch" == x86_64 && "$(sysctl -n sysctl.proc_translated 2>/dev/null)" == 1 ]]; then
    arch=arm64
  fi
  case "$os/$arch" in
    Darwin/arm64 | Darwin/aarch64) echo aarch64-apple-darwin ;;
    Darwin/x86_64) echo x86_64-apple-darwin ;;
    Linux/x86_64 | Linux/amd64) echo x86_64-unknown-linux-musl ;;
    Linux/aarch64 | Linux/arm64) echo aarch64-unknown-linux-musl ;;
  esac
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

json_urls() {
  grep -o '"browser_download_url"[[:space:]]*:[[:space:]]*"[^"]*"' | sed 's/.*"\([^"]*\)"$/\1/'
}

# Sets TAR_URL/SUMS_URL for triple $1 from the latest release; returns 1 if it has no such tarball.
find_prebuilt() {
  local triple="$1" json tag name
  json="$(curl -fsSL -H 'Accept: application/vnd.github+json' "$RELEASES_URL/releases/latest" 2>/dev/null)" || return 1
  tag="$(printf '%s' "$json" | grep -o '"tag_name"[[:space:]]*:[[:space:]]*"[^"]*"' | head -n1 | sed 's/.*"\([^"]*\)"$/\1/')"
  [[ -n "$tag" ]] || return 1
  name="ccbox-${tag#v}-$triple.tar.gz"
  TAR_URL="$(printf '%s' "$json" | json_urls | grep "/$name\$" | head -n1 || true)"
  SUMS_URL="$(printf '%s' "$json" | json_urls | grep '/SHA256SUMS$' | head -n1 || true)"
  TAR_NAME="$name"
  RELEASE_TAG="$tag"
  [[ -n "$TAR_URL" && -n "$SUMS_URL" ]]
}

install_prebuilt() {
  local tmp expected actual
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' RETURN
  echo "==> downloading ccbox $RELEASE_TAG ($TAR_NAME)"
  curl -fsSL -o "$tmp/$TAR_NAME" "$TAR_URL" || { err "download failed: $TAR_URL"; return 1; }
  curl -fsSL -o "$tmp/SHA256SUMS" "$SUMS_URL" || { err "download failed: $SUMS_URL"; return 1; }
  expected="$(awk -v n="$TAR_NAME" '$2 == n || $2 == "*" n { print $1 }' "$tmp/SHA256SUMS" | head -n1)"
  actual="$(sha256_of "$tmp/$TAR_NAME")"
  if [[ -z "$expected" ]]; then
    err "SHA256SUMS has no line for $TAR_NAME; not installing"
    return 1
  fi
  if [[ "$expected" != "$actual" ]]; then
    err "checksum mismatch for $TAR_NAME: expected $expected, got $actual; not installing"
    return 1
  fi
  echo "==> checksum verified ($actual)"
  tar -xzf "$tmp/$TAR_NAME" -C "$tmp" ccbox || { err "$TAR_NAME has no ccbox binary"; return 1; }
  [[ -f "$tmp/ccbox" ]] || { err "$TAR_NAME has no ccbox binary"; return 1; }
  local staged="$BIN_DIR/.ccbox-install.$$"
  if ! { mkdir -p "$BIN_DIR" && cp "$tmp/ccbox" "$staged" && chmod 0755 "$staged" && mv -f "$staged" "$BIN_DIR/ccbox"; }; then
    rm -f "$staged"
    err "could not write $BIN_DIR/ccbox; set CCBOX_BIN_DIR to a directory you can write to"
    return 1
  fi
  CCBOX_BIN="$BIN_DIR/ccbox"
  if [[ "$("$CCBOX_BIN" version 2>/dev/null)" != "ccbox ${RELEASE_TAG#v}" ]]; then
    err "$CCBOX_BIN does not report ccbox ${RELEASE_TAG#v} after installing"
    return 1
  fi
}

install_from_source() {
  if ! command -v cargo >/dev/null 2>&1; then
    err "cargo not found in PATH, and it is needed to build ccbox from source."
    err "Install Rust via rustup:"
    err "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
  fi
  local root="${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}"
  echo "==> building ccbox from source (this takes a minute)"
  if [[ -n "$LOCAL_SOURCE" ]]; then
    echo "==> source: $LOCAL_SOURCE"
    cargo install --path "$LOCAL_SOURCE" --locked --root "$root"
  else
    echo "==> source: $REPO_URL@$REPO_BRANCH"
    cargo install --git "$REPO_URL" --branch "$REPO_BRANCH" --locked --root "$root"
  fi
  CCBOX_BIN="$root/bin/ccbox"
}

CCBOX_BIN=""
TRIPLE="$(host_triple)"
case "$(printf '%s' "${CCBOX_BUILD_FROM_SOURCE:-}" | tr '[:upper:]' '[:lower:]')" in
  1 | true | yes | on)
    echo "==> CCBOX_BUILD_FROM_SOURCE is set"
    install_from_source
    ;;
  *)
    if [[ -z "$TRIPLE" ]]; then
      echo "==> no prebuilt ccbox for $(uname -s)/$(uname -m); building from source instead"
      install_from_source
    elif find_prebuilt "$TRIPLE"; then
      install_prebuilt || exit 1
    else
      echo "==> no prebuilt ccbox for $TRIPLE in the latest release at $RELEASES_URL; building from source instead"
      install_from_source
    fi
    ;;
esac

if [[ ! -x "$CCBOX_BIN" ]]; then
  err "could not find the installed ccbox binary at $CCBOX_BIN"
  exit 1
fi
echo "==> installed: $CCBOX_BIN ($("$CCBOX_BIN" --version))"

"$CCBOX_BIN" setup

case ":$PATH:" in
  *":$(dirname -- "$CCBOX_BIN"):"*) ;;
  *) echo "==> note: $(dirname -- "$CCBOX_BIN") is not on your PATH; add it to run 'ccbox update' by name" ;;
esac
echo "Done. Restart Claude Code to pick up the new statusline."
