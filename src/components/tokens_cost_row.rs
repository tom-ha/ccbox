//! `TokensCostRow` — single-line tokens/cost/rate/sparkline row.

use crate::consts::NARROW_WIDTH;
use crate::layout::RowSpec;
use crate::render::sections::tokens_cost::{sparkline_bar_w, TokensCostExtras};

use super::component::{Component, ComponentOutput, SeparatorPolicy};
use super::context::ComponentContext;

pub struct TokensCostRow;
pub static TOKENS_COST_ROW: TokensCostRow = TokensCostRow;

impl Component for TokensCostRow {
    fn id(&self) -> &'static str {
        "tokens-cost-row"
    }

    fn is_visible(&self, ctx: &ComponentContext) -> bool {
        // Narrow layouts don't include the tokens-cost row today.
        ctx.width >= NARROW_WIDTH as i32
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let r = ctx.renderer;
        let usage = *ctx.data.transcript_usage(ctx);
        let tok_rate = ctx.data.token_rate(ctx);
        let sess_cost = ctx.data.session_cost(ctx);
        let day_cost = ctx.data.day_cost(ctx);
        let (in_active, out_active) = ctx.data.tokens_recently_active(ctx);
        let show_cost = ctx.data.show_cost(ctx);
        let bar_w =
            sparkline_bar_w(ctx.width, sess_cost, day_cost, tok_rate, show_cost).max(0) as usize;
        let spark_history = ctx.data.token_rate_history(ctx, bar_w);

        let tokens_cost = r.tokens_cost(
            usage.billed_in(),
            usage.output_tokens,
            sess_cost,
            day_cost,
            tok_rate,
            &TokensCostExtras {
                spark_history,
                in_active,
                out_active,
                show_cost,
            },
            ctx.width,
        );

        ComponentOutput {
            rows: vec![RowSpec::content(tokens_cost.line)],
            leading_separator: SeparatorPolicy::Dim,
            top_right_chip: String::new(),
            leading_left_chip: String::new(),
        }
    }
}
