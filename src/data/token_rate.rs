//! `TokenRate` — read/update `~/.claude/statusline-token-rate.log`.
//!
//! Line format:
//!   `<ts_with_3_decimals> <session_id> <total_in> <total_out>`

use std::fs;
use std::path::Path;

pub const WINDOW: f64 = 60.0;
pub const KEEP: f64 = 300.0;

#[derive(Debug, Clone, Copy)]
pub struct RateSample {
    pub ts: f64,
    pub total_in: u64,
    pub total_out: u64,
}

pub struct TokenRate;

fn read_lines(log: &Path) -> Vec<(f64, String, u64, u64)> {
    let mut rows = Vec::new();
    let contents = match fs::read_to_string(log) {
        Ok(s) => s,
        Err(_) => return rows,
    };
    for ln in contents.lines() {
        let parts: Vec<&str> = ln.split_ascii_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let ts: f64 = match parts[0].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ti: u64 = match parts[2].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let to: u64 = match parts[3].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        rows.push((ts, parts[1].to_string(), ti, to));
    }
    rows
}

impl TokenRate {
    /// Append a sample for `session_id`, garbage-collect lines older than
    /// `KEEP` seconds, then return the tokens-per-minute over the `WINDOW`
    /// most recent samples for that session.
    pub fn update(
        claude_dir: &Path,
        session_id: &str,
        total_in: u64,
        total_out: u64,
        now: f64,
    ) -> u64 {
        if session_id.is_empty() {
            return 0;
        }
        let log = claude_dir.join("statusline-token-rate.log");
        let mut rows = read_lines(&log);
        rows.retain(|(ts, _, _, _)| now - ts <= KEEP);
        rows.push((now, session_id.to_string(), total_in, total_out));

        if let Some(parent) = log.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let body = rows
            .iter()
            .map(|(ts, sid, ti, to)| format!("{ts:.3} {sid} {ti} {to}"))
            .collect::<Vec<_>>()
            .join("\n");
        let _ = fs::write(&log, format!("{body}\n"));

        let mut samples: Vec<(f64, u64, u64)> = rows
            .iter()
            .filter(|(_, sid, _, _)| sid == session_id)
            .map(|(ts, _, ti, to)| (*ts, *ti, *to))
            .filter(|(ts, _, _)| now - *ts <= WINDOW)
            .collect();
        if samples.len() < 2 {
            return 0;
        }
        samples.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let (_, ti0, to0) = samples.first().copied().unwrap();
        let (_, ti1, to1) = samples.last().copied().unwrap();
        ((ti1 + to1).saturating_sub(ti0 + to0)) as u64
    }

    /// Build an N-bucket spark history covering the most recent `window`
    /// seconds. Each bucket holds the summed token delta in that bucket.
    pub fn history(
        claude_dir: &Path,
        session_id: &str,
        n_buckets: usize,
        window: f64,
        now: f64,
    ) -> Vec<i32> {
        if n_buckets == 0 || session_id.is_empty() {
            return Vec::new();
        }
        let log = claude_dir.join("statusline-token-rate.log");
        let rows = read_lines(&log);
        let bucket_size = window / n_buckets as f64;
        let mut samples: Vec<(f64, u64, u64)> = rows
            .into_iter()
            .filter(|(_, sid, _, _)| sid == session_id)
            .filter(|(ts, _, _, _)| now - *ts <= window + bucket_size)
            .map(|(ts, _, ti, to)| (ts, ti, to))
            .collect();
        if samples.len() < 2 {
            return vec![0; n_buckets];
        }
        samples.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let last_bucket = (now / bucket_size) as i64;
        let first_bucket = last_bucket - n_buckets as i64 + 1;
        // Spread each inter-sample delta across the buckets it spans in time.
        // Pinning the whole delta to a single midpoint bucket made bursty
        // samples (e.g. 50k tokens between two ticks 2s apart) tower over
        // moderate activity, which then got crushed to zero by the
        // max-normalized sparkline rendering.
        let mut buckets = vec![0.0f64; n_buckets];
        for i in 0..samples.len() - 1 {
            let (ts0, ti0, to0) = samples[i];
            let (ts1, ti1, to1) = samples[i + 1];
            let delta = ((ti1 + to1).saturating_sub(ti0 + to0)) as f64;
            if delta == 0.0 || ts1 <= ts0 {
                continue;
            }
            let duration = ts1 - ts0;
            let start_bucket = ((ts0 / bucket_size).floor() as i64).max(first_bucket);
            let end_bucket = ((ts1 / bucket_size).floor() as i64).min(last_bucket);
            for b in start_bucket..=end_bucket {
                let bucket_start = b as f64 * bucket_size;
                let bucket_end = bucket_start + bucket_size;
                let overlap = (bucket_end.min(ts1) - bucket_start.max(ts0)).max(0.0);
                if overlap > 0.0 {
                    buckets[(b - first_bucket) as usize] += delta * overlap / duration;
                }
            }
        }
        buckets.iter().map(|v| v.round() as i32).collect()
    }

    /// Returns `(in_active, out_active)` — whether the matching counter grew
    /// in the last `window` seconds.
    pub fn recently_active(
        claude_dir: &Path,
        session_id: &str,
        now: f64,
        window: f64,
    ) -> (bool, bool) {
        if session_id.is_empty() {
            return (false, false);
        }
        let log = claude_dir.join("statusline-token-rate.log");
        let rows = read_lines(&log);
        let mut samples: Vec<(f64, u64, u64)> = rows
            .into_iter()
            .filter(|(_, sid, _, _)| sid == session_id)
            .filter(|(ts, _, _, _)| now - *ts <= window)
            .map(|(ts, _, ti, to)| (ts, ti, to))
            .collect();
        if samples.len() < 2 {
            return (false, false);
        }
        samples.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let (_, ti0, to0) = samples.first().copied().unwrap();
        let (_, ti1, to1) = samples.last().copied().unwrap();
        (ti1 > ti0, to1 > to0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn update_first_write_returns_zero() {
        let dir = tempdir().unwrap();
        let r = TokenRate::update(dir.path(), "s", 0, 0, 1000.0);
        assert_eq!(r, 0);
    }

    #[test]
    fn rate_over_window_returns_delta() {
        let dir = tempdir().unwrap();
        TokenRate::update(dir.path(), "s", 100, 200, 1000.0);
        // 30s later, totals doubled
        let r = TokenRate::update(dir.path(), "s", 200, 400, 1030.0);
        assert_eq!(r, 300);
    }

    #[test]
    fn history_returns_correct_length() {
        let dir = tempdir().unwrap();
        TokenRate::update(dir.path(), "s", 0, 0, 0.0);
        let h = TokenRate::history(dir.path(), "s", 10, 60.0, 60.0);
        assert_eq!(h.len(), 10);
    }

    #[test]
    fn history_spreads_delta_across_spanned_buckets() {
        // A single 1000-token burst spread evenly across a 10-second
        // inter-sample gap should populate every bucket in that range, not
        // pile the whole delta into a single midpoint bucket. Place the
        // burst safely inside the window so all of it lands in-range.
        let dir = tempdir().unwrap();
        TokenRate::update(dir.path(), "s", 0, 0, 1000.0);
        TokenRate::update(dir.path(), "s", 1000, 0, 1010.0);
        // 30 buckets over 60 seconds (now=1020) → 2s/bucket. The burst
        // [1000, 1010] falls well inside the window [960, 1020).
        let h = TokenRate::history(dir.path(), "s", 30, 60.0, 1020.0);
        assert_eq!(h.len(), 30);
        let nonzero = h.iter().filter(|&&v| v > 0).count();
        assert!(
            nonzero >= 4,
            "expected delta spread across ~5 buckets, got {h:?}"
        );
        let total: i32 = h.iter().sum();
        assert!(
            (total - 1000).abs() <= 5,
            "buckets must sum to ~delta, got {total}"
        );
    }

    #[test]
    fn recently_active_detects_growth() {
        let dir = tempdir().unwrap();
        TokenRate::update(dir.path(), "s", 100, 200, 1000.0);
        TokenRate::update(dir.path(), "s", 150, 250, 1005.0);
        let (in_active, out_active) = TokenRate::recently_active(dir.path(), "s", 1010.0, 10.0);
        assert!(in_active);
        assert!(out_active);
    }
}
