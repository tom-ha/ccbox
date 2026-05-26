//! `parse_iso_to_epoch` — RFC-3339/ISO-8601 → Unix epoch seconds.

use chrono::DateTime;

/// Returns `0.0` for any unparseable input.
pub fn parse_iso_to_epoch(ts: &str) -> f64 {
    if ts.is_empty() {
        return 0.0;
    }
    let normalised = if ts.ends_with('Z') {
        format!("{}+00:00", &ts[..ts.len() - 1])
    } else {
        ts.to_string()
    };
    match DateTime::parse_from_rfc3339(&normalised) {
        Ok(dt) => dt.timestamp_millis() as f64 / 1000.0,
        Err(_) => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn parses_z_suffix() {
        let v = parse_iso_to_epoch("2025-01-01T00:00:00Z");
        assert!(v > 1_700_000_000.0);
    }
    #[test] fn parses_offset() {
        let v = parse_iso_to_epoch("2025-01-01T00:00:00+00:00");
        assert!(v > 1_700_000_000.0);
    }
    #[test] fn parses_milliseconds() {
        let v = parse_iso_to_epoch("2025-01-01T00:00:00.500Z");
        assert!((v.fract() - 0.5).abs() < 1e-6);
    }
    #[test] fn empty_and_bad_input_returns_zero() {
        assert_eq!(parse_iso_to_epoch(""), 0.0);
        assert_eq!(parse_iso_to_epoch("nonsense"), 0.0);
    }
}
