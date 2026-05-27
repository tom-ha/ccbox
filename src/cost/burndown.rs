//! `burndown_delta` — signed difference between actual spend and the linearly
//! projected spend at the same fraction of a rate-limit window.

/// Inject the current time as `now` (seconds since epoch). Mirrors
/// `statusline_command.burndown_delta`.
///
/// Returns `None` when:
/// - `resets_at == 0` (no active window)
/// - The window has already expired (`now >= resets_at`)
/// - Less than `warmup_minutes` have elapsed in the window
pub fn burndown_delta(
    used_pct: f64,
    resets_at: i64,
    window_minutes: u32,
    warmup_minutes: u32,
    now: f64,
) -> Option<f64> {
    if resets_at == 0 {
        return None;
    }
    let resets_at_f = resets_at as f64;
    if now >= resets_at_f {
        return None;
    }
    let window_start_ts = resets_at_f - (window_minutes as f64) * 60.0;
    let elapsed_minutes = (now - window_start_ts) / 60.0;
    if elapsed_minutes < warmup_minutes as f64 {
        return None;
    }
    let ideal_pct = (elapsed_minutes / window_minutes as f64) * 100.0;
    Some(used_pct - ideal_pct)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consts::{
        FIVE_HOUR_MINUTES, FIVE_HOUR_WARMUP_MINUTES, SEVEN_DAY_MINUTES, SEVEN_DAY_WARMUP_MINUTES,
    };

    const NOW: f64 = 1_000_000_000.0;

    // --- spec examples ------------------------------------------------------
    #[test]
    fn over_pace() {
        let resets = (NOW + 150.0 * 60.0) as i64;
        let d = burndown_delta(
            60.0,
            resets,
            FIVE_HOUR_MINUTES,
            FIVE_HOUR_WARMUP_MINUTES,
            NOW,
        )
        .unwrap();
        assert!((d - 10.0).abs() < 0.01, "{d}");
    }
    #[test]
    fn under_pace() {
        let resets = (NOW + 150.0 * 60.0) as i64;
        let d = burndown_delta(
            30.0,
            resets,
            FIVE_HOUR_MINUTES,
            FIVE_HOUR_WARMUP_MINUTES,
            NOW,
        )
        .unwrap();
        assert!((d - -20.0).abs() < 0.01, "{d}");
    }
    #[test]
    fn zero_usage_past_warmup() {
        let resets = (NOW + 120.0 * 60.0) as i64;
        let d = burndown_delta(
            0.0,
            resets,
            FIVE_HOUR_MINUTES,
            FIVE_HOUR_WARMUP_MINUTES,
            NOW,
        )
        .unwrap();
        assert!((d - -60.0).abs() < 0.01, "{d}");
    }

    // --- suppression --------------------------------------------------------
    #[test]
    fn no_window_resets_at_zero() {
        assert!(
            burndown_delta(50.0, 0, FIVE_HOUR_MINUTES, FIVE_HOUR_WARMUP_MINUTES, NOW).is_none()
        );
    }
    #[test]
    fn expired_window() {
        let past = NOW as i64 - 3600;
        assert!(
            burndown_delta(50.0, past, FIVE_HOUR_MINUTES, FIVE_HOUR_WARMUP_MINUTES, NOW).is_none()
        );
    }
    #[test]
    fn warmup_boundary_just_inside() {
        let resets = (NOW + 297.0 * 60.0) as i64; // elapsed = 3
        assert!(burndown_delta(
            10.0,
            resets,
            FIVE_HOUR_MINUTES,
            FIVE_HOUR_WARMUP_MINUTES,
            NOW
        )
        .is_none());
    }
    #[test]
    fn warmup_boundary_just_outside() {
        let resets = (NOW + 294.0 * 60.0) as i64; // elapsed = 6
        let r = burndown_delta(
            10.0,
            resets,
            FIVE_HOUR_MINUTES,
            FIVE_HOUR_WARMUP_MINUTES,
            NOW,
        );
        assert!(r.is_some());
    }

    // --- 7d window ----------------------------------------------------------
    #[test]
    fn seven_day_half_elapsed() {
        let resets = (NOW + 5040.0 * 60.0) as i64;
        let d = burndown_delta(
            60.0,
            resets,
            SEVEN_DAY_MINUTES,
            SEVEN_DAY_WARMUP_MINUTES,
            NOW,
        )
        .unwrap();
        assert!((d - 10.0).abs() < 0.01, "{d}");
    }
    #[test]
    fn seven_day_warmup_suppressed() {
        let resets = (NOW + (SEVEN_DAY_MINUTES as f64 - 10.0) * 60.0) as i64;
        assert!(burndown_delta(
            5.0,
            resets,
            SEVEN_DAY_MINUTES,
            SEVEN_DAY_WARMUP_MINUTES,
            NOW
        )
        .is_none());
    }
}
