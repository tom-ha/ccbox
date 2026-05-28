//! Process-level environment: paths, width overrides, and density preset.

use std::path::PathBuf;

use crate::input::session::RateLimits;
use crate::input::toggles::Toggles;

/// Three-state density preset that gates which event-driven rows participate.
///
/// `Minimal` shows only the always-on rows (top row + context row + tokens/cost
/// row). `Standard` adds tasks, subagents, and openspec when present. `Verbose`
/// adds plugins/skills when present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Density {
    Minimal,
    Standard,
    Verbose,
}

impl Default for Density {
    fn default() -> Self {
        Self::Standard
    }
}

impl Density {
    pub fn includes_tasks(self) -> bool {
        matches!(self, Self::Standard | Self::Verbose)
    }
    pub fn includes_subagents(self) -> bool {
        matches!(self, Self::Standard | Self::Verbose)
    }
    pub fn includes_openspec(self) -> bool {
        matches!(self, Self::Standard | Self::Verbose)
    }
    pub fn includes_plugins_skills(self) -> bool {
        matches!(self, Self::Verbose)
    }
}

/// Inline kanban (single line) vs board kanban (multi-line grid).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TasksView {
    Inline,
    Board,
}

impl Default for TasksView {
    fn default() -> Self {
        Self::Board
    }
}

/// Which precedence layer decided a row's visibility. Used by [`resolve_row_visibility`]
/// and surfaced in the `--snapshot` output so users can debug "why isn't this
/// row rendering?" without reading source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowVisibilitySource {
    StateFile,
    Env,
    Density,
}

#[derive(Debug, Clone, Default)]
pub struct Env {
    pub claude_dir: PathBuf,
    pub home: PathBuf,
    pub max_width: Option<u16>,
    pub full_width: bool,
    /// `Some(true)` forces the cost columns visible, `Some(false)` forces them
    /// hidden, `None` lets [`should_show_cost`] auto-decide from rate-limit data.
    /// Populated from `CCBOX_SHOW_COST` at startup; see [`parse_bool_tristate`].
    pub show_cost_override: Option<bool>,
    /// `Some(true)` forces the tasks row visible (even under `Density::Minimal`),
    /// `Some(false)` forces it hidden (even under `Density::Standard`/`Verbose`),
    /// `None` falls through to the state file then the density preset.
    /// Populated from `CCBOX_SHOW_TASKS` at startup.
    pub show_tasks_override: Option<bool>,
    /// Same shape as [`Env::show_tasks_override`], for the subagents row.
    /// Populated from `CCBOX_SHOW_SUBAGENTS` at startup.
    pub show_subagents_override: Option<bool>,
    /// Per-row toggles loaded from `<claude_dir>/ccbox-toggles.json`. Takes
    /// precedence over the env-var overrides above; see [`resolve_row_visibility`].
    pub toggles: Toggles,
    /// Short name of the active Python virtual environment, or `None`. Sourced
    /// from `VIRTUAL_ENV_PROMPT` (when set) or the basename of `VIRTUAL_ENV`.
    pub venv: Option<String>,
    /// Which event-driven rows participate in the layout.
    pub density: Density,
    /// Board (default) or inline kanban for the task row.
    pub tasks_view: TasksView,
    /// TTL in milliseconds for the on-disk `GitInfo` cache (`0` disables).
    /// Populated from `CCBOX_GIT_CACHE_TTL_MS`; default `2000`.
    pub git_cache_ttl_ms: u64,
}

/// Resolve a row's effective override using the precedence chain
/// state file → env var → density preset. Returns `(override, source)`.
///
/// `override = Some(true|false)` means the visibility is forced; `None` means
/// fall back to the density preset and let the component's content check apply.
pub fn resolve_row_visibility(
    state: Option<bool>,
    env_var: Option<bool>,
) -> (Option<bool>, RowVisibilitySource) {
    if let Some(v) = state {
        (Some(v), RowVisibilitySource::StateFile)
    } else if let Some(v) = env_var {
        (Some(v), RowVisibilitySource::Env)
    } else {
        (None, RowVisibilitySource::Density)
    }
}

/// Parse a tri-state boolean env-var value.
///
/// Truthy spellings (`1`, `true`, `yes`, `on`, case-insensitive) → `Some(true)`.
/// Falsy spellings (`0`, `false`, `no`, `off`, case-insensitive) → `Some(false)`.
/// Anything else (including `None` and empty string) → `None`, meaning
/// "fall back to the default for this knob."
pub fn parse_bool_tristate(value: Option<&str>) -> Option<bool> {
    let raw = value?.trim();
    if raw.is_empty() {
        return None;
    }
    match raw.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// Parse `CCBOX_DENSITY` into a [`Density`]. Case-insensitive; unknown values
/// fall back to [`Density::Standard`].
pub fn parse_density(value: &str) -> Option<Density> {
    match value.trim().to_ascii_lowercase().as_str() {
        "minimal" => Some(Density::Minimal),
        "standard" => Some(Density::Standard),
        "verbose" => Some(Density::Verbose),
        _ => None,
    }
}

/// Parse `CCBOX_TASKS` into a [`TasksView`]. Case-insensitive; unknown values
/// fall back to [`TasksView::Board`].
pub fn parse_tasks_view(value: &str) -> Option<TasksView> {
    match value.trim().to_ascii_lowercase().as_str() {
        "inline" => Some(TasksView::Inline),
        "board" => Some(TasksView::Board),
        _ => None,
    }
}

/// Decide whether the tokens/cost block should render its cost cell. The
/// override (from `CCBOX_SHOW_COST`) wins; otherwise we treat either a
/// populated `rate_limits.five_hour.resets_at` OR a persistent subscription
/// marker (`<claude_dir>/ccbox-subscription`) as the "subscription user"
/// signal and hide cost.
pub fn should_show_cost(
    rate_limits: &RateLimits,
    env_override: Option<bool>,
    marker_exists: bool,
) -> bool {
    env_override.unwrap_or(rate_limits.five_hour.resets_at == 0 && !marker_exists)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bool_tristate_truthy_spellings() {
        for s in ["1", "true", "TRUE", "True", "yes", "YES", "on", "ON"] {
            assert_eq!(parse_bool_tristate(Some(s)), Some(true), "{s}");
        }
    }

    #[test]
    fn parse_bool_tristate_falsy_spellings() {
        for s in ["0", "false", "FALSE", "False", "no", "NO", "off", "OFF"] {
            assert_eq!(parse_bool_tristate(Some(s)), Some(false), "{s}");
        }
    }

    #[test]
    fn parse_bool_tristate_unrecognized_returns_none() {
        for s in ["", "  ", "maybe", "2", "tru", "ye s"] {
            assert_eq!(parse_bool_tristate(Some(s)), None, "{s:?}");
        }
        assert_eq!(parse_bool_tristate(None), None);
    }

    #[test]
    fn parse_bool_tristate_trims_whitespace() {
        assert_eq!(parse_bool_tristate(Some("  true  ")), Some(true));
        assert_eq!(parse_bool_tristate(Some("\tfalse\n")), Some(false));
    }

    #[test]
    fn density_default_is_standard() {
        assert_eq!(Density::default(), Density::Standard);
    }

    #[test]
    fn density_minimal_excludes_everything() {
        let d = Density::Minimal;
        assert!(!d.includes_tasks());
        assert!(!d.includes_subagents());
        assert!(!d.includes_openspec());
        assert!(!d.includes_plugins_skills());
    }

    #[test]
    fn density_standard_includes_tasks_subagents_openspec_only() {
        let d = Density::Standard;
        assert!(d.includes_tasks());
        assert!(d.includes_subagents());
        assert!(d.includes_openspec());
        assert!(!d.includes_plugins_skills());
    }

    #[test]
    fn density_verbose_includes_everything() {
        let d = Density::Verbose;
        assert!(d.includes_tasks());
        assert!(d.includes_subagents());
        assert!(d.includes_openspec());
        assert!(d.includes_plugins_skills());
    }

    #[test]
    fn parse_density_known_values() {
        assert_eq!(parse_density("minimal"), Some(Density::Minimal));
        assert_eq!(parse_density("standard"), Some(Density::Standard));
        assert_eq!(parse_density("verbose"), Some(Density::Verbose));
    }

    #[test]
    fn parse_density_is_case_insensitive() {
        assert_eq!(parse_density("MINIMAL"), Some(Density::Minimal));
        assert_eq!(parse_density("Verbose"), Some(Density::Verbose));
    }

    #[test]
    fn parse_density_unknown_returns_none() {
        assert_eq!(parse_density(""), None);
        assert_eq!(parse_density("loud"), None);
        assert_eq!(parse_density("medium"), None);
    }

    #[test]
    fn tasks_view_default_is_board() {
        assert_eq!(TasksView::default(), TasksView::Board);
    }

    #[test]
    fn parse_tasks_view_known_values() {
        assert_eq!(parse_tasks_view("inline"), Some(TasksView::Inline));
        assert_eq!(parse_tasks_view("board"), Some(TasksView::Board));
    }

    #[test]
    fn parse_tasks_view_unknown_returns_none() {
        assert_eq!(parse_tasks_view("grid"), None);
        assert_eq!(parse_tasks_view(""), None);
    }

    fn rl(resets_at: i64) -> RateLimits {
        let mut r = RateLimits::default();
        r.five_hour.resets_at = resets_at;
        r
    }

    #[test]
    fn should_show_cost_subscription_auto_hides() {
        assert!(!should_show_cost(&rl(1_777_000_000), None, false));
    }

    #[test]
    fn should_show_cost_non_subscription_auto_shows() {
        assert!(should_show_cost(&rl(0), None, false));
    }

    #[test]
    fn should_show_cost_env_true_forces_show_on_subscription() {
        assert!(should_show_cost(&rl(1_777_000_000), Some(true), false));
    }

    #[test]
    fn should_show_cost_env_false_forces_hide_on_non_subscription() {
        assert!(!should_show_cost(&rl(0), Some(false), false));
    }

    #[test]
    fn should_show_cost_marker_alone_hides_cost() {
        assert!(!should_show_cost(&rl(0), None, true));
    }

    #[test]
    fn should_show_cost_marker_overridden_by_env_true() {
        assert!(should_show_cost(&rl(0), Some(true), true));
    }

    #[test]
    fn should_show_cost_marker_and_rate_limits_both_hide() {
        assert!(!should_show_cost(&rl(1_777_000_000), None, true));
    }

    #[test]
    fn should_show_cost_env_false_beats_marker() {
        assert!(!should_show_cost(&rl(0), Some(false), true));
    }

    #[test]
    fn resolve_row_visibility_state_file_wins() {
        assert_eq!(
            resolve_row_visibility(Some(false), Some(true)),
            (Some(false), RowVisibilitySource::StateFile),
        );
        assert_eq!(
            resolve_row_visibility(Some(true), None),
            (Some(true), RowVisibilitySource::StateFile),
        );
    }

    #[test]
    fn resolve_row_visibility_env_when_no_state() {
        assert_eq!(
            resolve_row_visibility(None, Some(false)),
            (Some(false), RowVisibilitySource::Env),
        );
        assert_eq!(
            resolve_row_visibility(None, Some(true)),
            (Some(true), RowVisibilitySource::Env),
        );
    }

    #[test]
    fn resolve_row_visibility_density_fallthrough() {
        assert_eq!(
            resolve_row_visibility(None, None),
            (None, RowVisibilitySource::Density),
        );
    }

    #[test]
    fn parse_bool_tristate_covers_new_env_vars() {
        // The same parser backs CCBOX_SHOW_TASKS and CCBOX_SHOW_SUBAGENTS;
        // exhaust the value space once more to lock in coverage of the new vars.
        assert_eq!(parse_bool_tristate(Some("1")), Some(true));
        assert_eq!(parse_bool_tristate(Some("0")), Some(false));
        assert_eq!(parse_bool_tristate(Some("yes")), Some(true));
        assert_eq!(parse_bool_tristate(Some("no")), Some(false));
        assert_eq!(parse_bool_tristate(Some("maybe")), None);
        assert_eq!(parse_bool_tristate(None), None);
    }
}
