//! Wide-width layout builder.

use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Local, TimeZone};

use crate::config::{should_show_cost, Env, TasksView};
use crate::cost::{day_cost as compute_day_cost, rates_for, session_cost as compute_session_cost, DayTotals, Usage};
use crate::data::git_info::GitInfo;
use crate::data::loaded_skills::LoadedSkills;
use crate::data::openspec::OpenSpec;
use crate::data::running_subagents::RunningSubagents;
use crate::data::subscription_marker;
use crate::data::task_list::TaskList;
use crate::data::token_log::TokenLog;
use crate::data::token_rate::{TokenRate, WINDOW};
use crate::data::transcript_usage::TranscriptUsage;
use crate::input::session::SessionInfo;
use crate::render::sections::tokens_cost::{sparkline_bar_w, TokensCostExtras};
use crate::render::Renderer;
use crate::width::visible_width;

use super::{fill_ratio, session_elapsed, session_id_chip, LayoutSpec, RowKind, RowSpec};

pub fn build_wide(session: &SessionInfo, env: &Env, width: i32, r: &Renderer) -> LayoutSpec {
    let fill = fill_ratio(session);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let model_name = session.model_name();
    let short_pwd = session.short_pwd(&env.home);
    let git = GitInfo::from_cwd(&session.cwd).unwrap_or_default();

    let skills = LoadedSkills::from_transcript(&session.transcript_path);
    let skill_display: Vec<String> = skills.names.iter().map(|s| s.rsplit(':').next().unwrap_or("").to_string()).collect();
    let usage = TranscriptUsage::from_transcript(&session.transcript_path);

    let today = Local
        .timestamp_opt(now as i64, 0)
        .single()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".to_string());

    let token_log = TokenLog::update(
        &env.claude_dir,
        &session.session_id,
        &today,
        usage.billed_in(),
        usage.cache_read_input_tokens,
        usage.output_tokens,
    );
    let tok_rate = TokenRate::update(
        &env.claude_dir,
        &session.session_id,
        usage.billed_in(),
        usage.output_tokens,
        now,
    );

    let rates = rates_for(&model_name);
    let cost_usage = Usage {
        input_tokens: usage.input_tokens,
        cache_creation_input_tokens: usage.cache_creation_input_tokens,
        cache_read_input_tokens: usage.cache_read_input_tokens,
        output_tokens: usage.output_tokens,
    };
    let day_totals = DayTotals {
        day_in: token_log.day_in,
        day_cache_read: token_log.day_cache_read,
        day_out: token_log.day_out,
    };
    let sess_cost = compute_session_cost(rates, &cost_usage);
    let day_cost = compute_day_cost(rates, &day_totals);

    let subagents = RunningSubagents::from_session(&env.claude_dir, &session.session_id, &session.workspace.project_dir, now);
    let session_inout: i64 = (usage.billed_in() + usage.cache_read_input_tokens + usage.output_tokens) as i64
            + subagents.agents.iter().map(|s| (s.total_input + s.output) as i64).sum::<i64>();

    let tasks = TaskList::from_session(&session.transcript_path);
    let elapsed = session_elapsed(&session.transcript_path, Some(now));

    let (right_text, right_w) = r.model_right_section(
        &model_name,
        &session.model_thinking(),
        if session.thinking.enabled { &session.effort.level } else { "" },
        session.fast_mode,
    );
    let (venv_text, venv_w) = r.venv_section(env.venv.as_deref());
    let (branch_text, branch_w) = r.branch_chip(&git);

    if session.rate_limits.five_hour.resets_at != 0 {
        subscription_marker::touch(&env.claude_dir);
    }
    let marker_exists = subscription_marker::exists(&env.claude_dir);
    let show_cost = should_show_cost(&session.rate_limits, env.show_cost_override, marker_exists);

    let (in_active, out_active) = TokenRate::recently_active(&env.claude_dir, &session.session_id, now, 10.0);
    let bar_w = sparkline_bar_w(width, sess_cost, day_cost, tok_rate, show_cost).max(0) as usize;
    let spark_history = TokenRate::history(&env.claude_dir, &session.session_id, bar_w, WINDOW * 2.0, now);

    let tokens_cost = r.tokens_cost(
        usage.billed_in(), usage.output_tokens,
        sess_cost, day_cost, tok_rate,
        &TokensCostExtras { spark_history, in_active, out_active, show_cost },
        width,
    );

    let plugins_line = r.plugins_skills(
        skills.names.len() as u32,
        &skill_display.join(","),
        &session.workspace.plugins(&env.claude_dir),
        0,
    );

    let changes = OpenSpec::from_cwd(&session.cwd).changes;
    let title_cap = (width - 45).max(10);
    let max_name = changes.iter().map(|(n, _, _)| n.chars().count() as i32).max().unwrap_or(25);
    let title_w = 40.min(title_cap).min(max_name);
    let openspec_bars: Vec<String> = changes
        .iter()
        .enumerate()
        .map(|(i, (name, d, t))| r.openspec_bar(name, *d as i32, *t as i32, width, title_w, i))
        .collect();

    let line_context = r.context_line(&session.context_window, width - 3);

    // Top content row: path · branch · pad · venv · model
    let content_w = width - 3;
    let venv_w = venv_w as i32;
    let model_join = if venv_w > 0 { 5 } else { 0 }; // " · " with theme codes wrapping the dot
    let path_branch_gap = if branch_w > 0 { 3 } else { 0 }; // "   "
    let path_budget = (content_w
        - branch_w as i32 - path_branch_gap
        - venv_w - model_join - right_w as i32
        - 1).max(10);

    let line_path = r.path_section(&short_pwd, path_budget);
    let path_w = visible_width(&line_path) as i32;

    let branch_chunk = if branch_w > 0 {
        format!("   {branch_text}")
    } else {
        String::new()
    };
    let venv_model = if venv_w > 0 {
        let label = r.theme.label;
        format!("{venv_text} {label}·\x1b[0m {right_text}")
    } else {
        right_text
    };

    let row_left_w = path_w + visible_width(&branch_chunk) as i32;
    let row_right_w = visible_width(&venv_model) as i32;
    let pad = (content_w - row_left_w - row_right_w).max(1) as usize;
    let top_row = format!("{line_path}{branch_chunk}{}{venv_model}", " ".repeat(pad));

    let top_right_chip = session_id_chip(r.theme, &session.session_id, &elapsed);

    let mut rows: Vec<RowSpec> = Vec::new();
    rows.push(RowSpec::new(RowKind::TopBorder));
    rows.push(RowSpec::content(top_row));

    rows.push(RowSpec::new(RowKind::SeparatorDim));

    rows.push(RowSpec::content(line_context));

    // No vertical seams threading through the tokens-cost row anymore; the
    // sparkline-midpoint marker stays out of the surrounding separators.
    rows.push(RowSpec::new(RowKind::SeparatorDim));
    rows.push(RowSpec::content(tokens_cost.line));
    let _ = tokens_cost.mark_col;

    // Density-gated event-driven rows.
    let want_tasks = env.density.includes_tasks() && tasks.is_visible(now);
    let want_subagents = env.density.includes_subagents() && !subagents.agents.is_empty();
    let want_openspec = env.density.includes_openspec() && !openspec_bars.is_empty();
    let want_plugins = env.density.includes_plugins_skills() && !plugins_line.is_empty();

    let mut any_seam = false;
    let mut next_ups: Vec<i32> = vec![];

    let push_sep = |rows: &mut Vec<RowSpec>, ups: &mut Vec<i32>, any_seam: &mut bool, dim: bool| {
        let mut s = if *any_seam {
            *any_seam = false;
            RowSpec::new(RowKind::SeparatorSeam)
        } else if dim {
            RowSpec::new(RowKind::SeparatorDim)
        } else {
            RowSpec::new(RowKind::Separator)
        };
        s.ups = std::mem::take(ups);
        rows.push(s);
    };

    if want_plugins {
        push_sep(&mut rows, &mut next_ups, &mut any_seam, true);
        rows.push(RowSpec::content(plugins_line));
    }

    if want_tasks {
        push_sep(&mut rows, &mut next_ups, &mut any_seam, true);
        match env.tasks_view {
            TasksView::Board => {
                for line in r.task_board(&tasks, width) {
                    rows.push(RowSpec::content(line));
                }
            }
            TasksView::Inline => {
                rows.push(RowSpec::content(r.task_row(&tasks, width, 0)));
            }
        }
    }

    if want_subagents {
        push_sep(&mut rows, &mut next_ups, &mut any_seam, true);
        for sub in &subagents.agents {
            let row_text = r.subagent_row(sub, width, session_inout, 0, now);
            for line in row_text.split('\n') {
                rows.push(RowSpec::content(line.to_string()));
            }
        }
    }

    if want_openspec {
        push_sep(&mut rows, &mut next_ups, &mut any_seam, false);
        for bar in &openspec_bars {
            rows.push(RowSpec::content(bar.clone()));
        }
        rows.push(RowSpec::new(RowKind::BottomBorder));
    } else {
        let mut b = RowSpec::new(RowKind::BottomBorder);
        b.ups = std::mem::take(&mut next_ups);
        rows.push(b);
    }

    LayoutSpec { width, fill, top_right_chip, rows }
}
