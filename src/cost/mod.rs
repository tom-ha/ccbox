//! Per-model token rates and session/day cost aggregation.

pub mod burndown;

/// Per-1M input/output rates in USD.
#[derive(Debug, Clone, Copy)]
pub struct Rates {
    pub input: f64,
    pub output: f64,
}

pub const OPUS: Rates = Rates {
    input: 15.00,
    output: 75.00,
};
pub const HAIKU: Rates = Rates {
    input: 0.80,
    output: 4.00,
};
pub const DEFAULT_RATES: Rates = Rates {
    input: 3.00,
    output: 15.00,
};

/// Return the per-1M USD rates for a model. Case-insensitive substring match
/// against the model name; unknown models fall back to the default.
pub fn rates_for(model_name: &str) -> Rates {
    let m = model_name.to_ascii_lowercase();
    if m.contains("opus") {
        OPUS
    } else if m.contains("haiku") {
        HAIKU
    } else {
        DEFAULT_RATES
    }
}

/// Compact per-message token usage breakdown.
#[derive(Debug, Default, Clone, Copy)]
pub struct Usage {
    pub input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub output_tokens: u64,
}

/// Compact day-totals record stored in `~/.claude/statusline-tokens.log`.
#[derive(Debug, Default, Clone, Copy)]
pub struct DayTotals {
    pub day_in: u64,
    pub day_cache_read: u64,
    pub day_out: u64,
}

/// Used by callers to pick rates without committing to a specific Model
/// type. The renderer's `Model` struct lives in `input::session`.
pub trait HasCostRates {
    fn cost_rates(&self) -> Rates;
}

/// Compute the USD session cost from a usage tally + per-model rates.
/// Mirrors `TokenAccounting.session_cost`.
pub fn session_cost(rates: Rates, usage: &Usage) -> f64 {
    let cost = (usage.input_tokens as f64) * rates.input
        + (usage.cache_creation_input_tokens as f64) * rates.input * 1.25
        + (usage.cache_read_input_tokens as f64) * rates.input * 0.1
        + (usage.output_tokens as f64) * rates.output;
    cost / 1_000_000.0
}

/// Compute the USD day cost from token-log day totals.
/// Mirrors `TokenAccounting.day_cost`.
pub fn day_cost(rates: Rates, totals: &DayTotals) -> f64 {
    let cost = (totals.day_in as f64) * rates.input
        + (totals.day_cache_read as f64) * rates.input * 0.1
        + (totals.day_out as f64) * rates.output;
    cost / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    // --- rates_for ----------------------------------------------------------
    #[test]
    fn opus_rates() {
        let r = rates_for("claude-opus-4-7");
        assert_eq!(r.input, 15.00);
        assert_eq!(r.output, 75.00);
    }
    #[test]
    fn haiku_rates_via_id() {
        let r = rates_for("claude-haiku-4-5-20251001");
        assert_eq!(r.input, 0.80);
        assert_eq!(r.output, 4.00);
    }
    #[test]
    fn sonnet_default_rates() {
        let r = rates_for("claude-sonnet-4-6");
        assert_eq!(r.input, 3.00);
        assert_eq!(r.output, 15.00);
    }
    #[test]
    fn unknown_model_default_rates() {
        let r = rates_for("gpt-5");
        assert_eq!(r.input, 3.00);
        assert_eq!(r.output, 15.00);
    }
    #[test]
    fn case_insensitive_opus() {
        let r = rates_for("CLAUDE-OPUS-4");
        assert_eq!(r.input, 15.00);
    }
    #[test]
    fn case_insensitive_haiku() {
        let r = rates_for("HAIKU 4.5");
        assert_eq!(r.input, 0.80);
    }

    // --- session_cost -------------------------------------------------------
    #[test]
    fn session_cost_sonnet() {
        let r = rates_for("claude-sonnet-4-6");
        let u = Usage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            ..Default::default()
        };
        approx(session_cost(r, &u), 18.0);
    }
    #[test]
    fn session_cost_opus_cache() {
        let r = rates_for("opus");
        let u = Usage {
            cache_creation_input_tokens: 1_000_000,
            cache_read_input_tokens: 1_000_000,
            ..Default::default()
        };
        approx(session_cost(r, &u), 20.25);
    }
    #[test]
    fn session_cost_haiku() {
        let r = rates_for("haiku");
        let u = Usage {
            input_tokens: 2_000_000,
            output_tokens: 1_000_000,
            ..Default::default()
        };
        approx(session_cost(r, &u), 5.60);
    }
    #[test]
    fn session_cost_default_zero() {
        let r = rates_for("");
        approx(session_cost(r, &Usage::default()), 0.0);
    }

    // --- day_cost -----------------------------------------------------------
    #[test]
    fn day_cost_via_log() {
        let r = rates_for("claude-sonnet-4-6");
        let log = DayTotals {
            day_in: 500_000,
            day_cache_read: 200_000,
            day_out: 100_000,
        };
        approx(day_cost(r, &log), 3.06);
    }
}
