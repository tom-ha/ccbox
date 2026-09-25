use std::env;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ccbox::{
    config::{
        parse_bool_tristate, parse_density, parse_tasks_view, resolve_row_visibility, Density, Env,
        RowVisibilitySource, TasksView,
    },
    consts::{DEFAULT_MAX_WIDTH, MIN_WIDTH},
    input::session::SessionInfo,
    input::toggles::{self, Toggles},
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
    let raw_args: Vec<String> = env::args().skip(1).collect();
    let rest = raw_args.get(1..).unwrap_or_default();
    match raw_args.first().map(String::as_str) {
        Some("usage-refresh") => {
            ccbox::data::account_usage::refresh(&resolve_claude_dir(), now_secs());
            return ExitCode::SUCCESS;
        }
        Some("hook") => return run_hook(),
        Some("update-check") => {
            ccbox::data::update_check::run_check(
                &resolve_claude_dir(),
                now_secs(),
                update_check_enabled(),
                &ccbox::release::releases_url(),
            );
            return ExitCode::SUCCESS;
        }
        Some("setup") => return run_setup(),
        Some("update") => return run_update(rest),
        Some("toggle") => return run_toggle(rest),
        Some(verb @ ("show" | "hide" | "flip")) => return run_row_verb(verb, rest),
        Some("status") => return run_status(rest),
        Some("version") => return run_version(rest),
        Some(other) if !other.starts_with('-') => {
            eprintln!("ccbox: unknown command: {other}");
            print_usage();
            return ExitCode::from(2);
        }
        _ => {}
    }

    let mut theme_cli: Option<String> = None;
    let mut width_cli: Option<u16> = None;
    let mut full_width = env::var_os("CCBOX_FULL_WIDTH").is_some();
    let mut bg_shift = BgShift::Warm;
    let mut snapshot = false;
    let mut args: std::vec::IntoIter<String> = raw_args.into_iter();
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
        } else if arg == "--version" {
            println!("ccbox {VERSION}");
            return ExitCode::SUCCESS;
        } else {
            eprintln!("ccbox: unknown flag: {arg}");
            print_usage();
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
    let show_tasks_override = parse_bool_tristate(env::var("CCBOX_SHOW_TASKS").ok().as_deref());
    let show_subagents_override =
        parse_bool_tristate(env::var("CCBOX_SHOW_SUBAGENTS").ok().as_deref());
    let toggles = toggles::load(&claude_dir);
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
        show_tasks_override,
        show_subagents_override,
        toggles,
        venv,
        density,
        tasks_view,
        git_cache_ttl_ms,
        update_check: update_check_enabled(),
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

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn run_setup() -> ExitCode {
    let exe = env::current_exe().and_then(std::fs::canonicalize);
    let result = exe
        .map_err(|e| format!("cannot find this binary: {e}"))
        .and_then(|exe| ccbox::setup::run(&resolve_claude_dir(), &exe));
    match result {
        Ok(report) => {
            ccbox::update::print_setup_report(&mut io::stdout(), &report);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("ccbox setup: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_update(args: &[String]) -> ExitCode {
    let args = match ccbox::update::parse_args(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("ccbox update: {e}");
            print_usage();
            return ExitCode::from(2);
        }
    };
    match ccbox::update::run(&args, &mut io::stdout()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ccbox update: {e}");
            ExitCode::FAILURE
        }
    }
}

fn update_check_enabled() -> bool {
    parse_bool_tristate(env::var("CCBOX_UPDATE_CHECK").ok().as_deref()) != Some(false)
}

fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn parse_bg(v: &str, fallback: BgShift) -> BgShift {
    match v.to_ascii_lowercase().as_str() {
        "warm" => BgShift::Warm,
        "cool" => BgShift::Cool,
        _ => fallback,
    }
}

/// Always exits 0 so a hook can never block or fail the session it runs in.
fn run_hook() -> ExitCode {
    use std::io::Read;
    let mut raw = String::new();
    let _ = std::io::stdin().read_to_string(&mut raw);
    if let Ok(input) = serde_json::from_str::<ccbox::data::waiting::HookInput>(&raw) {
        ccbox::data::waiting::apply_hook(
            &resolve_claude_dir(),
            &input,
            now_secs(),
            ccbox::data::waiting::owner_pid,
        );
    }
    ExitCode::SUCCESS
}

/// Resolve the Claude config directory the way `main()` does, so the
/// `toggle` subcommand reads/writes the same `ccbox-toggles.json` the
/// statusline renders against.
fn resolve_claude_dir() -> PathBuf {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"))
}

/// Resolve the current density preset for the `toggle` subcommand. Same
/// fallback as `main()`.
fn resolve_density() -> Density {
    env::var("CCBOX_DENSITY")
        .ok()
        .as_deref()
        .and_then(parse_density)
        .unwrap_or_default()
}

/// `ccbox toggle …` entry point. Args is everything after the literal
/// `toggle` token. Mutates `<claude_dir>/ccbox-toggles.json` (atomically
/// via the input::toggles module) or prints the resolved table.
fn run_toggle(args: &[String]) -> ExitCode {
    let claude_dir = resolve_claude_dir();
    let env_tasks = parse_bool_tristate(env::var("CCBOX_SHOW_TASKS").ok().as_deref());
    let env_subs = parse_bool_tristate(env::var("CCBOX_SHOW_SUBAGENTS").ok().as_deref());
    let density = resolve_density();
    let current = toggles::load(&claude_dir);

    match args.first().map(String::as_str) {
        Some("show") => apply_set(LEGACY_TOGGLE_PREFIX, &claude_dir, current, args.get(1), Some(true)),
        Some("hide") => apply_set(LEGACY_TOGGLE_PREFIX, &claude_dir, current, args.get(1), Some(false)),
        Some("flip") => apply_flip(
            LEGACY_TOGGLE_PREFIX,
            &claude_dir,
            current,
            args.get(1),
            env_tasks,
            env_subs,
            density,
        ),
        Some("status") => print_status(&current, env_tasks, env_subs, density),
        Some(other) => {
            eprintln!("ccbox toggle: unknown subcommand: {other}");
            print_toggle_help();
            ExitCode::from(2)
        }
        None => {
            print_toggle_help();
            ExitCode::SUCCESS
        }
    }
}

const LEGACY_TOGGLE_PREFIX: &str = "ccbox toggle";
const VERB_PREFIX: &str = "ccbox";

fn parse_row(prefix: &str, name: Option<&String>) -> Result<Row, ExitCode> {
    match name.map(String::as_str) {
        Some("tasks") => Ok(Row::Tasks),
        Some("subagents") => Ok(Row::Subagents),
        Some(other) => {
            eprintln!("{prefix}: unknown row: {other} (expected 'tasks' or 'subagents')");
            Err(ExitCode::from(2))
        }
        None => {
            eprintln!("{prefix}: missing row argument (expected 'tasks' or 'subagents')");
            Err(ExitCode::from(2))
        }
    }
}

fn run_row_verb(verb: &str, args: &[String]) -> ExitCode {
    if args.len() > 1 {
        eprintln!("ccbox {verb}: unexpected argument: {}", args[1]);
        print_usage();
        return ExitCode::from(2);
    }
    if parse_row(VERB_PREFIX, args.first()).is_err() {
        print_usage();
        return ExitCode::from(2);
    }
    let claude_dir = resolve_claude_dir();
    let current = toggles::load(&claude_dir);
    match verb {
        "show" => apply_set(VERB_PREFIX, &claude_dir, current, args.first(), Some(true)),
        "hide" => apply_set(VERB_PREFIX, &claude_dir, current, args.first(), Some(false)),
        _ => apply_flip(
            VERB_PREFIX,
            &claude_dir,
            current,
            args.first(),
            parse_bool_tristate(env::var("CCBOX_SHOW_TASKS").ok().as_deref()),
            parse_bool_tristate(env::var("CCBOX_SHOW_SUBAGENTS").ok().as_deref()),
            resolve_density(),
        ),
    }
}

fn reject_extra(cmd: &str, args: &[String]) -> Option<ExitCode> {
    let extra = args.first()?;
    eprintln!("ccbox {cmd}: unexpected argument: {extra}");
    print_usage();
    Some(ExitCode::from(2))
}

fn run_version(args: &[String]) -> ExitCode {
    if let Some(code) = reject_extra("version", args) {
        return code;
    }
    println!("ccbox {VERSION}");
    ExitCode::SUCCESS
}

fn run_status(args: &[String]) -> ExitCode {
    if let Some(code) = reject_extra("status", args) {
        return code;
    }
    let claude_dir = resolve_claude_dir();
    let env_tasks = parse_bool_tristate(env::var("CCBOX_SHOW_TASKS").ok().as_deref());
    let env_subs = parse_bool_tristate(env::var("CCBOX_SHOW_SUBAGENTS").ok().as_deref());
    print_status(&toggles::load(&claude_dir), env_tasks, env_subs, resolve_density());
    let cache = ccbox::data::update_check::read_cache(&claude_dir);
    let latest = if cache.checked_at <= 0.0 {
        "unknown (not checked yet)".to_string()
    } else {
        let t = chrono::DateTime::from_timestamp(cache.checked_at as i64, 0)
            .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default();
        match (&cache.latest, cache.newer_than_installed()) {
            (Some(v), Some(_)) => format!("{v} (checked {t}; run ccbox update)"),
            (Some(v), None) => format!("{v} (checked {t})"),
            (None, _) => format!("none published (checked {t})"),
        }
    };
    println!();
    println!("installed     {VERSION}");
    println!("latest        {latest}");
    println!(
        "update check  {}",
        if update_check_enabled() { "on" } else { "off (CCBOX_UPDATE_CHECK)" }
    );
    ExitCode::SUCCESS
}

#[derive(Clone, Copy)]
enum Row {
    Tasks,
    Subagents,
}

fn set_row(toggles: &mut Toggles, row: Row, value: Option<bool>) {
    match row {
        Row::Tasks => toggles.show_tasks = value,
        Row::Subagents => toggles.show_subagents = value,
    }
}

fn current_state_value(toggles: &Toggles, row: Row) -> Option<bool> {
    match row {
        Row::Tasks => toggles.show_tasks,
        Row::Subagents => toggles.show_subagents,
    }
}

fn effective_value(
    toggles: &Toggles,
    row: Row,
    env_tasks: Option<bool>,
    env_subs: Option<bool>,
    density: Density,
) -> bool {
    let (state, env_var) = match row {
        Row::Tasks => (toggles.show_tasks, env_tasks),
        Row::Subagents => (toggles.show_subagents, env_subs),
    };
    let (override_val, _src) = resolve_row_visibility(state, env_var);
    override_val.unwrap_or_else(|| match row {
        Row::Tasks => density.includes_tasks(),
        Row::Subagents => density.includes_subagents(),
    })
}

fn apply_set(
    prefix: &str,
    claude_dir: &Path,
    current: Toggles,
    row_arg: Option<&String>,
    value: Option<bool>,
) -> ExitCode {
    let row = match parse_row(prefix, row_arg) {
        Ok(r) => r,
        Err(code) => return code,
    };
    let mut next = current;
    set_row(&mut next, row, value);
    if let Err(e) = toggles::save(claude_dir, &next) {
        eprintln!("{prefix}: failed to write state file: {e}");
        return ExitCode::from(1);
    }
    let row_name = match row {
        Row::Tasks => "tasks",
        Row::Subagents => "subagents",
    };
    let verb = match value {
        Some(true) => "shown",
        Some(false) => "hidden",
        None => "cleared",
    };
    println!("ccbox: {row_name} row {verb}");
    ExitCode::SUCCESS
}

fn apply_flip(
    prefix: &str,
    claude_dir: &Path,
    current: Toggles,
    row_arg: Option<&String>,
    env_tasks: Option<bool>,
    env_subs: Option<bool>,
    density: Density,
) -> ExitCode {
    let row = match parse_row(prefix, row_arg) {
        Ok(r) => r,
        Err(code) => return code,
    };
    let effective = effective_value(&current, row, env_tasks, env_subs, density);
    apply_set(prefix, claude_dir, current, row_arg, Some(!effective))
}

fn print_status(
    toggles: &Toggles,
    env_tasks: Option<bool>,
    env_subs: Option<bool>,
    density: Density,
) -> ExitCode {
    println!("row         visible  source");
    for (name, row, env_var) in [
        ("tasks", Row::Tasks, env_tasks),
        ("subagents", Row::Subagents, env_subs),
    ] {
        let state = current_state_value(toggles, row);
        let (_override_val, source) = resolve_row_visibility(state, env_var);
        let visible = effective_value(toggles, row, env_tasks, env_subs, density);
        let src = match source {
            RowVisibilitySource::StateFile => "state_file",
            RowVisibilitySource::Env => "env",
            RowVisibilitySource::Density => "density",
        };
        println!("{name:<10}  {:<7}  {src}", visible.to_string());
    }
    ExitCode::SUCCESS
}

/// Run the same logic as `apply_set` against an explicit `claude_dir`,
/// returning the resulting `Toggles` on disk so unit tests can assert
/// without going through env-var resolution.
#[cfg(test)]
fn apply_set_for_test(claude_dir: &Path, row: Row, value: Option<bool>) -> ExitCode {
    let current = toggles::load(claude_dir);
    let mut next = current;
    set_row(&mut next, row, value);
    if toggles::save(claude_dir, &next).is_err() {
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod toggle_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parse_row_accepts_tasks() {
        match parse_row(VERB_PREFIX, Some(&"tasks".to_string())) {
            Ok(Row::Tasks) => {}
            _ => panic!("expected Row::Tasks"),
        }
    }

    #[test]
    fn parse_row_accepts_subagents() {
        match parse_row(VERB_PREFIX, Some(&"subagents".to_string())) {
            Ok(Row::Subagents) => {}
            _ => panic!("expected Row::Subagents"),
        }
    }

    #[test]
    fn parse_row_rejects_other() {
        assert!(parse_row(VERB_PREFIX, Some(&"openspec".to_string())).is_err());
        assert!(parse_row(VERB_PREFIX, None).is_err());
    }

    #[test]
    fn show_writes_true_to_state_file() {
        let dir = tempdir().unwrap();
        let code = apply_set_for_test(dir.path(), Row::Tasks, Some(true));
        // ExitCode doesn't expose its inner value; round-trip via the file is the assertion.
        let _ = code;
        let t = toggles::load(dir.path());
        assert_eq!(t.show_tasks, Some(true));
        assert_eq!(t.show_subagents, None);
    }

    #[test]
    fn hide_writes_false_to_state_file() {
        let dir = tempdir().unwrap();
        apply_set_for_test(dir.path(), Row::Subagents, Some(false));
        let t = toggles::load(dir.path());
        assert_eq!(t.show_subagents, Some(false));
    }

    #[test]
    fn show_then_hide_preserves_other_field() {
        let dir = tempdir().unwrap();
        apply_set_for_test(dir.path(), Row::Tasks, Some(true));
        apply_set_for_test(dir.path(), Row::Subagents, Some(false));
        let t = toggles::load(dir.path());
        assert_eq!(t.show_tasks, Some(true));
        assert_eq!(t.show_subagents, Some(false));
    }

    #[test]
    fn flip_inverts_state_file_value() {
        let dir = tempdir().unwrap();
        let initial = Toggles {
            show_tasks: Some(true),
            ..Default::default()
        };
        toggles::save(dir.path(), &initial).unwrap();
        // density irrelevant here because state file wins.
        let effective = effective_value(&initial, Row::Tasks, None, None, Density::Standard);
        apply_set_for_test(dir.path(), Row::Tasks, Some(!effective));
        let t = toggles::load(dir.path());
        assert_eq!(t.show_tasks, Some(false));
    }

    #[test]
    fn flip_uses_env_var_when_state_absent() {
        // No state file. CCBOX_SHOW_TASKS=true → effective=true → flip persists false.
        let dir = tempdir().unwrap();
        let current = toggles::load(dir.path());
        let effective = effective_value(&current, Row::Tasks, Some(true), None, Density::Minimal);
        assert!(effective, "env should drive effective when no state");
        apply_set_for_test(dir.path(), Row::Tasks, Some(!effective));
        let t = toggles::load(dir.path());
        assert_eq!(t.show_tasks, Some(false));
    }

    #[test]
    fn flip_uses_density_when_state_and_env_absent() {
        let dir = tempdir().unwrap();
        let current = toggles::load(dir.path());
        // Minimal density: includes_tasks() == false → effective=false → flip writes true.
        let effective_minimal = effective_value(&current, Row::Tasks, None, None, Density::Minimal);
        assert!(!effective_minimal);
        // Standard density: includes_tasks() == true → effective=true → flip writes false.
        let effective_standard =
            effective_value(&current, Row::Tasks, None, None, Density::Standard);
        assert!(effective_standard);
    }

    #[test]
    fn effective_value_state_beats_env() {
        let toggles = Toggles {
            show_tasks: Some(false),
            ..Default::default()
        };
        assert!(!effective_value(
            &toggles,
            Row::Tasks,
            Some(true),
            None,
            Density::Verbose
        ));
    }

    #[test]
    fn status_does_not_panic_with_no_inputs() {
        // The function returns an ExitCode and prints to stdout; just exercise it.
        let toggles = Toggles::default();
        let code = print_status(&toggles, None, None, Density::Standard);
        let _ = code;
    }

    #[test]
    fn set_row_clears_to_none() {
        let mut toggles = Toggles {
            show_tasks: Some(true),
            show_subagents: Some(false),
        };
        set_row(&mut toggles, Row::Tasks, None);
        assert_eq!(toggles.show_tasks, None);
        assert_eq!(toggles.show_subagents, Some(false));
    }
}

fn print_toggle_help() {
    eprintln!(
        "Usage: ccbox toggle <show|hide|flip> <tasks|subagents>\n       ccbox toggle status\n\
         \n\
         Mutates <claude_dir>/ccbox-toggles.json. Takes effect on the next\n\
         statusline render; no Claude Code restart required.\n\
         \n\
         Precedence: state file > CCBOX_SHOW_TASKS / CCBOX_SHOW_SUBAGENTS > CCBOX_DENSITY."
    );
}

const USAGE: &str = "\
Usage:
  ccbox [--theme NAME] [--width COLS] [--full-width] [--bg-shift warm|cool] [--snapshot]
  ccbox show|hide|flip tasks|subagents
  ccbox status
  ccbox update [--check] [--version X.Y.Z] [--force]
  ccbox version";

fn print_usage() {
    eprintln!("{USAGE}");
}

fn print_help() {
    eprintln!("{USAGE}\n\n{HELP}");
}

const HELP: &str = r#"With no command, ccbox reads Claude Code session JSON on stdin and writes a
multi-line ANSI statusline to stdout. With --snapshot it writes one JSON object
instead: the parsed session, resolved env, layout, per-component visibility and
computed values, with no ANSI escapes. --version prints the installed version.

Commands:
  show|hide|flip ROW    Force the tasks or subagents row visible, hidden, or the
                        opposite of what it is now. Writes
                        <claude_dir>/ccbox-toggles.json; the next render uses it
                        without a Claude Code restart.
  status                Each row's visibility and what decided it, then the
                        installed version, the latest known release, and
                        whether the update check is on.
  update                Replace this binary with the latest release (or
                        --version X.Y.Z) after checking its SHA-256, then rewire
                        settings.json for the new version. --check only
                        reports; --force allows a downgrade.
  version               Print the installed version.

Environment variables:
  CCBOX_MAX_WIDTH       Cap rendered width (default 140).
  CCBOX_FULL_WIDTH      If set (any value), render at full terminal width.
  CCBOX_SHOW_COST       1/true/yes/on or 0/false/no/off: force the cost cell
                        visible/hidden; unset = auto-detect from rate-limit data.
  CCBOX_SHOW_TASKS      1/true/yes/on or 0/false/no/off: force the tasks row
                        visible/hidden regardless of CCBOX_DENSITY. Overridden
                        by the state file (<claude_dir>/ccbox-toggles.json) when
                        present; unset = fall through to the density preset.
  CCBOX_SHOW_SUBAGENTS  Same shape as CCBOX_SHOW_TASKS, for the subagents row.
  CCBOX_DENSITY         minimal | standard (default) | verbose: which
                        event-driven rows participate. minimal = ctx +
                        tokens/cost only; standard = + tasks/subagents/openspec
                        when present; verbose = + plugins/skills when present.
  CCBOX_TASKS           board (default) | inline: render the task row as a
                        multi-line kanban board (auto-degrades to inline below
                        ~100 columns) or as a single inline kanban.
  CCBOX_GIT_CACHE_TTL_MS
                        TTL in milliseconds for the on-disk GitInfo cache under
                        <claude_dir>/ccbox-cache/git/ (default 2000; 0 disables
                        caching).
  CCBOX_UPDATE_CHECK    0/false/no/off turns off the daily background check for
                        a new release and the notice on the bottom border;
                        unset = on. Takes effect after a Claude Code restart.
  CCBOX_RELEASES_URL    Where releases are read from, as a GitHub API repo URL
                        (default https://api.github.com/repos/tom-ha/ccbox).
  CLAUDE_STATUSLINE_THEME
                        Theme name: claude-dark (default), claude-light,
                        catppuccin-latte, catppuccin-mocha.
  VIRTUAL_ENV_PROMPT    Short name of the active Python venv (preferred).
  VIRTUAL_ENV           Path to the active Python venv; basename is shown when
                        VIRTUAL_ENV_PROMPT is unset.

Visibility precedence (for tasks/subagents rows):
  <claude_dir>/ccbox-toggles.json  >  CCBOX_SHOW_TASKS / CCBOX_SHOW_SUBAGENTS  >  CCBOX_DENSITY"#;

#[cfg(test)]
mod help_tests {
    use super::{HELP, USAGE};

    #[test]
    fn usage_and_help_hide_internal_entry_points() {
        for text in [USAGE, HELP] {
            for word in text.split(|c: char| c.is_whitespace() || c == '|' || c == '`') {
                assert!(
                    !["hook", "usage-refresh", "update-check", "setup", "toggle"].contains(&word),
                    "{word:?} is internal"
                );
            }
        }
    }

    #[test]
    fn help_lines_keep_their_indentation() {
        assert!(HELP.lines().any(|l| l.starts_with("  CCBOX_MAX_WIDTH ")));
        assert!(HELP.lines().all(|l| l.chars().count() <= 100));
    }
}
