//! Medium-width layout builder.

use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Local, TimeZone};

use crate::config::{should_show_cost, Env, TasksView};
use crate::cost::{day_cost as compute_day_cost, rates_for, session_cost as compute_session_cost, DayTotals, Usage};
use crate::data::git_info::GitInfo;
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

pub fn build_medium(session: &SessionInfo, env: &Env, width: i32, r: &Renderer) -> LayoutSpec {
    let fill = fill_ratio(session);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let model_name = session.model_name();
    let short_pwd = session.short_pwd(&env.home);
    let git = GitInfo::from_cwd(&session.cwd).unwrap_or_default();

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

    let line_context = r.context_line(&session.context_window, width - 3);
    let elapsed = session_elapsed(&session.transcript_path, Some(now));

    let (right_text, right_w) = r.model_right_section(
        &model_name,
        &session.model_thinking(),
        if session.thinking.enabled { &session.effort.level } else { "" },
        session.fast_mode,
    );
    let (venv_text, venv_w) = r.venv_section(env.venv.as_deref());
    let (branch_text, branch_w) = r.branch_chip(&git);

    let content_w = width - 3;
    let venv_w = venv_w as i32;
    let model_join = if venv_w > 0 { 5 } else { 0 };
    let path_branch_gap = if branch_w > 0 { 3 } else { 0 };
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

    let tasks = TaskList::from_session(&session.transcript_path);
    let subagents = RunningSubagents::from_session(&env.claude_dir, &session.session_id, &session.workspace.project_dir, now);

    let want_tasks = env.density.includes_tasks() && tasks.is_visible(now);
    let want_subagents = env.density.includes_subagents() && !subagents.agents.is_empty();

    let mut rows: Vec<RowSpec> = Vec::new();
    rows.push(RowSpec::new(RowKind::TopBorder));
    rows.push(RowSpec::content(top_row));
    rows.push(RowSpec::new(RowKind::SeparatorDim));
    rows.push(RowSpec::content(line_context));
    rows.push(RowSpec::new(RowKind::SeparatorDim));
    rows.push(RowSpec::content(tokens_cost.line));

    if want_tasks {
        rows.push(RowSpec::new(RowKind::SeparatorDim));
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
        rows.push(RowSpec::new(RowKind::SeparatorDim));
        for sub in &subagents.agents {
            let row_text = r.subagent_row(sub, width, 0, 0, now);
            for line in row_text.split('\n') {
                rows.push(RowSpec::content(line.to_string()));
            }
        }
    }

    rows.push(RowSpec::new(RowKind::BottomBorder));

    LayoutSpec { width, fill, top_right_chip, rows }
}
