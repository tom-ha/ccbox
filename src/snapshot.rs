//! `--snapshot` diagnostic: emit the parsed session, resolved env, layout pick,
//! per-component visibility, and the computed values feeding the visible rows
//! as a single JSON object. Strings are scrubbed of ANSI escapes before
//! serialization.

use serde_json::{json, Value};

use crate::ansi::strip_ansi;
use crate::components::{compose, ComponentContext, Composition, RenderCache};
use crate::config::{resolve_row_visibility, Density, Env, RowVisibilitySource, TasksView};
use crate::consts::{MEDIUM_WIDTH, MIN_WIDTH, NARROW_WIDTH};
use crate::input::session::SessionInfo;
use crate::layout::fill_ratio;
use crate::render::Renderer;
use crate::theme::Theme;

/// Build the snapshot JSON object for the given inputs. Performs the same
/// render-time computation as `render(...)` but emits structured data instead
/// of the ANSI box.
pub fn build_snapshot(session: &SessionInfo, env: &Env, width: i32, r: &Renderer) -> Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let comp = if width < NARROW_WIDTH as i32 {
        Composition::narrow()
    } else if width < MEDIUM_WIDTH as i32 {
        Composition::medium()
    } else {
        Composition::wide()
    };
    let layout_name = if width < NARROW_WIDTH as i32 {
        "narrow"
    } else if width < MEDIUM_WIDTH as i32 {
        "medium"
    } else {
        "wide"
    };

    let data = RenderCache::new();
    let ctx = ComponentContext::new(session, env, r, width, now, &data);
    // Drive compose() so the cache gets populated identically to a real render.
    let _spec = compose(&comp, &ctx);

    let body: Vec<Value> = comp
        .body
        .iter()
        .map(|c| json!({ "id": c.id(), "visible": c.is_visible(&ctx) }))
        .collect();

    let git = data.git_info(&ctx);
    let computed = json!({
        "model_name": strip_ansi(&session.model_name()),
        "short_pwd": strip_ansi(&session.short_pwd(&env.home)),
        "branch": {
            "name": git.branch,
            "ahead": git.ahead,
            "behind": git.behind,
            "dirty": git.modified + git.untracked + git.deleted + git.renamed,
        },
        "session_cost_usd": data.session_cost(&ctx),
        "day_cost_usd": data.day_cost(&ctx),
        "tokens_per_minute": data.token_rate(&ctx),
        "fill_ratio": fill_ratio(session),
    });

    json!({
        "session": session,
        "env": env_json(env),
        "theme": r.theme.name,
        "width": width,
        "layout": layout_name,
        "composition": { "body": body },
        "computed": computed,
    })
}

fn env_json(env: &Env) -> Value {
    json!({
        "claude_dir": env.claude_dir.display().to_string(),
        "home": env.home.display().to_string(),
        "max_width": env.max_width,
        "full_width": env.full_width,
        "show_cost_override": env.show_cost_override,
        "show_tasks_override": env.show_tasks_override,
        "show_subagents_override": env.show_subagents_override,
        "toggles": {
            "show_tasks": env.toggles.show_tasks,
            "show_subagents": env.toggles.show_subagents,
        },
        "row_visibility": row_visibility_json(env),
        "venv": env.venv,
        "density": density_str(env.density),
        "tasks_view": tasks_view_str(env.tasks_view),
        "git_cache_ttl_ms": env.git_cache_ttl_ms,
    })
}

/// The post-override decision for each gated row, alongside the precedence
/// layer that made the call (`state_file`, `env`, or `density`). `visible`
/// here is the *override* layer's verdict — it does NOT account for content
/// presence; the final boolean factoring content lives under
/// `composition.body[*].visible`.
fn row_visibility_json(env: &Env) -> Value {
    let (tasks_override, tasks_src) =
        resolve_row_visibility(env.toggles.show_tasks, env.show_tasks_override);
    let tasks_visible = tasks_override.unwrap_or_else(|| env.density.includes_tasks());
    let (sub_override, sub_src) =
        resolve_row_visibility(env.toggles.show_subagents, env.show_subagents_override);
    let sub_visible = sub_override.unwrap_or_else(|| env.density.includes_subagents());
    json!({
        "tasks":     { "visible": tasks_visible, "source": source_str(tasks_src) },
        "subagents": { "visible": sub_visible,   "source": source_str(sub_src)  },
    })
}

fn source_str(s: RowVisibilitySource) -> &'static str {
    match s {
        RowVisibilitySource::StateFile => "state_file",
        RowVisibilitySource::Env => "env",
        RowVisibilitySource::Density => "density",
    }
}

fn density_str(d: Density) -> &'static str {
    match d {
        Density::Minimal => "minimal",
        Density::Standard => "standard",
        Density::Verbose => "verbose",
    }
}

fn tasks_view_str(t: TasksView) -> &'static str {
    match t {
        TasksView::Inline => "inline",
        TasksView::Board => "board",
    }
}

/// Entry analogue of `render(...)`: build the snapshot, pretty-print, return.
/// Empty string when `width < MIN_WIDTH` (matching `render`'s gate).
pub fn render_snapshot(
    session: &SessionInfo,
    env: &Env,
    width: u16,
    theme: &'static Theme,
    bg_shift: crate::render::BgShift,
) -> String {
    if (width as i32) < MIN_WIDTH as i32 {
        return String::new();
    }
    let r = Renderer::new(theme, bg_shift);
    let v = build_snapshot(session, env, width as i32, &r);
    serde_json::to_string_pretty(&v).unwrap_or_default()
}
