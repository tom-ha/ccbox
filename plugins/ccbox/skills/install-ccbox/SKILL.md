---
name: install-ccbox
description: Install ccbox — the Rust statusline for Claude Code — and confirm it is wired into settings.json. User-invoked only; the agent will not auto-trigger this skill.
license: Apache-2.0
allowed-tools: Bash(curl:*), Bash(cargo:*), Bash(ccbox:*), Bash(cat:*), Bash(command:*), Bash(uname:*), Bash(system_profiler:*), Bash(fc-list:*), Bash(grep:*), Read
disable-model-invocation: true
---

# Install ccbox

Install ccbox — the Rust statusline for Claude Code — and confirm it is wired into the user's `settings.json`.

## Procedure

1. **Pre-flight: verify `cargo` is installed.** Run `command -v cargo`. If it does not resolve, **stop** and tell the user that Rust's `cargo` is required to build ccbox. Recommend installing Rust via the official `rustup` installer — **do not suggest Homebrew/`brew install rust`**:

   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

   After installing, the user must open a new shell (or `source "$HOME/.cargo/env"`) so `cargo` lands on `PATH`, then re-run this skill. Do not proceed until `command -v cargo` succeeds.

2. **Pre-flight: best-effort Nerd Font check.** ccbox's glyphs only render correctly with a Nerd Font in the terminal. Detect by OS:

   - **macOS** (`uname -s` = `Darwin`): `system_profiler SPFontsDataType 2>/dev/null | grep -i 'nerd font' | head -5`
   - **Linux** (`uname -s` = `Linux`): `fc-list 2>/dev/null | grep -i 'nerd font' | head -5`
   - **Other / command missing:** skip silently.

   This is advisory and **not a hard fail**. The check looks for "Nerd Font" in the family name and will miss Nerd-patched fonts that use a `NF` / `NFM` suffix instead (e.g. `JetBrainsMono NF`, `MesloLGS NF`). It also can't tell whether the terminal is actually configured to use the font it finds.

   - If nothing matches: tell the user you couldn't detect a Nerd Font, point them at https://www.nerdfonts.com/ to download one (e.g. `JetBrainsMono Nerd Font`, `MesloLGS NF`, `Hack Nerd Font`), and remind them their terminal must be configured to use it. Ask whether to continue anyway — don't block indefinitely; if they say yes or don't object, proceed.
   - If something matches: mention what you found and remind them their terminal must be configured to use it, then proceed.

3. **Check for an existing non-ccbox statusline.** Read `$CLAUDE_CONFIG_DIR/settings.json` (or `~/.claude/settings.json` if `CLAUDE_CONFIG_DIR` is unset). If `statusLine.command` is set to something that is clearly not ccbox, **stop and ask the user before proceeding** — the installer will back the file up, but the user should know they are about to swap a different statusline out. If `statusLine` is unset, already points at `ccbox`, or the file does not exist, proceed without asking.

4. **Run the installer.** Execute:

   ```bash
   curl -fsSL https://raw.githubusercontent.com/tom-ha/ccbox/main/install.sh | bash
   ```

   The script builds ccbox from source via `cargo install --git` (this takes a minute), then patches `settings.json`. It backs up any pre-existing `settings.json` to a timestamped `.bak` file.

5. **Verify the binary is on PATH.** Run `command -v ccbox`. It MUST resolve to a path. If it does not, surface the failure and stop — typically this means cargo's bin directory (e.g. `~/.cargo/bin`) is missing from `PATH`.

6. **Verify settings.json was patched.** Read `$CLAUDE_CONFIG_DIR/settings.json` (or `~/.claude/settings.json`) and confirm `statusLine.command` points at the binary path reported in step 5. If the field is missing or points elsewhere, surface the mismatch.

7. **Tell the user to restart Claude Code.** The statusline subprocess is wired up at Claude Code startup, so the user must fully restart Claude Code before the new statusline appears.

## Notes

- The installer also requires `python3` (to patch `settings.json`). If it is missing the script exits with a clear error — relay it to the user.
- For a forked repo or a branch, the script honors `CCBOX_REPO_URL` and `CCBOX_REPO_BRANCH` env vars. Most users do not need these.
- If the user reports "boxes / question marks" after the restart, the terminal is not actually using a Nerd Font — point them at https://www.nerdfonts.com/.
