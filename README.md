# ccbox

A fast, Rust statusline for [Claude Code](https://claude.com/claude-code) — drawn under every prompt. It surfaces what's actually relevant during a session: which model you're on, context-window burn, session tokens and cost, in-flight tasks and subagents, OpenSpec progress, and your active Python venv — themed and configurable from your shell rc.

![ccbox statusline](docs/hero.png)
<!-- TODO: drop a real hero screenshot at docs/hero.png to replace this placeholder. -->

```
╭───────────────────────────────────────────────────────[ 51e977df…  1h23m ]─╮
│ 󰉋  /Users/you/code/ccbox                                    main    󰢹 Sonnet 4.6│
├┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┤
│ ctx  120K of 200.0K (60%)  ████████████████████████░░░░░░░░░░░░░░░░░░░░     │
├┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┤
│ ↓ in 120K   ↑ out 3.4K   $0.18 sess · $1.42 today   󱢧 4.2K t/m ▁▂▃▅▆▇█▇▆▅▃▂▁│
├┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┤
│  todo 2   ▶  doing 1  ship-the-feature   done 5 ✓                            │
╰─────────────────────────────────────────────────────────────────────────────╯
```

## Features

- **Model identity** — see which Claude model the session is on at a glance.
- **Context-window burn** — token count, percentage, and a progress bar, with thresholds that adapt to 200K and 1M models.
- **Tokens & cost** — input / output token totals and per-session / per-day spend (auto-hidden for subscription accounts).
- **Tasks** — todo / doing / done counts inline, or a multi-column kanban **board** view for wider terminals.
- **Subagents** — in-flight subagent activity from the current session.
- **OpenSpec progress** — bars and counts when there's an active OpenSpec change.
- **Python venv** — name of the activated venv shown on the top row, when present.
- **Theming** — multiple built-in themes, switchable from an env var or `--theme`.

## Requirements

- **A Rust toolchain** (stable) — needed to `cargo install` ccbox. Install via [rustup](https://rustup.rs/) if you don't have one.
- **Claude Code** — ccbox runs as Claude Code's `statusLine` subprocess. Install from [claude.com/claude-code](https://claude.com/claude-code).
- **A [Nerd Font](https://www.nerdfonts.com/)** in your terminal — ccbox uses glyphs (folder, branch, model, task icons, etc.) that **only render correctly with a Nerd Font**. Without one, you'll see boxes / question marks in place of the glyphs; that's the typical "it looks broken" failure mode.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/tom-ha/ccbox/main/install.sh | bash
```

This runs `cargo install --git` to build the binary, then patches `~/.claude/settings.json` so Claude Code invokes `ccbox` as its statusline. Restart Claude Code afterwards. The installer backs up your existing `settings.json` to `settings.json.bak.YYYYMMDD-HHMMSS`.

Want to read the script before running it? Fetch the same URL without the `| bash` to inspect it (`curl -fsSL https://raw.githubusercontent.com/tom-ha/ccbox/main/install.sh`).

If you'd rather not pipe a script to bash, install the binary directly and wire up `settings.json` by hand:

```bash
cargo install --git https://github.com/tom-ha/ccbox.git --locked
```

Then add to `~/.claude/settings.json`:

```json
{ "statusLine": { "type": "command", "command": "/path/to/ccbox" } }
```

### Uninstall

```bash
curl -fsSL https://raw.githubusercontent.com/tom-ha/ccbox/main/uninstall.sh | bash
```

This unwires `statusLine` from `~/.claude/settings.json` (only if it currently points at a ccbox binary — a foreign statusline is left alone), removes ccbox's files under `~/.claude` (the `ccbox-subscription` marker and the `ccbox-cache/` directory), and runs `cargo uninstall ccbox` to drop the binary. Your `settings.json` is backed up to `settings.json.bak.YYYYMMDD-HHMMSS` first. Restart Claude Code afterwards.

### Install via the Claude Code plugin

You can also install ccbox from inside Claude Code. Add this repo as a plugin marketplace, install the `ccbox` plugin, then ask Claude to "install ccbox" — the plugin's `install-ccbox` skill runs the same one-liner and verifies `command -v ccbox` resolves and `settings.json` was patched.

```text
/plugin marketplace add tom-ha/ccbox
/plugin install ccbox@ccbox
```

## Quick start

After installing, **fully restart Claude Code** (the statusline is wired up at startup). On the next prompt you should see the multi-line box appear underneath, with your repo path, branch, model, and a context bar — much like the example up top.

Want to tweak something straight away? Two one-liners worth trying first:

```bash
export CLAUDE_STATUSLINE_THEME=tokyonight   # try a different theme
export CCBOX_DENSITY=verbose                # show every available row
```

Restart Claude Code after changing env vars — the statusline subprocess inherits its environment from the Claude Code parent, so changes only take effect on the next launch. See **Customize** below for the full list of knobs.

## Customize

ccbox reads configuration entirely from environment variables — exported in your shell rc so they're inherited by the Claude Code process and forwarded to the statusline subprocess. There is no config file.

### Width and theme

| Variable | Default | What it does |
|---|---|---|
| `CCBOX_MAX_WIDTH` | `140` | Caps rendered width in columns. |
| `CCBOX_FULL_WIDTH` | unset | If set (any value), renders at the full terminal width instead of capping at `CCBOX_MAX_WIDTH`. |
| `CLAUDE_STATUSLINE_THEME` | unset | Theme name. Run `ccbox --help` or browse [`src/theme/builtin.rs`](src/theme/builtin.rs) for the full list. |

Equivalent CLI flags: `--width`, `--full-width`, `--theme NAME`, `--bg-shift warm|cool`.

### Density preset

The density preset controls which **event-driven** rows participate in the rendered box. Each event-driven row still requires its own content to be present (e.g. tasks only render when the session has a TaskList) — the preset gates *which* of those rows are allowed to render at all.

| Variable | Default | Values |
|---|---|---|
| `CCBOX_DENSITY` | `standard` | `minimal` · `standard` · `verbose` |

- `minimal` — only the top row (path/branch/venv/model), the context line, and the tokens/cost line. No tasks, no subagents, no openspec, no plugins/skills, even when present.
- `standard` (default) — adds the task row, subagent rows, and OpenSpec bars when there is content for each.
- `verbose` — same as standard plus the plugins+skills summary row when present.

```bash
export CCBOX_DENSITY=minimal  # quietest
export CCBOX_DENSITY=verbose  # show everything you've got
```

### Tasks view (inline vs. board)

| Variable | Default | Values |
|---|---|---|
| `CCBOX_TASKS` | `board` | `board` · `inline` |

- `board` (default) — the task row is a multi-line kanban board with three columns (TODO / DOING / DONE), bullets per column, up to five tasks per column, and a `+N more` overflow indicator. Auto-degrades to inline below 100 columns.
- `inline` — the task row is a single-line kanban: `todo N   ▶ doing N  <active-subject>   done N ✓`.

```bash
export CCBOX_TASKS=inline
```

### Cost cell

| Variable | Default | What it does |
|---|---|---|
| `CCBOX_SHOW_COST` | auto | `1`/`true` forces the `$X sess · $Y today` cost cell visible. `0`/`false` hides it. Unset → ccbox auto-detects: subscription sessions (those that report `rate_limits.five_hour.resets_at`) get cost hidden; API users keep cost visible. |

A persistent marker (`<claude_dir>/ccbox-subscription`) is written the first time ccbox sees populated `rate_limits`, so fresh sessions on a subscription account still get cost-hidden behaviour before the first message lands.

### Python venv indicator

If you launch Claude Code from inside an activated Python venv, ccbox shows the venv name on the top row, just to the left of the model identity:

```
│ 󰉋 ~/proj            󰌠 venv: py311 · 󰢹 Sonnet 4.6│
```

ccbox reads the venv name from these environment variables, in order:

| Variable | What it does |
|---|---|
| `VIRTUAL_ENV_PROMPT` | The short name (some venv tools and `uv` set this). Preferred when present. |
| `VIRTUAL_ENV` | The path to the active venv; ccbox displays the basename (e.g. `/opt/conda/envs/py311` → `py311`, `/Users/me/proj/.venv` → `.venv`). |

If neither is set, no venv slot is rendered. Activate the venv **before** starting Claude Code — ccbox runs as a subprocess of Claude Code and inherits the parent's environment, so activations done inside Claude Code's shell tools won't appear.

### Git cache

`GitInfo` (branch, ahead/behind, dirty markers) is shelled out to `git` on each render. To absorb the high frequency of statusline calls during streaming responses, results are cached on disk under `<claude_dir>/ccbox-cache/git/<hash>.json`.

| Variable | Default | What it does |
|---|---|---|
| `CCBOX_GIT_CACHE_TTL_MS` | `2000` | TTL in milliseconds for the cache. `0` disables caching. Stale or missing entries trigger a live read and a refresh of the cache file. |

The cache is per-cwd (FNV1a-hashed for a stable, filesystem-safe filename) and is refreshed atomically via a tempfile + rename, so concurrent ccbox processes don't clobber each other's entries.

### Context-window percentage on 1M-context models

The context-line `%` is computed against an **effective limit** derived from the session's reported `context_window_size`, not a hard-coded constant. The threshold is 75% of the window:

| Model context window | Effective limit | What `100%` means |
|---|---|---|
| 200K (default Claude Code) | 150K tokens | Auto-compaction zone. |
| 1M (e.g. `Opus 4.7 (1M context)`) | 750K tokens | The "you should think about `/compact` soon" line for a 1M window. |
| Unknown / unreported | 150K tokens | Legacy fallback (the bar still renders as `X of ?`). |

This is automatic — nothing to configure.

### CLI flags

```
Usage: ccbox [--theme NAME] [--width COLS] [--full-width] [--bg-shift warm|cool] [--snapshot]
```

`ccbox --help` prints the full list, including the env-var reference. The `ccbox-demo` binary renders the bundled fixture at a progression of widths and theme variants — useful for previewing changes.

#### `--snapshot`

For diagnostics: instead of printing the ANSI-styled box, `--snapshot` writes a single JSON object to stdout containing the parsed `SessionInfo`, resolved `Env`, theme, layout selection, per-component visibility, and the computed values feeding the visible rows (model name, short pwd, branch, costs, tokens-per-minute, fill ratio). No ANSI escapes are emitted.

```bash
ccbox --snapshot < session.json | jq '.composition.body'
ccbox --snapshot --width 100 < session.json | jq '.computed.session_cost_usd'
```

Useful for answering "why isn't this row rendering?" without reading source: the per-component `visible` flag in `composition.body` is the answer.

### Layouts

ccbox picks a layout based on the effective terminal width:

- **Narrow** (`< 55` cols): two-line content (path + branch on line 1, venv/model on line 2). Compact context line, no tokens-cost row.
- **Medium** (`55–79` cols): single-line content, single-percentage compact context line, tokens-cost row.
- **Wide** (`≥ 80` cols): full layout — top row, full context line with `tokens of window (pct%) bar`, tokens-cost row with augmented in/out labels and consolidated cost cell, plus event-driven rows (tasks/subagents/openspec/plugins-skills) gated by the density preset.

The CLI flag `--width COLS` lets you preview a specific size regardless of the actual terminal.

## Contributing

PRs and issues welcome. The dev loop:

```bash
git clone https://github.com/<your-fork>/ccbox.git
cd ccbox
cargo test                  # run the test suite
cargo install --path .      # install your local build over the released one
```

## Acknowledgments

- Inspired by [yet-another-statusline](https://github.com/tmck-code/yet-another-statusline)
- Built for [Claude Code](https://claude.com/claude-code).
