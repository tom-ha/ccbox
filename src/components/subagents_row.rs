//! `SubagentsRow` — one row per running subagent (table layout).

use crate::config::resolve_row_visibility;
use crate::glyphs::{BOLD, RESET};
use crate::layout::RowSpec;
use crate::render::format::fmt_tok;
use crate::width::visible_width;

use super::component::{Component, ComponentOutput, SeparatorPolicy};
use super::context::ComponentContext;

pub struct SubagentsRow;
pub static SUBAGENTS_ROW: SubagentsRow = SubagentsRow;

impl Component for SubagentsRow {
    fn id(&self) -> &'static str {
        "subagents-row"
    }

    fn is_visible(&self, ctx: &ComponentContext) -> bool {
        let (override_val, _src) = resolve_row_visibility(
            ctx.env.toggles.show_subagents,
            ctx.env.show_subagents_override,
        );
        let allowed = override_val.unwrap_or_else(|| ctx.env.density.includes_subagents());
        allowed && !ctx.data.running_subagents(ctx).agents.is_empty()
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let r = ctx.renderer;
        let subagents = ctx.data.running_subagents(ctx);

        // Column widths pre-computed across every running subagent so the
        // table aligns regardless of how many agents are running.
        let type_w = subagents
            .agents
            .iter()
            .map(|s| {
                let t = if s.agent_type.is_empty() {
                    "?"
                } else {
                    s.agent_type.as_str()
                };
                visible_width(t)
            })
            .max()
            .unwrap_or(1);
        let out_w = subagents
            .agents
            .iter()
            .map(|s| visible_width(&fmt_tok(s.output)))
            .max()
            .unwrap_or(1);

        let mut rows: Vec<RowSpec> = Vec::with_capacity(subagents.agents.len() + 1);
        rows.push(RowSpec::content(
            r.subagent_header_row(ctx.width, type_w, out_w),
        ));
        for (idx, sub) in subagents.agents.iter().enumerate() {
            let row_text = r.subagent_row(sub, ctx.width, type_w, out_w, idx, ctx.now);
            rows.push(RowSpec::content(row_text));
        }

        let label = r.theme.label;
        let leading_left_chip = format!(" {label}{BOLD}Sub-Agents{RESET} ");

        ComponentOutput {
            rows,
            leading_separator: SeparatorPolicy::Strong,
            top_right_chip: String::new(),
            leading_left_chip,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::time::UNIX_EPOCH;

    use tempfile::tempdir;

    use super::*;
    use crate::ansi::strip_ansi;
    use crate::components::render_cache::RenderCache;
    use crate::config::Env;
    use crate::input::session::{SessionInfo, Workspace};
    use crate::render::Renderer;

    /// Seed a tempdir with one `.meta.json` + `.jsonl` pair under the
    /// claude-dir / project-slug / session-id / subagents layout the
    /// `RunningSubagents::from_session` loader expects. Returns `(env,
    /// session, now)` ready to plug into a `ComponentContext`.
    fn seed_subagent_fixture(meta: &str, jsonl: &str) -> (Env, SessionInfo, f64, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let project_dir = "/p";
        let session_id = "s";
        let slug: String = project_dir
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let subagents = dir
            .path()
            .join("projects")
            .join(&slug)
            .join(session_id)
            .join("subagents");
        std::fs::create_dir_all(&subagents).unwrap();
        std::fs::write(subagents.join("a.meta.json"), meta).unwrap();
        std::fs::File::create(subagents.join("a.jsonl"))
            .unwrap()
            .write_all(jsonl.as_bytes())
            .unwrap();
        let mtime = subagents
            .join("a.jsonl")
            .metadata()
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let env = Env {
            claude_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let session = SessionInfo {
            session_id: session_id.into(),
            workspace: Workspace {
                project_dir: project_dir.into(),
                ..Default::default()
            },
            ..Default::default()
        };
        (env, session, mtime + 1.0, dir)
    }

    #[test]
    fn render_emits_sub_agents_chip() {
        let (env, session, now, _guard) = seed_subagent_fixture(
            r#"{"agentType":"Explore","description":"look around"}"#,
            "{}",
        );
        let renderer = Renderer::default();
        let data = RenderCache::new();
        let ctx = ComponentContext::new(&session, &env, &renderer, 140, now, &data);
        let out = SUBAGENTS_ROW.render(&ctx);
        assert!(
            strip_ansi(&out.leading_left_chip).contains("Sub-Agents"),
            "leading_left_chip should contain 'Sub-Agents', got {:?}",
            out.leading_left_chip,
        );
        assert_eq!(out.leading_separator, SeparatorPolicy::Strong);
        assert!(!out.rows.is_empty(), "expected at least one row");
    }

    #[test]
    fn render_emits_header_row_above_agents() {
        let (env, session, now, _guard) = seed_subagent_fixture(
            r#"{"agentType":"Explore","description":"look around"}"#,
            "{}",
        );
        let renderer = Renderer::default();
        let data = RenderCache::new();
        let ctx = ComponentContext::new(&session, &env, &renderer, 140, now, &data);
        let out = SUBAGENTS_ROW.render(&ctx);
        // 1 header + 1 agent.
        assert_eq!(out.rows.len(), 2, "expected header + agent row");
        let header_text = strip_ansi(&out.rows[0].content);
        for label in ["type", "description", "in", "out", "dur"] {
            assert!(
                header_text.contains(label),
                "first row should be the column header containing {label:?}, got {header_text:?}",
            );
        }
    }
}

