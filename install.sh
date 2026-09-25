#!/usr/bin/env bash
set -euo pipefail

# Build & install ccbox, then wire it into Claude Code's statusLine.
# Requires: cargo (https://rustup.rs) and python3.
#
# Works both when run from a local ccbox checkout (./install.sh) and when
# piped from curl (curl -fsSL <raw-url> | bash). In the local case it
# installs from the working tree; in the curl case it installs from the
# canonical git repo (override with CCBOX_REPO_URL / CCBOX_REPO_BRANCH).

REPO_URL="${CCBOX_REPO_URL:-https://github.com/tom-ha/ccbox.git}"
REPO_BRANCH="${CCBOX_REPO_BRANCH:-main}"

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

if [[ -n "$LOCAL_SOURCE" ]]; then
  echo "==> installing ccbox from local source ($LOCAL_SOURCE)"
  echo "==> building ccbox from source (this takes a minute)"
  cargo install --path "$LOCAL_SOURCE" --locked
else
  echo "==> installing ccbox from $REPO_URL@$REPO_BRANCH"
  echo "==> building ccbox from source (this takes a minute)"
  cargo install --git "$REPO_URL" --branch "$REPO_BRANCH" --locked
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
import json, shlex, sys, pathlib
path = pathlib.Path(sys.argv[1])
ccbox = sys.argv[2]
data = json.loads(path.read_text() or "{}")
prev = data.get("statusLine")
if prev is not None:
    print(f"==> previous statusLine: {json.dumps(prev)}")
data["statusLine"] = {"type": "command", "command": ccbox, "refreshInterval": 5}

# Hooks feed the "needs you" row. Replace any earlier ccbox hook groups so
# re-running the installer stays idempotent; other hooks are left alone.
hook_cmd = f"{shlex.quote(ccbox)} hook"
def is_ccbox_hook(h):
    return isinstance(h, dict) and str(h.get("command", "")).endswith("ccbox hook")
wanted = {
    "Notification": [""],
    "PreToolUse": ["AskUserQuestion"],
    "PostToolUse": [""],
    "UserPromptSubmit": [""],
    "Stop": [""],
    "SessionEnd": [""],
}
hooks = data.setdefault("hooks", {})
for event, matchers in wanted.items():
    groups = [
        g for g in hooks.get(event, [])
        if not (isinstance(g, dict) and g.get("hooks") and all(map(is_ccbox_hook, g["hooks"])))
    ]
    for m in matchers:
        groups.append({"matcher": m, "hooks": [{"type": "command", "command": hook_cmd}]})
    hooks[event] = groups
tmp = path.with_suffix(path.suffix + ".tmp")
tmp.write_text(json.dumps(data, indent=2) + "\n")
tmp.replace(path)
PY

echo "==> wrote statusLine.command = $CCBOX_BIN and ccbox hooks to $SETTINGS"
echo "Done. Restart Claude Code to pick up the new statusline."
