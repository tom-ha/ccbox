//! `PluginsSkillsRow` — the plugins/skills summary line. Verbose density only.

use crate::layout::RowSpec;

use super::component::{Component, ComponentOutput, SeparatorPolicy};
use super::context::ComponentContext;

pub struct PluginsSkillsRow;
pub static PLUGINS_SKILLS_ROW: PluginsSkillsRow = PluginsSkillsRow;

fn render_line(ctx: &ComponentContext) -> String {
    let r = ctx.renderer;
    let skills = ctx.data.loaded_skills(ctx);
    let skill_display: Vec<String> = skills
        .names
        .iter()
        .map(|s| s.rsplit(':').next().unwrap_or("").to_string())
        .collect();
    let plugins = ctx.session.workspace.plugins(&ctx.env.claude_dir);
    r.plugins_skills(
        skills.names.len() as u32,
        &skill_display.join(","),
        &plugins,
        0,
    )
}

impl Component for PluginsSkillsRow {
    fn id(&self) -> &'static str {
        "plugins-skills-row"
    }

    fn is_visible(&self, ctx: &ComponentContext) -> bool {
        if !ctx.env.density.includes_plugins_skills() {
            return false;
        }
        !render_line(ctx).is_empty()
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        ComponentOutput {
            rows: vec![RowSpec::content(render_line(ctx))],
            leading_separator: SeparatorPolicy::Dim,
            top_right_chip: String::new(),
            leading_left_chip: String::new(),
        }
    }
}
