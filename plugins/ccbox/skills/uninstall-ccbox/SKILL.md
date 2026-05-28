---
name: uninstall-ccbox
description: Use when the user wants to uninstall ccbox, remove the ccbox statusline, or unwire ccbox from Claude Code. Runs the published curl one-liner uninstaller and verifies that `settings.json` no longer points at ccbox and that the binary is gone.
license: Apache-2.0
allowed-tools: Bash(curl:*), Bash(cargo:*), Bash(command:*), Bash(cat:*), Read
---

# Uninstall ccbox

Remove ccbox — the Rust statusline for Claude Code — and confirm it is unwired from the user's `settings.json` and removed from disk.

## When to use this skill

- "uninstall ccbox"
- "remove ccbox"
- "unwire the ccbox statusline"
- Any equivalent phrasing where the user wants ccbox gone.

## Procedure

1. **Confirm the user really wants to uninstall.** This is destructive — it removes the binary and edits `settings.json`. Ask once before running, unless the user's message is already unambiguous ("uninstall ccbox now", "yes uninstall it").

2. **Run the uninstaller.** Execute:

   ```bash
   curl -fsSL https://raw.githubusercontent.com/tom-ha/ccbox/main/uninstall.sh | bash
   ```

   The script unwires `statusLine` from `settings.json` (only if it currently points at a ccbox binary — a foreign statusline is preserved untouched), removes the `ccbox-subscription` marker and the `ccbox-cache/` directory under `$CLAUDE_CONFIG_DIR` (or `~/.claude`), and runs `cargo uninstall ccbox`. It backs `settings.json` up to a timestamped `.bak` file first.

3. **Verify the binary is gone.** Run `command -v ccbox`. It MUST NOT resolve. If it still does, surface the path — typically this means ccbox was installed by something other than `cargo install` (e.g. a package manager or a manual copy into `/usr/local/bin`) and the user needs to remove it by hand.

4. **Verify settings.json is clean.** Read `$CLAUDE_CONFIG_DIR/settings.json` (or `~/.claude/settings.json`) and confirm `statusLine.command` no longer points at ccbox. If the field is missing entirely (because the uninstaller stripped the only entry) that is the expected result.

5. **Tell the user to restart Claude Code.** The old statusline subprocess will keep running until Claude Code is fully restarted.

## Notes

- The uninstaller requires `python3` (to edit `settings.json`). `cargo` is only needed if the binary is still installed; the script tolerates its absence.
- If `statusLine.command` does not point at ccbox, the uninstaller leaves it alone and says so — surface that note to the user rather than treating it as a failure.
- The timestamped `settings.json.bak.*` file is left in place; mention it to the user so they know how to restore the prior config if they change their mind.
