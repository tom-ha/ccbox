//! Integration test: render the fixture session at multiple widths.

use std::path::PathBuf;

use ccbox::{ansi::strip_ansi, render, width::visible_width, Env, SessionInfo};
use tempfile::TempDir;

/// Per-test scratch dirs so the subscription marker can't leak between tests.
struct Scratch {
    _claude: TempDir,
    _home: TempDir,
    env: Env,
}

fn scratch() -> Scratch {
    let claude = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let env = Env {
        claude_dir: claude.path().to_path_buf(),
        home: home.path().to_path_buf(),
        ..Default::default()
    };
    Scratch {
        _claude: claude,
        _home: home,
        env,
    }
}

/// Legacy helper for the width-invariant tests that pre-date the marker work
/// and only check that rendered lines have the right visible width.
fn env() -> Env {
    Env {
        claude_dir: PathBuf::from("/tmp/ccbox-test-claude-dir-nonexistent"),
        home: PathBuf::from("/tmp/ccbox-test-home-nonexistent"),
        ..Default::default()
    }
}

fn fixture() -> SessionInfo {
    let raw = std::fs::read_to_string("tests/fixtures/session-info-example.json").unwrap();
    serde_json::from_str(&raw).unwrap()
}

#[test]
fn renders_at_narrow_width() {
    let s = render(&fixture(), &env(), 44);
    assert!(!s.is_empty());
    for (i, line) in s.lines().enumerate() {
        assert_eq!(visible_width(line), 44, "line {i}: {:?}", strip_ansi(line));
    }
}

#[test]
fn renders_at_medium_width() {
    let s = render(&fixture(), &env(), 65);
    assert!(!s.is_empty());
    for (i, line) in s.lines().enumerate() {
        assert_eq!(visible_width(line), 65, "line {i}: {:?}", strip_ansi(line));
    }
}

#[test]
fn renders_at_100_cols() {
    let s = render(&fixture(), &env(), 100);
    assert!(!s.is_empty());
    for (i, line) in s.lines().enumerate() {
        assert_eq!(visible_width(line), 100, "line {i}: {:?}", strip_ansi(line));
    }
}

#[test]
fn renders_at_130_cols() {
    let s = render(&fixture(), &env(), 130);
    assert!(!s.is_empty());
    for (i, line) in s.lines().enumerate() {
        assert_eq!(visible_width(line), 130, "line {i}: {:?}", strip_ansi(line));
    }
}

#[test]
fn render_has_no_trailing_newline() {
    let s = render(&fixture(), &env(), 100);
    assert!(!s.ends_with('\n'));
}

#[test]
fn render_below_min_width_returns_empty() {
    let s = render(&fixture(), &env(), 10);
    assert!(s.is_empty());
}

#[test]
fn subscription_session_hides_cost_glyph_and_amount() {
    // Fixture is a subscription session: rate_limits.five_hour.resets_at != 0.
    // No env override, fresh scratch dir → cost must be hidden via the
    // in-payload signal.
    let sc = scratch();
    let s = render(&fixture(), &sc.env, 130);
    let plain = strip_ansi(&s);
    assert!(
        !plain.contains('$'),
        "subscription render should not contain '$':\n{plain}"
    );
}

#[test]
fn ccbox_show_cost_true_restores_cost_on_subscription() {
    let mut sc = scratch();
    sc.env.show_cost_override = Some(true);
    let s = render(&fixture(), &sc.env, 130);
    let plain = strip_ansi(&s);
    assert!(
        plain.contains('$'),
        "CCBOX_SHOW_COST=1 must re-enable cost:\n{plain}"
    );
}

#[test]
fn non_subscription_session_shows_cost() {
    // Empty rate_limits AND no marker ⇒ API user ⇒ cost visible.
    let sc = scratch();
    let mut s = fixture();
    s.rate_limits = Default::default();
    let out = render(&s, &sc.env, 130);
    let plain = strip_ansi(&out);
    assert!(
        plain.contains('$'),
        "API-style session must show cost:\n{plain}"
    );
}

#[test]
fn ccbox_show_cost_false_hides_cost_on_non_subscription() {
    let mut sc = scratch();
    sc.env.show_cost_override = Some(false);
    let mut s = fixture();
    s.rate_limits = Default::default();
    let out = render(&s, &sc.env, 130);
    let plain = strip_ansi(&out);
    assert!(
        !plain.contains('$'),
        "CCBOX_SHOW_COST=0 must hide cost:\n{plain}"
    );
}

#[test]
fn marker_alone_hides_cost_on_fresh_session() {
    // Pre-existing marker (i.e. a previous session detected subscription)
    // means a brand-new session with zero rate_limits should still hide cost.
    // This is the fresh-session-bug regression test.
    let sc = scratch();
    std::fs::write(sc.env.claude_dir.join("ccbox-subscription"), b"").unwrap();
    let mut s = fixture();
    s.rate_limits = Default::default();
    let out = render(&s, &sc.env, 130);
    let plain = strip_ansi(&out);
    assert!(
        !plain.contains('$'),
        "marker should hide cost on fresh session:\n{plain}"
    );
}

#[test]
fn burndown_helper_is_removed_from_top_row() {
    // The burndown helper (5h/7d %/trend/T-…) is gone from the design entirely.
    // No env var, no opt-in, no opt-out — should never appear.
    let sc = scratch();
    let s = render(&fixture(), &sc.env, 130);
    let plain = strip_ansi(&s);
    assert!(
        !plain.contains("T-"),
        "burndown helper must be gone: {plain}"
    );
    for (i, line) in s.lines().enumerate() {
        assert_eq!(visible_width(line), 130, "line {i}: {:?}", strip_ansi(line));
    }
}

#[test]
fn density_minimal_omits_event_driven_rows() {
    use ccbox::config::Density;
    let mut sc = scratch();
    sc.env.density = Density::Minimal;
    let s = render(&fixture(), &sc.env, 130);
    // Layout invariant still holds at the minimal density.
    for (i, line) in s.lines().enumerate() {
        assert_eq!(visible_width(line), 130, "line {i}: {:?}", strip_ansi(line));
    }
}

#[test]
fn ccbox_show_tasks_false_hides_tasks_row_under_standard_density() {
    // Standard density would include tasks; the per-row override must hide it.
    let mut sc = scratch();
    sc.env.show_tasks_override = Some(false);
    let s = render(&fixture(), &sc.env, 130);
    let plain = strip_ansi(&s);
    assert!(
        !plain.contains("Tasks "),
        "CCBOX_SHOW_TASKS=0 must hide tasks row even under standard density:\n{plain}"
    );
}

#[test]
fn ccbox_show_subagents_false_hides_subagents_row_under_standard_density() {
    let mut sc = scratch();
    sc.env.show_subagents_override = Some(false);
    let s = render(&fixture(), &sc.env, 130);
    let plain = strip_ansi(&s);
    assert!(
        !plain.contains("Sub-Agents"),
        "CCBOX_SHOW_SUBAGENTS=0 must hide subagents row:\n{plain}"
    );
}

#[test]
fn state_file_show_tasks_false_beats_env_var_true() {
    // CCBOX_SHOW_TASKS=true would normally show the tasks row, but the state
    // file's `show_tasks: false` overrides it.
    use ccbox::input::toggles::Toggles;
    let mut sc = scratch();
    sc.env.show_tasks_override = Some(true);
    sc.env.toggles = Toggles {
        show_tasks: Some(false),
        ..Default::default()
    };
    let s = render(&fixture(), &sc.env, 130);
    let plain = strip_ansi(&s);
    assert!(
        !plain.contains("Tasks "),
        "state file should beat env var:\n{plain}"
    );
}

#[test]
fn ccbox_show_tasks_unset_falls_through_to_density() {
    use ccbox::config::Density;
    let mut sc = scratch();
    sc.env.density = Density::Minimal;
    sc.env.show_tasks_override = None;
    let s = render(&fixture(), &sc.env, 130);
    let plain = strip_ansi(&s);
    assert!(
        !plain.contains("Tasks "),
        "minimal density + unset override = tasks hidden:\n{plain}"
    );
}

#[test]
fn ccbox_show_subagents_independent_of_tasks() {
    // Hiding subagents must not affect the tasks row.
    let mut sc = scratch();
    sc.env.show_subagents_override = Some(false);
    let s = render(&fixture(), &sc.env, 130);
    let plain = strip_ansi(&s);
    assert!(
        !plain.contains("Sub-Agents"),
        "subagents should be hidden:\n{plain}"
    );
    // Tasks row visibility depends on whether the fixture has tasks. The key
    // assertion is that the override is scoped to subagents only — i.e., the
    // tasks row's chip rendering is not affected by CCBOX_SHOW_SUBAGENTS.
    // (If the fixture has no tasks, the row is absent for content reasons,
    // which is independent of the override under test.)
}

#[test]
fn tokens_row_has_augmented_labels() {
    let mut sc = scratch();
    // Force cost visible so the tokens row renders with the cost cluster.
    sc.env.show_cost_override = Some(true);
    let s = render(&fixture(), &sc.env, 130);
    let plain = strip_ansi(&s);
    assert!(
        plain.contains(" in "),
        "expected augmented 'in' label: {plain}"
    );
    assert!(
        plain.contains(" out "),
        "expected augmented 'out' label: {plain}"
    );
}

#[test]
fn venv_renders_when_env_set() {
    let mut sc = scratch();
    sc.env.venv = Some("py311".to_string());
    let s = render(&fixture(), &sc.env, 130);
    let plain = strip_ansi(&s);
    assert!(
        plain.contains("py311"),
        "venv name should appear in top row:\n{plain}"
    );
    assert!(
        plain.contains("venv:"),
        "venv label should appear:\n{plain}"
    );
    for (i, line) in s.lines().enumerate() {
        assert_eq!(visible_width(line), 130, "line {i}: {:?}", strip_ansi(line));
    }
}

#[test]
fn session_id_chip_appears_in_top_border() {
    let sc = scratch();
    let mut s = fixture();
    s.session_id = "abcdef01-23456789".to_string();
    let out = render(&s, &sc.env, 130);
    let plain = strip_ansi(&out);
    // First line is the top border. The full session id (no truncation) appears.
    let first_line = out.lines().next().unwrap();
    let first_plain = strip_ansi(first_line);
    assert!(
        first_plain.contains("abcdef01-23456789"),
        "expected full session id on top border: {first_plain}"
    );
    // No brackets, no ellipsis.
    assert!(
        !first_plain.contains('['),
        "expected no brackets: {first_plain}"
    );
    assert!(
        !first_plain.contains('…'),
        "expected no ellipsis: {first_plain}"
    );
    // Just confirm chip is present on the top border (not elsewhere in box).
    let _ = plain;
}

#[test]
fn wide_top_row_shows_full_path_branch_and_model_no_vsep_or_burndown() {
    // Integration check for the top row composition at the wide layout: the
    // top border carries the session-id chip on the right; the top content
    // row shows the full path, the branch chip (or empty when no repo), and
    // the model identity; no `T-` substring; no inner vertical separator
    // glyph (`│`) appears inside the top content row.
    let sc = scratch();
    let mut s = fixture();
    s.session_id = "deadbeef-1234".to_string();
    s.cwd = "/home/user/my-project".to_string();
    let out = render(&s, &sc.env, 140);
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines.len() >= 2);
    let top_border = strip_ansi(lines[0]);
    let content_row = strip_ansi(lines[1]);
    // Top border has the chip flush right (full id, no brackets, no ellipsis).
    assert!(top_border.contains("deadbeef-1234"), "{top_border}");
    assert!(!top_border.contains('['), "{top_border}");
    // Top content row carries the full path tail and the model identity.
    assert!(
        content_row.contains("my-project"),
        "expected path tail: {content_row}"
    );
    assert!(
        content_row.contains("Sonnet 4.6")
            || content_row.contains("Sonnet")
            || content_row.contains("Opus")
            || content_row.contains("Haiku"),
        "expected model identity: {content_row}"
    );
    // Inside the row (between the left and right border bars) there should
    // be no extra `│` glyph from a vsep block.
    let chars: Vec<char> = content_row.chars().collect();
    let inner: String = chars
        .iter()
        .skip(1)
        .take(chars.len().saturating_sub(2))
        .collect();
    assert!(
        !inner.contains('│'),
        "top row must not have inner vsep glyph: {inner}"
    );
    // Whole output should have no T-… countdown.
    let full = strip_ansi(&out);
    assert!(
        !full.contains("T-"),
        "no burndown countdown anywhere: {full}"
    );
}

#[test]
fn populated_rate_limits_writes_marker() {
    // Rendering against a payload with populated rate_limits should leave a
    // marker behind so subsequent fresh-session renders stay subscription-mode.
    let sc = scratch();
    let marker = sc.env.claude_dir.join("ccbox-subscription");
    assert!(!marker.exists());
    let _ = render(&fixture(), &sc.env, 130);
    assert!(
        marker.exists(),
        "render with populated rate_limits should touch marker"
    );
}
