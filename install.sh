#!/usr/bin/env bash
set -euo pipefail

# Build & install ccbox, then wire it into Claude Code's statusLine.
# Requires: cargo (https://rustup.rs) and python3.
#
# Works two ways:
#   - From a local clone:  ./install.sh         (installs from $SCRIPT_DIR)
#   - Piped from curl:     curl ... | bash      (installs from the git repo)

CCBOX_GIT_URL="${CCBOX_GIT_URL:-https://github.com/tom-ha/ccbox.git}"

# When piped via `curl | bash`, BASH_SOURCE[0] is unset/empty. Fall back to
# an empty SCRIPT_DIR in that case so the local-checkout detection below fails
# cleanly and we install from git instead.
if [[ -n "${BASH_SOURCE[0]:-}" && -f "${BASH_SOURCE[0]}" ]]; then
  SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
else
  SCRIPT_DIR=""
fi

err() { printf 'error: %s\n' "$*" >&2; }

if ! command -v cargo >/dev/null 2>&1; then
  err "cargo not found in PATH."
  err "Install Rust via rustup:"
  err "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  err "python3 not found in PATH (used to patch settings.json)."
  exit 1
fi

if [[ -n "$SCRIPT_DIR" && -f "$SCRIPT_DIR/Cargo.toml" ]] \
  && grep -q '^name\s*=\s*"ccbox"' "$SCRIPT_DIR/Cargo.toml"; then
  echo "==> cargo install --path $SCRIPT_DIR"
  cargo install --path "$SCRIPT_DIR" --locked
else
  echo "==> cargo install --git $CCBOX_GIT_URL"
  cargo install --git "$CCBOX_GIT_URL" --locked ccbox
fi

CCBOX_BIN="$(command -v ccbox || true)"
if [[ -z "$CCBOX_BIN" ]]; then
  CARGO_BIN="${CARGO_HOME:-$HOME/.cargo}/bin"
  if [[ -x "$CARGO_BIN/ccbox" ]]; then
    CCBOX_BIN="$CARGO_BIN/ccbox"
  else
    err "could not locate the installed ccbox binary; check 'cargo install' output above."
    exit 1
  fi
fi
echo "==> installed: $CCBOX_BIN"

CLAUDE_DIR="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
SETTINGS="$CLAUDE_DIR/settings.json"
mkdir -p "$CLAUDE_DIR"

if [[ -f "$SETTINGS" ]]; then
  BACKUP="$SETTINGS.bak.$(date +%Y%m%d-%H%M%S)"
  cp "$SETTINGS" "$BACKUP"
  echo "==> backed up existing settings to $BACKUP"
else
  echo "{}" > "$SETTINGS"
fi

python3 - "$SETTINGS" "$CCBOX_BIN" <<'PY'
import json, sys, pathlib
path = pathlib.Path(sys.argv[1])
ccbox = sys.argv[2]
data = json.loads(path.read_text() or "{}")
prev = data.get("statusLine")
if prev is not None:
    print(f"==> previous statusLine: {json.dumps(prev)}")
data["statusLine"] = {"type": "command", "command": ccbox}
tmp = path.with_suffix(path.suffix + ".tmp")
tmp.write_text(json.dumps(data, indent=2) + "\n")
tmp.replace(path)
PY

echo "==> wrote statusLine.command = $CCBOX_BIN to $SETTINGS"
echo "Done. Restart Claude Code to pick up the new statusline."
