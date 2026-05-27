use std::env;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ccbox::{
    config::{parse_bool_tristate, parse_density, parse_tasks_view, Density, Env, TasksView},
    consts::{DEFAULT_MAX_WIDTH, MIN_WIDTH},
    input::session::SessionInfo,
    render::BgShift,
    render_with,
    terminal::terminal_width,
    theme::resolve::resolve_theme,
};

fn read_venv_name() -> Option<String> {
    if let Some(prompt) = env::var("VIRTUAL_ENV_PROMPT").ok() {
        let trimmed = prompt.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    let raw = env::var("VIRTUAL_ENV").ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let basename = Path::new(trimmed)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| trimmed.to_string());
    Some(basename)
}

fn main() -> ExitCode {
    let mut theme_cli: Option<String> = None;
    let mut width_cli: Option<u16> = None;
    let mut full_width = env::var_os("CCBOX_FULL_WIDTH").is_some();
    let mut bg_shift = BgShift::Warm;
    let mut snapshot = false;
    let mut args: std::vec::IntoIter<String> = env::args().skip(1).collect::<Vec<_>>().into_iter();
    while let Some(arg) = args.next() {
        if let Some(rest) = arg.strip_prefix("--theme=") {
            theme_cli = Some(rest.to_string());
        } else if arg == "--theme" {
            if let Some(v) = args.next() {
                theme_cli = Some(v);
            }
        } else if let Some(rest) = arg.strip_prefix("--width=") {
            width_cli = rest.parse().ok();
        } else if arg == "--width" {
            if let Some(v) = args.next() {
                width_cli = v.parse().ok();
            }
        } else if arg == "--full-width" {
            full_width = true;
        } else if arg == "--snapshot" {
            snapshot = true;
        } else if let Some(rest) = arg.strip_prefix("--bg-shift=") {
            bg_shift = parse_bg(rest, bg_shift);
        } else if arg == "--bg-shift" {
            if let Some(v) = args.next() {
                bg_shift = parse_bg(&v, bg_shift);
            }
        } else if arg == "-h" || arg == "--help" {
            print_help();
            return ExitCode::SUCCESS;
        } else {
            eprintln!("ccbox: unknown flag: {arg}");
            print_help();
            return ExitCode::from(2);
        }
    }

    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    let claude_dir = env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"));

    let max_width: u16 = env::var("CCBOX_MAX_WIDTH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MAX_WIDTH);

    let show_cost_override = parse_bool_tristate(env::var("CCBOX_SHOW_COST").ok().as_deref());
    let density: Density = env::var("CCBOX_DENSITY")
        .ok()
        .as_deref()
        .and_then(parse_density)
        .unwrap_or_default();
    let tasks_view: TasksView = env::var("CCBOX_TASKS")
        .ok()
        .as_deref()
        .and_then(parse_tasks_view)
        .unwrap_or_default();
    let git_cache_ttl_ms: u64 = env::var("CCBOX_GIT_CACHE_TTL_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2000);
    let venv = read_venv_name();
    let env_struct = Env {
        claude_dir: claude_dir.clone(),
        home: home.clone(),
        max_width: Some(max_width),
        full_width,
        show_cost_override,
        venv,
        density,
        tasks_view,
        git_cache_ttl_ms,
    };

    let theme_env = env::var("CLAUDE_STATUSLINE_THEME").ok();
    let (theme, _unknown) = resolve_theme(
        theme_cli.as_deref(),
        theme_env.as_deref(),
        Some(&claude_dir),
    );

    // Read JSON blob from stdin.
    let mut buf = String::new();
    let _ = io::stdin().read_to_string(&mut buf);
    let session: SessionInfo = if buf.trim().is_empty() {
        SessionInfo::default()
    } else {
        serde_json::from_str(&buf).unwrap_or_default()
    };

    let raw_tw = width_cli.unwrap_or_else(|| terminal_width(Some(&claude_dir)));
    if raw_tw < MIN_WIDTH {
        return ExitCode::SUCCESS;
    }
    let width = if full_width {
        raw_tw.saturating_sub(6).max(MIN_WIDTH)
    } else {
        raw_tw.saturating_sub(6).clamp(MIN_WIDTH, max_width)
    };

    let out = if snapshot {
        ccbox::snapshot::render_snapshot(&session, &env_struct, width, theme, bg_shift)
    } else {
        render_with(&session, &env_struct, width, theme, bg_shift)
    };
    let _ = io::stdout().write_all(out.as_bytes());
    let _ = io::stdout().flush();
    ExitCode::SUCCESS
}

fn parse_bg(v: &str, fallback: BgShift) -> BgShift {
    match v.to_ascii_lowercase().as_str() {
        "warm" => BgShift::Warm,
        "cool" => BgShift::Cool,
        _ => fallback,
    }
}

fn print_help() {
    eprintln!(
        "Usage: ccbox [--theme NAME] [--width COLS] [--full-width] [--bg-shift warm|cool] [--snapshot]\n\
         \n\
         Reads Claude Code session JSON on stdin and writes a multi-line ANSI\n\
         statusline to stdout. With --snapshot, writes a single JSON object\n\
         describing the parsed session, resolved env, layout, per-component\n\
         visibility, and computed values — no ANSI escapes.\n\
         \n\
         Environment variables:\n\
         \n\
           CCBOX_MAX_WIDTH       Cap rendered width (default 140).\n\
           CCBOX_FULL_WIDTH      If set (any value), render at full terminal width.\n\
           CCBOX_SHOW_COST       1/true/yes/on or 0/false/no/off — force the cost cell\n\
                                 visible/hidden; unset = auto-detect from rate-limit data.\n\
           CCBOX_DENSITY         minimal | standard (default) | verbose — controls which\n\
                                 event-driven rows participate. minimal = ctx + tokens/cost\n\
                                 only; standard = + tasks/subagents/openspec when present;\n\
                                 verbose = + plugins/skills when present.\n\
           CCBOX_TASKS           board (default) | inline — render the task row as a\n\
                                 multi-line kanban board (default; auto-degrades to inline\n\
                                 below ~100 columns) or as a single inline kanban.\n\
           CCBOX_GIT_CACHE_TTL_MS  TTL in milliseconds for the on-disk GitInfo cache\n\
                                 under <claude_dir>/ccbox-cache/git/ (default 2000;\n\
                                 0 disables caching). Absorbs the high-frequency\n\
                                 statusline calls during streaming responses.\n\
           CLAUDE_STATUSLINE_THEME  Theme name. See --help for the list of built-ins.\n\
           VIRTUAL_ENV_PROMPT    Short name of the active Python venv (preferred).\n\
           VIRTUAL_ENV           Path to the active Python venv; basename is shown when\n\
                                 VIRTUAL_ENV_PROMPT is unset."
    );
}
