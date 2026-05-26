#!/usr/bin/env bash
set -euo pipefail

# Build & install ccbox, then wire it into Claude Code's statusLine.
# Requires: cargo (https://rustup.rs) and python3.

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

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

echo "==> cargo install --path $SCRIPT_DIR"
cargo install --path "$SCRIPT_DIR" --locked

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
