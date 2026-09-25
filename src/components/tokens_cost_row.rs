use crate::consts::{
    FIVE_HOUR_MINUTES, FIVE_HOUR_WARMUP_MINUTES, NARROW_WIDTH, SEVEN_DAY_MINUTES,
    SEVEN_DAY_WARMUP_MINUTES,
};
use crate::cost::burndown::burndown_delta;
use crate::data::{extra_usage, limit_history};
use crate::input::session::RateBucket;
use crate::layout::RowSpec;
use crate::render::sections::tokens_cost::{ExtraSpend, Trend, UsageLimit};

use super::component::{Component, ComponentOutput, SeparatorPolicy};
use super::context::ComponentContext;

pub struct TokensCostRow;
pub static TOKENS_COST_ROW: TokensCostRow = TokensCostRow;

fn usage_limit(
    label: &str,
    b: &RateBucket,
    window_minutes: u32,
    warmup_minutes: u32,
    now: f64,
) -> Option<UsageLimit> {
    if b.resets_at == 0 {
        return None;
    }
    let remaining = b.resets_at - now as i64;
    Some(UsageLimit {
        label: label.to_string(),
        used_pct: b.used_percentage,
        resets_in_secs: (remaining > 0).then_some(remaining),
        now,
        pace_delta: burndown_delta(
            b.used_percentage,
            b.resets_at,
            window_minutes,
            warmup_minutes,
            now,
        ),
        trend: (remaining > 0).then(|| {
            let end = b.resets_at as f64;
            Trend {
                start: end - window_minutes as f64 * 60.0,
                end,
                now,
                samples: vec![(now, b.used_percentage)],
                lookback: if window_minutes == FIVE_HOUR_MINUTES {
                    SESSION_LOOKBACK_SECS
                } else {
                    WEEKLY_LOOKBACK_SECS
                },
            }
        }),
        elapsed_frac: (remaining > 0).then(|| {
            let window = window_minutes as f64 * 60.0;
            (1.0 - remaining as f64 / window).clamp(0.0, 1.0)
        }),
    })
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

const SESSION_LOOKBACK_SECS: f64 = 30.0 * 60.0;
const WEEKLY_LOOKBACK_SECS: f64 = 24.0 * 3600.0;

/// `sampled_at` is when the reading was taken, which for cached account data
/// can be well before this render.
fn with_history(
    ctx: &ComponentContext,
    mut l: UsageLimit,
    series: &str,
    bucket: &RateBucket,
    min_gap_secs: f64,
    sampled_at: f64,
) -> UsageLimit {
    if let Some(trend) = l.trend.as_mut() {
        trend.samples = limit_history::record(
            &ctx.env.claude_dir,
            series,
            bucket,
            min_gap_secs,
            sampled_at,
        );
        if let Some(&(_, latest)) = trend.samples.last() {
            l.used_pct = l.used_pct.max(latest);
        }
    }
    l
}

pub fn usage_limits(ctx: &ComponentContext) -> Vec<UsageLimit> {
    let rl = &ctx.session.rate_limits;
    let session = usage_limit(
        "session",
        &rl.five_hour,
        FIVE_HOUR_MINUTES,
        FIVE_HOUR_WARMUP_MINUTES,
        ctx.now,
    )
    .map(|l| with_history(ctx, l, "five-hour", &rl.five_hour, 30.0, ctx.now));
    let account = ctx.data.account_usage(ctx);
    let model_limits = account
        .map(|u| u.model_limits.as_slice())
        .unwrap_or_default();
    let fetched_at = account.map_or(ctx.now, |u| u.fetched_at);
    [
        session,
        usage_limit(
            "week",
            &rl.seven_day,
            SEVEN_DAY_MINUTES,
            SEVEN_DAY_WARMUP_MINUTES,
            ctx.now,
        )
        .map(|l| with_history(ctx, l, "seven-day", &rl.seven_day, 300.0, ctx.now)),
    ]
    .into_iter()
    .flatten()
    .chain(model_limits.iter().filter_map(|m| {
        let bucket = RateBucket {
            used_percentage: m.used_pct,
            resets_at: m.resets_at,
        };
        let series = format!("model-{}", slug(&m.name));
        usage_limit(
            &m.name,
            &bucket,
            SEVEN_DAY_MINUTES,
            SEVEN_DAY_WARMUP_MINUTES,
            ctx.now,
        )
        .map(|l| with_history(ctx, l, &series, &bucket, 300.0, fetched_at))
    }))
    .collect()
}

fn extra_spend(ctx: &ComponentContext, limits: &[UsageLimit]) -> Option<ExtraSpend> {
    let estimate = extra_usage::update(
        &ctx.env.claude_dir,
        &ctx.session.session_id,
        &ctx.session.rate_limits,
        ctx.session.cost.total_cost_usd,
    );
    if !limits.iter().any(|l| l.used_pct >= 100.0) {
        return None;
    }
    match ctx
        .data
        .account_usage(ctx)
        .and_then(|u| u.extra_usage.as_ref())
    {
        Some(e) if e.used > 0.0 => Some(ExtraSpend::Actual {
            used: e.used,
            limit: e.limit,
        }),
        _ => estimate.map(ExtraSpend::Estimated),
    }
}

impl Component for TokensCostRow {
    fn id(&self) -> &'static str {
        "tokens-cost-row"
    }

    fn is_visible(&self, ctx: &ComponentContext) -> bool {
        ctx.width >= NARROW_WIDTH as i32
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let usage = *ctx.data.transcript_usage(ctx);
        let limits = usage_limits(ctx);
        let cost = ctx
            .data
            .show_cost(ctx)
            .then(|| (ctx.data.session_cost(ctx), ctx.data.day_cost(ctx)));
        let line = ctx.renderer.tokens_cost(
            usage.billed_in(),
            usage.output_tokens,
            cost,
            &limits,
            extra_spend(ctx, &limits),
            ctx.width,
        );

        ComponentOutput {
            rows: vec![RowSpec::content(line)],
            leading_separator: SeparatorPolicy::Dim,
            top_right_chip: String::new(),
            leading_left_chip: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn model_names_become_file_safe_series() {
        assert_eq!(slug("Fable"), "fable");
        assert_eq!(slug("Sonnet 4.5"), "sonnet-4-5");
        assert_eq!(slug("  Opus/4.7 (1M) "), "opus-4-7-1m");
    }
}
