# ccbox

A fast Rust statusline for [Claude Code](https://claude.com/claude-code). Renders model identity, context-window burn, session tokens, cost, subagents, tasks (inline or kanban board), OpenSpec progress, and (optionally) your active Python venv into a multi-line ANSI box that's drawn under every Claude Code prompt.

```
╭───────────────────────────────────────────────────────[ 51e977df…  1h23m ]─╮
│ 󰉋  /Users/tomh/git/public/yet-another-statusline-in-rust   main    󰢹 Sonnet 4.6│
├┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┤
│ ctx  120K of 200.0K (60%)  ████████████████████████░░░░░░░░░░░░░░░░░░░░     │
├┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┤
│ ↓ in 120K   ↑ out 3.4K   $0.18 sess · $1.42 today   󱢧 4.2K t/m ▁▂▃▅▆▇█▇▆▅▃▂▁│
├┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┤
│  todo 2   ▶  doing 1  ship-the-feature   done 5 ✓                            │
╰─────────────────────────────────────────────────────────────────────────────╯
```

## Install

```bash
./install.sh
```

This runs `cargo install --path .`, then patches `~/.claude/settings.json` so Claude Code invokes `ccbox` as its statusline. Restart Claude Code afterwards. The installer backs up your existing `settings.json` to `settings.json.bak.YYYYMMDD-HHMMSS`.

If you'd rather wire it up by hand: `cargo install --path .` and then add to `settings.json`:

```json
{ "statusLine": { "type": "command", "command": "/path/to/ccbox" } }
```

## Configuration

ccbox reads configuration entirely from environment variables — exported in your shell rc so they're inherited by the Claude Code process and forwarded to the statusline subprocess. There is no config file.

### Width and theme

| Variable | Default | What it does |
|---|---|---|
| `CCBOX_MAX_WIDTH` | `140` | Caps rendered width in columns. |
| `CCBOX_FULL_WIDTH` | unset | If set (any value), renders at the full terminal width instead of capping at `CCBOX_MAX_WIDTH`. |
| `CLAUDE_STATUSLINE_THEME` | unset | Theme name. Run `ccbox --help` or browse [`src/theme/builtin.rs`](src/theme/builtin.rs) for the full list. |

Equivalent CLI flags: `--width`, `--full-width`, `--theme NAME`, `--bg-shift warm|cool`.

### Cost cell

| Variable | Default | What it does |
|---|---|---|
| `CCBOX_SHOW_COST` | auto | `1`/`true` forces the `$X sess · $Y today` cost cell visible. `0`/`false` hides it. Unset → ccbox auto-detects: subscription sessions (those that report `rate_limits.five_hour.resets_at`) get cost hidden; API users keep cost visible. |

A persistent marker (`<claude_dir>/ccbox-subscription`) is written the first time ccbox sees populated `rate_limits`, so fresh sessions on a subscription account still get cost-hidden behaviour before the first message lands.

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
| `CCBOX_TASKS` | `inline` | `inline` · `board` |

- `inline` (default) — the task row is a single-line kanban: `todo N   ▶ doing N  <active-subject>   done N ✓`.
- `board` — the task row becomes a multi-line kanban board with three columns (TODO / DOING / DONE), bullets per column, up to five tasks per column, and a `+N more` overflow indicator. Auto-degrades to inline below 100 columns.

```bash
export CCBOX_TASKS=board
```

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

### Context-window percentage on 1M-context models

The context-line `%` is computed against an **effective limit** derived from the session's reported `context_window_size`, not a hard-coded constant. The threshold is 75% of the window:

| Model context window | Effective limit | What `100%` means |
|---|---|---|
| 200K (default Claude Code) | 150K tokens | Auto-compaction zone. |
| 1M (e.g. `Opus 4.7 (1M context)`) | 750K tokens | The "you should think about `/compact` soon" line for a 1M window. |
| Unknown / unreported | 150K tokens | Legacy fallback (the bar still renders as `X of ?`). |

This is automatic — nothing to configure.

## CLI flags

```
Usage: ccbox [--theme NAME] [--width COLS] [--full-width] [--bg-shift warm|cool]
```

`ccbox --help` prints the full list, including the env-var reference. The `ccbox-demo` binary renders the bundled fixture at a progression of widths and theme variants — useful for previewing changes.

## Layouts

ccbox picks a layout based on the effective terminal width:

- **Narrow** (`< 55` cols): two-line content (path + branch on line 1, venv/model on line 2). Compact context line, no tokens-cost row.
- **Medium** (`55–79` cols): single-line content, single-percentage compact context line, tokens-cost row.
- **Wide** (`≥ 80` cols): full layout — top row, full context line with `tokens of window (pct%) bar`, tokens-cost row with augmented in/out labels and consolidated cost cell, plus event-driven rows (tasks/subagents/openspec/plugins-skills) gated by the density preset.

The CLI flag `--width COLS` lets you preview a specific size regardless of the actual terminal.
