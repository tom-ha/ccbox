//! `SubagentsRow` — one or more rows per running subagent.

use crate::consts::MEDIUM_WIDTH;
use crate::layout::RowSpec;

use super::component::{Component, ComponentOutput, SeparatorPolicy};
use super::context::ComponentContext;

pub struct SubagentsRow;
pub static SUBAGENTS_ROW: SubagentsRow = SubagentsRow;

impl Component for SubagentsRow {
    fn id(&self) -> &'static str {
        "subagents-row"
    }

    fn is_visible(&self, ctx: &ComponentContext) -> bool {
        ctx.env.density.includes_subagents() && !ctx.data.running_subagents(ctx).agents.is_empty()
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let r = ctx.renderer;
        let subagents = ctx.data.running_subagents(ctx);
        // `session_inout` is only meaningful in the wide layout, which uses
        // it to size the subagent's share of the box. Narrow/medium passed
        // `0` historically; preserve that by gating on width.
        let session_inout: i64 = if ctx.width >= MEDIUM_WIDTH as i32 {
            let usage = *ctx.data.transcript_usage(ctx);
            (usage.billed_in() + usage.cache_read_input_tokens + usage.output_tokens) as i64
                + subagents
                    .agents
                    .iter()
                    .map(|s| (s.total_input + s.output) as i64)
                    .sum::<i64>()
        } else {
            0
        };

        let mut rows: Vec<RowSpec> = Vec::new();
        for sub in &subagents.agents {
            let row_text = r.subagent_row(sub, ctx.width, session_inout, 0, ctx.now);
            for line in row_text.split('\n') {
                rows.push(RowSpec::content(line.to_string()));
            }
        }

        ComponentOutput {
            rows,
            leading_separator: SeparatorPolicy::Dim,
            top_right_chip: String::new(),
        }
    }
}
