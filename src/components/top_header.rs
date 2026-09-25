//! `TopHeader` — top border + path/branch/venv/model row(s) + session chip.

use crate::consts::NARROW_WIDTH;
use crate::layout::{session_elapsed, session_id_chip, RowKind, RowSpec};
use crate::width::{pad, visible_width};

use super::component::{Component, ComponentOutput, SeparatorPolicy};
use super::context::ComponentContext;

pub struct TopHeader;
pub static TOP_HEADER: TopHeader = TopHeader;

impl Component for TopHeader {
    fn id(&self) -> &'static str {
        "top-header"
    }

    fn is_visible(&self, _ctx: &ComponentContext) -> bool {
        true
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let session = ctx.session;
        let env = ctx.env;
        let r = ctx.renderer;
        let width = ctx.width;
        let now = ctx.now;

        let model_name = session.model_name();
        let short_pwd = session.short_pwd(&env.home);
        let git = ctx.data.git_info(ctx);

        let (right_text, right_w) = r.model_right_section(
            &model_name,
            &session.model_thinking(),
            if session.thinking.enabled {
                &session.effort.level
            } else {
                ""
            },
            session.fast_mode,
        );
        let (venv_text, venv_w) = r.venv_section(env.venv.as_deref());
        let (branch_text, branch_w) = r.branch_chip(git);

        let elapsed = session_elapsed(&session.transcript_path, Some(now));
        let name = ctx.data.session_name(ctx).unwrap_or_default();
        let top_right_chip = session_id_chip(r.theme, name, &session.session_id, &elapsed);

        let content_w = width - 3;

        let mut rows: Vec<RowSpec> = Vec::new();
        rows.push(RowSpec::new(RowKind::TopBorder));

        if width < NARROW_WIDTH as i32 {
            // Two-line narrow form: path+branch row, then venv·model row.
            let path_branch_gap = if branch_w > 0 { 3 } else { 0 };
            let path_budget = (content_w - branch_w as i32 - path_branch_gap - 1).max(8);
            let line_path = r.path_section(&short_pwd, path_budget);
            let branch_chunk = if branch_w > 0 {
                format!("   {branch_text}")
            } else {
                String::new()
            };
            let path_row_w = visible_width(&line_path) as i32 + visible_width(&branch_chunk) as i32;
            let path_row = format!(
                "{line_path}{branch_chunk}{}",
                " ".repeat(pad(content_w, path_row_w)),
            );

            let venv_w = venv_w as i32;
            let model_join_w = if venv_w > 0 { 5 } else { 0 };
            let model_chunk_w = venv_w + model_join_w + right_w as i32;
            let model_chunk = if venv_w > 0 {
                let label = r.theme.label;
                format!("{venv_text} {label}·\x1b[0m {right_text}")
            } else {
                right_text
            };
            let model_row_pad = pad(content_w, model_chunk_w);
            let model_row = format!("{}{model_chunk}", " ".repeat(model_row_pad));

            rows.push(RowSpec::content(path_row));
            rows.push(RowSpec::content(model_row));
        } else {
            // Single-line medium/wide form: path · branch · pad · venv · model.
            let venv_w = venv_w as i32;
            let model_join = if venv_w > 0 { 5 } else { 0 };
            let path_branch_gap = if branch_w > 0 { 3 } else { 0 };
            let path_budget = (content_w
                - branch_w as i32
                - path_branch_gap
                - venv_w
                - model_join
                - right_w as i32
                - 1)
            .max(10);

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

            rows.push(RowSpec::content(top_row));
        }

        ComponentOutput {
            rows,
            leading_separator: SeparatorPolicy::None,
            top_right_chip,
            leading_left_chip: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::render_cache::RenderCache;
    use crate::config::Env;
    use crate::input::session::SessionInfo;
    use crate::render::Renderer;

    fn out_for_width(width: i32) -> ComponentOutput {
        let session = SessionInfo {
            session_id: "abc-123".into(),
            ..Default::default()
        };
        let env = Env::default();
        let r = Renderer::default();
        let data = RenderCache::new();
        let ctx = ComponentContext::new(&session, &env, &r, width, 0.0, &data);
        TOP_HEADER.render(&ctx)
    }

    #[test]
    fn wide_emits_topborder_plus_one_content() {
        let out = out_for_width(140);
        assert_eq!(out.rows.len(), 2);
        assert_eq!(out.rows[0].kind, RowKind::TopBorder);
        assert_eq!(out.rows[1].kind, RowKind::Content);
    }

    #[test]
    fn narrow_emits_topborder_plus_two_content() {
        let out = out_for_width(40);
        assert_eq!(out.rows.len(), 3);
        assert_eq!(out.rows[0].kind, RowKind::TopBorder);
        assert_eq!(out.rows[1].kind, RowKind::Content);
        assert_eq!(out.rows[2].kind, RowKind::Content);
    }

    #[test]
    fn chip_text_non_empty_when_session_id_present() {
        let out = out_for_width(140);
        assert!(!out.top_right_chip.is_empty());
    }
}
