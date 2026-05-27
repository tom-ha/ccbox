//! Pure formatting helpers: `fmt_tok`, `fmt_dur`, `sparkline_width`.

/// Format a token count as `1`, `999`, `1.0K`, `12.3K`, `1.0M`, `4.7B`.
/// Tier boundaries are picked so the result is never longer than 6 chars —
/// token-column dividers depend on this width.
pub fn fmt_tok(n: u64) -> String {
    if n >= 999_950_000 {
        return format!("{:.1}B", n as f64 / 1_000_000_000.0);
    }
    if n >= 999_950 {
        return format!("{:.1}M", n as f64 / 1_000_000.0);
    }
    if n >= 1_000 {
        return format!("{:.1}K", n as f64 / 1000.0);
    }
    n.to_string()
}

/// Format a duration in seconds as `Ns`, `NmSSs`, or `NhMMm`.
pub fn fmt_dur(seconds: f64) -> String {
    let mut s = seconds as i64;
    if s < 0 {
        s = 0;
    }
    if s < 60 {
        return format!("{s}s");
    }
    if s < 3600 {
        return format!("{}m{:02}s", s / 60, s % 60);
    }
    format!("{}h{:02}m", s / 3600, (s % 3600) / 60)
}

/// Decide how many cells the cost-rate sparkline gets based on terminal width.
pub fn sparkline_width(terminal_width: u16) -> u16 {
    if terminal_width >= 130 {
        return 30;
    }
    if terminal_width >= 110 {
        return 20;
    }
    if terminal_width >= 90 {
        return 10;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- fmt_tok ------------------------------------------------------------
    #[test]
    fn fmt_tok_zero() {
        assert_eq!(fmt_tok(0), "0");
    }
    #[test]
    fn fmt_tok_one() {
        assert_eq!(fmt_tok(1), "1");
    }
    #[test]
    fn fmt_tok_below_thousand_unchanged() {
        assert_eq!(fmt_tok(999), "999");
    }
    #[test]
    fn fmt_tok_thousand_rounds_to_k() {
        assert_eq!(fmt_tok(1000), "1.0K");
    }
    #[test]
    fn fmt_tok_five_digit_rounds_to_one_decimal_k() {
        assert_eq!(fmt_tok(12_345), "12.3K");
    }
    #[test]
    fn fmt_tok_just_below_million_promotes_to_m() {
        assert_eq!(fmt_tok(999_999), "1.0M");
    }
    #[test]
    fn fmt_tok_million_displays_as_m() {
        assert_eq!(fmt_tok(1_000_000), "1.0M");
    }
    #[test]
    fn fmt_tok_multi_million_displays_as_m() {
        assert_eq!(fmt_tok(2_500_000), "2.5M");
    }
    #[test]
    fn fmt_tok_just_below_billion_promotes_to_b() {
        assert_eq!(fmt_tok(999_999_999), "1.0B");
    }
    #[test]
    fn fmt_tok_multi_billion_displays_as_b() {
        assert_eq!(fmt_tok(4_660_500_000), "4.7B");
    }
    #[test]
    fn fmt_tok_never_exceeds_six_chars() {
        for n in [
            0u64,
            999,
            999_999,
            1_000_000,
            999_999_999,
            4_660_500_000,
            99_999_999_999,
        ] {
            let s = fmt_tok(n);
            assert!(s.chars().count() <= 6, "n = {n}, fmt_tok = {s:?}");
        }
    }

    // --- fmt_dur ------------------------------------------------------------
    #[test]
    fn fmt_dur_negative_clamps() {
        assert_eq!(fmt_dur(-5.0), "0s");
    }
    #[test]
    fn fmt_dur_seconds() {
        assert_eq!(fmt_dur(0.0), "0s");
        assert_eq!(fmt_dur(59.0), "59s");
    }
    #[test]
    fn fmt_dur_minutes() {
        assert_eq!(fmt_dur(60.0), "1m00s");
        assert_eq!(fmt_dur(125.0), "2m05s");
        assert_eq!(fmt_dur(3599.0), "59m59s");
    }
    #[test]
    fn fmt_dur_hours() {
        assert_eq!(fmt_dur(3600.0), "1h00m");
        assert_eq!(fmt_dur(3725.0), "1h02m");
    }

    // --- sparkline_width ----------------------------------------------------
    #[test]
    fn sw_below_lower_threshold_returns_zero() {
        assert_eq!(sparkline_width(89), 0);
    }
    #[test]
    fn sw_at_lower_threshold_returns_ten() {
        assert_eq!(sparkline_width(90), 10);
    }
    #[test]
    fn sw_below_second_returns_ten() {
        assert_eq!(sparkline_width(109), 10);
    }
    #[test]
    fn sw_at_second_returns_twenty() {
        assert_eq!(sparkline_width(110), 20);
    }
    #[test]
    fn sw_below_third_returns_twenty() {
        assert_eq!(sparkline_width(129), 20);
    }
    #[test]
    fn sw_at_third_returns_thirty() {
        assert_eq!(sparkline_width(130), 30);
    }
    #[test]
    fn sw_above_all_returns_thirty() {
        assert_eq!(sparkline_width(200), 30);
    }
    #[test]
    fn sw_zero_returns_zero() {
        assert_eq!(sparkline_width(0), 0);
    }
}
