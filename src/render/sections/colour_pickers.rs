//! Three-step colour pickers: `fill_colour`, `risk_zone_color`, `day_cost_colour`.

use crate::render::Renderer;

impl Renderer {
    /// Fill colour by percentage: < 70 safe, 70–89 warn, ≥ 90 alert.
    pub fn fill_colour(&self, pct: f64) -> &'static str {
        if pct >= 90.0 { self.theme.alert }
        else if pct >= 70.0 { self.theme.warn }
        else { self.theme.safe }
    }

    /// Context-window risk colour by absolute token count. Used by callers
    /// (e.g. subagent rows) that don't have a per-session effective limit;
    /// thresholds are tuned for the legacy 200K/150K context window.
    pub fn risk_zone_color(&self, tokens: u64) -> &'static str {
        if tokens <= 50_000 { self.theme.safe }
        else if tokens <= 80_000 { self.theme.yellow }
        else if tokens <= 150_000 { self.theme.warn }
        else { self.theme.alert }
    }

    /// Context-window risk colour by `tokens / effective_limit` ratio. Same
    /// bucket structure as [`risk_zone_color`] but scales with the session's
    /// reported context window: 50K/150K ≈ 0.333, 80K/150K ≈ 0.533, 150K/150K
    /// = 1.0. Used by the context line so 1M-context users get colour shifts
    /// proportional to their actual auto-compact threshold.
    pub fn risk_zone_color_for_ratio(&self, ratio: f64) -> &'static str {
        if ratio <= 1.0 / 3.0 { self.theme.safe }
        else if ratio <= 8.0 / 15.0 { self.theme.yellow }
        else if ratio <= 1.0 { self.theme.warn }
        else { self.theme.alert }
    }

    /// Day-cost colour: < $25 safe, $25-$50 yellow, > $50 alert.
    pub fn day_cost_colour(&self, cost: f64) -> &'static str {
        if cost > 50.0 { self.theme.alert }
        else if cost >= 25.0 { self.theme.yellow }
        else { self.theme.safe }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::builtin::CLAUDE_DARK;

    fn r() -> Renderer { Renderer::default() }

    #[test]
    fn fill_colour_thresholds() {
        assert_eq!(r().fill_colour(0.0), CLAUDE_DARK.safe);
        assert_eq!(r().fill_colour(69.9), CLAUDE_DARK.safe);
        assert_eq!(r().fill_colour(70.0), CLAUDE_DARK.warn);
        assert_eq!(r().fill_colour(89.9), CLAUDE_DARK.warn);
        assert_eq!(r().fill_colour(90.0), CLAUDE_DARK.alert);
        assert_eq!(r().fill_colour(100.0), CLAUDE_DARK.alert);
    }

    #[test]
    fn risk_zone_thresholds() {
        assert_eq!(r().risk_zone_color(50_000), CLAUDE_DARK.safe);
        assert_eq!(r().risk_zone_color(50_001), CLAUDE_DARK.yellow);
        assert_eq!(r().risk_zone_color(80_001), CLAUDE_DARK.warn);
        assert_eq!(r().risk_zone_color(150_001), CLAUDE_DARK.alert);
    }

    #[test]
    fn day_cost_thresholds() {
        assert_eq!(r().day_cost_colour(0.0), CLAUDE_DARK.safe);
        assert_eq!(r().day_cost_colour(24.99), CLAUDE_DARK.safe);
        assert_eq!(r().day_cost_colour(25.0), CLAUDE_DARK.yellow);
        assert_eq!(r().day_cost_colour(50.0), CLAUDE_DARK.yellow);
        assert_eq!(r().day_cost_colour(50.01), CLAUDE_DARK.alert);
    }
}
