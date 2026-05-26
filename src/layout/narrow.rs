//! Narrow-width layout builder. Two-line content form:
//!   line 1: path (head-ellipsis) + branch chip
//!   line 2: venv · model

use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Env;
use crate::data::git_info::GitInfo;
use crate::data::running_subagents::RunningSubagents;
use crate::input::session::SessionInfo;
use crate::render::Renderer;
use crate::width::visible_width;

use super::{fill_ratio, session_elapsed, session_id_chip, LayoutSpec, RowKind, RowSpec};

pub fn build_narrow(session: &SessionInfo, env: &Env, width: i32, r: &Renderer) -> LayoutSpec {
    let fill = fill_ratio(session);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let model_name = session.model_name();
    let short_pwd = session.short_pwd(&env.home);
    let git = GitInfo::from_cwd(&session.cwd).unwrap_or_default();

    let (right_text, right_w) = r.model_right_section(
        &model_name,
        &session.model_thinking(),
        if session.thinking.enabled { &session.effort.level } else { "" },
        session.fast_mode,
    );
    let (venv_text, venv_w) = r.venv_section(env.venv.as_deref());
    let (branch_text, branch_w) = r.branch_chip(&git);

    let line_context = r.context_line_compact(&session.context_window, width - 3);

    let subagents = RunningSubagents::from_session(&env.claude_dir, &session.session_id, &session.workspace.project_dir, now);
    let elapsed = session_elapsed(&session.transcript_path, Some(now));
    let top_right_chip = session_id_chip(r.theme, &session.session_id, &elapsed);

    let content_w = width - 3;
    // Line 1: path + branch (head-ellipsis path takes whatever's left)
    let path_branch_gap = if branch_w > 0 { 3 } else { 0 };
    let path_budget = (content_w - branch_w as i32 - path_branch_gap - 1).max(8);
    let line_path = r.path_section(&short_pwd, path_budget);
    let branch_chunk = if branch_w > 0 {
        format!("   {branch_text}")
    } else {
        String::new()
    };
    let path_row_w = visible_width(&line_path) as i32 + visible_width(&branch_chunk) as i32;
    let path_row = format!("{line_path}{branch_chunk}{}", " ".repeat((content_w - path_row_w).max(0) as usize));

    // Line 2: venv · model (right-anchored)
    let venv_w = venv_w as i32;
    let model_join_w = if venv_w > 0 { 5 } else { 0 };
    let model_chunk_w = venv_w + model_join_w + right_w as i32;
    let model_chunk = if venv_w > 0 {
        let label = r.theme.label;
        format!("{venv_text} {label}·\x1b[0m {right_text}")
    } else {
        right_text
    };
    let model_row_pad = (content_w - model_chunk_w).max(0) as usize;
    let model_row = format!("{}{model_chunk}", " ".repeat(model_row_pad));

    let want_subagents = env.density.includes_subagents() && !subagents.agents.is_empty();

    let mut rows: Vec<RowSpec> = Vec::new();
    rows.push(RowSpec::new(RowKind::TopBorder));
    rows.push(RowSpec::content(path_row));
    rows.push(RowSpec::content(model_row));
    rows.push(RowSpec::new(RowKind::SeparatorDim));

    if want_subagents {
        for sub in &subagents.agents {
            let row_text = r.subagent_row(sub, width, 0, 0, now);
            for line in row_text.split('\n') {
                rows.push(RowSpec::content(line.to_string()));
            }
        }
        rows.push(RowSpec::new(RowKind::SeparatorDim));
    }

    rows.push(RowSpec::content(line_context));
    rows.push(RowSpec::new(RowKind::BottomBorder));

    LayoutSpec { width, fill, top_right_chip, rows }
}
