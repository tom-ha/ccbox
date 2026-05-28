---
name: "ccbox"
description: "Toggle ccbox tasks/subagents rows at runtime (no restart). Subcommands: show|hide|flip <tasks|subagents>, status."
category: Statusline
tags: [ccbox, statusline, toggles]
---

Run the ccbox toggle subcommand and relay its output to the user.

**Input**: The arguments after `/ccbox` map directly onto `ccbox toggle …`:

- `/ccbox show tasks` — force the tasks row visible
- `/ccbox hide tasks` — force the tasks row hidden
- `/ccbox flip tasks` — invert the current effective visibility of the tasks row
- `/ccbox show subagents` / `hide subagents` / `flip subagents` — same, for subagents
- `/ccbox status` — print the resolved visibility and source (state_file / env / density) for each gated row

The state file at `<claude_dir>/ccbox-toggles.json` takes precedence over `CCBOX_SHOW_TASKS` / `CCBOX_SHOW_SUBAGENTS`, which take precedence over `CCBOX_DENSITY`. The next statusline render reflects the new state — no Claude Code restart required.

## Steps

1. Use the **Bash tool** to invoke:
   ```bash
   if command -v ccbox >/dev/null 2>&1; then
     ccbox toggle $ARGUMENTS
   elif [ -x "$HOME/.cargo/bin/ccbox" ]; then
     "$HOME/.cargo/bin/ccbox" toggle $ARGUMENTS
   else
     echo "ccbox: binary not found on PATH or at ~/.cargo/bin/ccbox. Install it with: cargo install --git https://github.com/tom-ha/ccbox.git --locked" >&2
     exit 127
   fi
   ```

2. Print the command's stdout back to the user verbatim. If exit code is non-zero, surface stderr too.

3. If the user passed no arguments, the binary prints its own usage; relay that.

## Notes

- The command mutates only the JSON state file. It does not edit shell rc, `settings.json`, or anything else.
- To clear an override and return a row to env-var / density behavior, delete the corresponding key from `<claude_dir>/ccbox-toggles.json` by hand (a future revision may add a `/ccbox clear <row>` subcommand).
- `ccbox toggle status` is read-only — safe to run anytime to inspect what's driving the current visibility.
