---
name: install-ccbox
description: Use when the user wants to install ccbox, set up ccbox, or wire up the ccbox statusline for Claude Code. Runs the published curl one-liner installer and verifies that the `ccbox` binary is on PATH and that Claude Code's `settings.json` `statusLine.command` was patched to point at it.
license: Apache-2.0
allowed-tools: Bash(curl:*), Bash(cargo:*), Bash(ccbox:*), Bash(cat:*), Bash(command:*), Read
---

# Install ccbox

Install ccbox — the Rust statusline for Claude Code — and confirm it is wired into the user's `settings.json`.

## When to use this skill

- "install ccbox"
- "set up ccbox"
- "wire up the ccbox statusline"
- Any equivalent phrasing where the user wants ccbox running as their Claude Code statusline.

## Procedure

1. **Check for an existing non-ccbox statusline.** Read `$CLAUDE_CONFIG_DIR/settings.json` (or `~/.claude/settings.json` if `CLAUDE_CONFIG_DIR` is unset). If `statusLine.command` is set to something that is clearly not ccbox, **stop and ask the user before proceeding** — the installer will back the file up, but the user should know they are about to swap a different statusline out. If `statusLine` is unset, already points at `ccbox`, or the file does not exist, proceed without asking.

2. **Run the installer.** Execute:

   ```bash
   curl -fsSL https://raw.githubusercontent.com/tom-ha/ccbox/main/install.sh | bash
   ```

   The script builds ccbox from source via `cargo install --git` (this takes a minute), then patches `settings.json`. It backs up any pre-existing `settings.json` to a timestamped `.bak` file.

3. **Verify the binary is on PATH.** Run `command -v ccbox`. It MUST resolve to a path. If it does not, surface the failure and stop — typically this means cargo's bin directory (e.g. `~/.cargo/bin`) is missing from `PATH`.

4. **Verify settings.json was patched.** Read `$CLAUDE_CONFIG_DIR/settings.json` (or `~/.claude/settings.json`) and confirm `statusLine.command` points at the binary path reported in step 3. If the field is missing or points elsewhere, surface the mismatch.

5. **Tell the user to restart Claude Code.** The statusline subprocess is wired up at Claude Code startup, so the user must fully restart Claude Code before the new statusline appears.

## Notes

- The installer requires `cargo` (Rust toolchain) and `python3`. If either is missing the script exits with a clear error — relay it to the user.
- For a forked repo or a branch, the script honors `CCBOX_REPO_URL` and `CCBOX_REPO_BRANCH` env vars. Most users do not need these.
- ccbox's glyphs only render correctly with a Nerd Font in the terminal. If the user reports "boxes / question marks" after the restart, point them at https://www.nerdfonts.com/.
