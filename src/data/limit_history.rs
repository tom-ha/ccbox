use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::consts::RESETS_AT_TOLERANCE_SECS;

use crate::input::session::RateBucket;

const MAX_SAMPLES: usize = 3_000;

#[derive(Debug, Default, Serialize, Deserialize)]
struct History {
    resets_at: i64,
    samples: Vec<(f64, f64)>,
}

/// Samples are shared by every session on the account. `series` names the
/// limit (one file each); `min_gap_secs` throttles unchanged samples.
pub fn record(
    claude_dir: &Path,
    series: &str,
    bucket: &RateBucket,
    min_gap_secs: f64,
    now: f64,
) -> Vec<(f64, f64)> {
    let safe = !series.is_empty()
        && series
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-');
    if bucket.resets_at == 0 || !safe {
        return Vec::new();
    }
    let path = claude_dir
        .join("ccbox-cache")
        .join(format!("{series}-usage.json"));
    let mut h: History = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if (h.resets_at - bucket.resets_at).abs() > RESETS_AT_TOLERANCE_SECS {
        h = History::default();
    }
    h.resets_at = bucket.resets_at;

    let pct = bucket.used_percentage;
    // Each Claude Code session reports usage as of its own last API call, so
    // idle sessions keep sending older, lower values. Usage only rises
    // within a window, so anything below the latest sample is stale.
    if h.samples
        .last()
        .is_some_and(|&(ts, last)| pct < last || now < ts)
    {
        return h.samples;
    }
    let due = match h.samples.last() {
        Some(&(ts, last_pct)) => now - ts >= min_gap_secs || pct != last_pct,
        None => true,
    };
    if due {
        h.samples.push((now, pct));
        if h.samples.len() > MAX_SAMPLES {
            h.samples.drain(..h.samples.len() - MAX_SAMPLES);
        }
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(body) = serde_json::to_string(&h) {
            let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
            if fs::write(&tmp, body).is_ok() {
                let _ = fs::rename(&tmp, &path);
            }
        }
    }
    h.samples
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn bucket(pct: f64, resets_at: i64) -> RateBucket {
        RateBucket {
            used_percentage: pct,
            resets_at,
        }
    }

    #[test]
    fn no_window_records_nothing() {
        let dir = tempdir().unwrap();
        assert!(record(dir.path(), "five-hour", &bucket(10.0, 0), 30.0, 100.0).is_empty());
    }

    #[test]
    fn throttles_unchanged_samples_but_keeps_changes() {
        let dir = tempdir().unwrap();
        record(dir.path(), "five-hour", &bucket(10.0, 20_000), 30.0, 100.0);
        record(dir.path(), "five-hour", &bucket(10.0, 20_000), 30.0, 110.0);
        record(dir.path(), "five-hour", &bucket(11.0, 20_000), 30.0, 115.0);
        let s = record(dir.path(), "five-hour", &bucket(11.0, 20_000), 30.0, 150.0);
        assert_eq!(s, vec![(100.0, 10.0), (115.0, 11.0), (150.0, 11.0)]);
    }

    #[test]
    fn series_are_kept_apart_and_names_are_sanitised() {
        let dir = tempdir().unwrap();
        record(dir.path(), "five-hour", &bucket(10.0, 20_000), 30.0, 100.0);
        let week = record(dir.path(), "seven-day", &bucket(50.0, 90_000), 300.0, 100.0);
        assert_eq!(week, vec![(100.0, 50.0)]);
        assert!(record(dir.path(), "../x", &bucket(1.0, 1), 30.0, 100.0).is_empty());
    }

    #[test]
    fn stale_lower_readings_from_idle_sessions_are_dropped() {
        let dir = tempdir().unwrap();
        record(dir.path(), "seven-day", &bucket(84.0, 90_000), 300.0, 100.0);
        record(dir.path(), "seven-day", &bucket(50.0, 90_000), 300.0, 500.0);
        record(dir.path(), "seven-day", &bucket(79.0, 90_000), 300.0, 900.0);
        let s = record(
            dir.path(),
            "seven-day",
            &bucket(85.0, 90_000),
            300.0,
            1_300.0,
        );
        assert_eq!(s, vec![(100.0, 84.0), (1_300.0, 85.0)]);
    }

    #[test]
    fn new_window_discards_old_samples() {
        let dir = tempdir().unwrap();
        record(dir.path(), "five-hour", &bucket(80.0, 20_000), 30.0, 100.0);
        record(dir.path(), "five-hour", &bucket(80.0, 20_300), 30.0, 200.0);
        let s = record(dir.path(), "five-hour", &bucket(2.0, 40_000), 30.0, 300.0);
        assert_eq!(s, vec![(300.0, 2.0)]);
    }
}
