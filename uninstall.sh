#!/usr/bin/env bash
set -euo pipefail

# Remove ccbox: unwire it from Claude Code's statusLine, delete ccbox's
# files under <claude_dir>, and uninstall the binary via cargo.
# Requires: python3 (used to patch settings.json). cargo is only required
# if the binary is still installed.
#
# Works both when run from a local checkout (./uninstall.sh) and when
# piped from curl (curl -fsSL <raw-url> | bash).

err() { printf 'error: %s\n' "$*" >&2; }

if ! command -v python3 >/dev/null 2>&1; then
  err "python3 not found in PATH (used to patch settings.json)."
  exit 1
fi

CLAUDE_DIR="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
SETTINGS="$CLAUDE_DIR/settings.json"

# Step 1: unwire statusLine from settings.json, but only if it currently
# points at a ccbox binary. Leaves a foreign statusLine untouched.
if [[ -f "$SETTINGS" ]]; then
  BACKUP="$SETTINGS.bak.$(date +%Y%m%d-%H%M%S)"
  cp "$SETTINGS" "$BACKUP"
  echo "==> backed up existing settings to $BACKUP"

  python3 - "$SETTINGS" <<'PY'
import json, shlex, sys, pathlib
path = pathlib.Path(sys.argv[1])
data = json.loads(path.read_text() or "{}")
sl = data.get("statusLine")
cmd = sl.get("command") if isinstance(sl, dict) else None
changed = False
if cmd and pathlib.PurePosixPath(cmd).name == "ccbox":
    data.pop("statusLine", None)
    changed = True
    print(f"==> removed statusLine.command ({cmd}) from {path}")
elif cmd:
    print(f"==> statusLine.command is {cmd!r}, not ccbox — leaving it alone")
else:
    print(f"==> no statusLine entry in {path} — nothing to unwire")

def is_ccbox_hook(h):
    if not isinstance(h, dict):
        return False
    try:
        toks = shlex.split(str(h.get("command", "")))
    except ValueError:
        return False
    return len(toks) >= 2 and toks[-1] == "hook" and pathlib.PurePosixPath(toks[-2]).name == "ccbox"

hooks = data.get("hooks")
if isinstance(hooks, dict):
    removed = 0
    for event in list(hooks):
        if not isinstance(hooks[event], list):
            continue
        kept_groups = []
        event_removed = 0
        for g in hooks[event]:
            inner = g.get("hooks") if isinstance(g, dict) else None
            if not isinstance(inner, list):
                kept_groups.append(g)
                continue
            kept = [h for h in inner if not is_ccbox_hook(h)]
            event_removed += len(inner) - len(kept)
            if kept or not inner:
                kept_groups.append({**g, "hooks": kept})
        removed += event_removed
        if kept_groups:
            hooks[event] = kept_groups
        elif event_removed:
            del hooks[event]
    if not hooks:
        data.pop("hooks")
    if removed:
        changed = True
        print(f"==> removed {removed} ccbox hook(s) from {path}")

if changed:
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(data, indent=2) + "\n")
    tmp.replace(path)
PY
else
  echo "==> $SETTINGS does not exist — nothing to unwire"
fi

# Step 2: remove ccbox-created files under <claude_dir>.
MARKER="$CLAUDE_DIR/ccbox-subscription"
if [[ -e "$MARKER" ]]; then
  rm -f "$MARKER"
  echo "==> removed $MARKER"
fi

CACHE_DIR="$CLAUDE_DIR/ccbox-cache"
if [[ -d "$CACHE_DIR" ]]; then
  rm -rf "$CACHE_DIR"
  echo "==> removed $CACHE_DIR"
fi

# Step 3: uninstall the binary. `cargo uninstall` is the inverse of the
# `cargo install` used by install.sh. We swallow failures so the script
# stays useful when cargo isn't on PATH or the package isn't installed.
CCBOX_BIN="$(command -v ccbox || true)"
if command -v cargo >/dev/null 2>&1; then
  if cargo uninstall ccbox 2>/dev/null; then
    echo "==> uninstalled ccbox via cargo"
  else
    echo "==> cargo uninstall ccbox reported nothing to remove"
  fi
else
  echo "==> cargo not found in PATH — skipping binary uninstall"
fi

# If the binary is still resolvable (e.g. installed outside cargo, or a
# stale entry remained), surface it so the user can clean it up.
LEFTOVER="$(command -v ccbox || true)"
if [[ -n "$LEFTOVER" ]]; then
  echo "==> note: 'ccbox' still resolves to $LEFTOVER — remove manually if unwanted"
fi

echo "Done. Restart Claude Code to drop the statusline subprocess."
