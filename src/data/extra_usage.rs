use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::consts::RESETS_AT_TOLERANCE_SECS;

use crate::input::session::RateLimits;

/// One file per session, so concurrent sessions never overwrite each other.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Entry {
    window_resets_at: i64,
    /// Cost when this session was first seen over the limit.
    base: f64,
    last: f64,
}

fn ledger_dir(claude_dir: &Path) -> PathBuf {
    claude_dir.join("ccbox-cache").join("extra-usage")
}

fn read_entry(path: &Path) -> Option<Entry> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

fn same_window(a: i64, b: i64) -> bool {
    (a - b).abs() <= RESETS_AT_TOLERANCE_SECS
}

pub fn exhausted_window(rl: &RateLimits) -> Option<i64> {
    [rl.five_hour, rl.seven_day]
        .iter()
        .filter(|b| b.resets_at != 0 && b.used_percentage >= 100.0)
        .map(|b| b.resets_at)
        .max()
}

/// Claude Code reports no extra-usage spend, so this estimates it: list-price
/// cost accrued after a window hit 100%, summed across sessions until it resets.
pub fn update(
    claude_dir: &Path,
    session_id: &str,
    rate_limits: &RateLimits,
    session_cost: f64,
) -> Option<f64> {
    let window = exhausted_window(rate_limits)?;
    let safe = !session_id.is_empty()
        && session_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !safe {
        return None;
    }
    let dir = ledger_dir(claude_dir);
    let path = dir.join(format!("{session_id}.json"));
    let mut entry = read_entry(&path)
        .filter(|e| same_window(e.window_resets_at, window))
        .unwrap_or(Entry {
            window_resets_at: window,
            base: session_cost,
            last: session_cost,
        });
    entry.base = entry.base.min(session_cost);
    entry.last = session_cost;
    let _ = fs::create_dir_all(&dir);
    if let Ok(body) = serde_json::to_string(&entry) {
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        if fs::write(&tmp, body).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
    }

    let mut spent = 0.0;
    for e in fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
    {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        match read_entry(&p) {
            Some(other) if same_window(other.window_resets_at, window) => {
                spent += other.last - other.base;
            }
            Some(other) if other.window_resets_at < window - RESETS_AT_TOLERANCE_SECS => {
                let _ = fs::remove_file(&p);
            }
            _ => {}
        }
    }
    (spent >= 0.005).then_some(spent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::session::RateBucket;
    use tempfile::tempdir;

    fn limits(five: f64, week: f64) -> RateLimits {
        RateLimits {
            five_hour: RateBucket {
                used_percentage: five,
                resets_at: 1_000,
            },
            seven_day: RateBucket {
                used_percentage: week,
                resets_at: 9_000,
            },
        }
    }

    #[test]
    fn within_limits_shows_nothing_and_writes_nothing() {
        let dir = tempdir().unwrap();
        assert_eq!(update(dir.path(), "s1", &limits(99.0, 50.0), 4.0), None);
        assert!(!ledger_dir(dir.path()).exists());
    }

    #[test]
    fn counts_only_cost_after_the_limit_is_hit() {
        let dir = tempdir().unwrap();
        let rl = limits(100.0, 50.0);
        assert_eq!(update(dir.path(), "s1", &rl, 4.0), None);
        let spent = update(dir.path(), "s1", &rl, 6.5).unwrap();
        assert!((spent - 2.5).abs() < 1e-9, "{spent}");
    }

    #[test]
    fn sums_across_sessions_in_the_same_window() {
        let dir = tempdir().unwrap();
        let rl = limits(100.0, 50.0);
        update(dir.path(), "s1", &rl, 1.0);
        update(dir.path(), "s1", &rl, 3.0);
        update(dir.path(), "s2", &rl, 0.0);
        let spent = update(dir.path(), "s2", &rl, 1.5).unwrap();
        assert!((spent - 3.5).abs() < 1e-9, "{spent}");
    }

    #[test]
    fn new_window_starts_a_fresh_ledger() {
        let dir = tempdir().unwrap();
        update(dir.path(), "s1", &limits(100.0, 50.0), 1.0);
        update(dir.path(), "s1", &limits(100.0, 50.0), 3.0);
        let mut next = limits(100.0, 50.0);
        next.five_hour.resets_at = 2_000;
        assert_eq!(update(dir.path(), "s1", &next, 3.0), None);
    }

    #[test]
    fn weekly_exhaustion_is_the_binding_window() {
        assert_eq!(exhausted_window(&limits(100.0, 100.0)), Some(9_000));
        assert_eq!(exhausted_window(&limits(40.0, 100.0)), Some(9_000));
        assert_eq!(exhausted_window(&limits(40.0, 99.9)), None);
    }

    #[test]
    fn wobbling_resets_at_keeps_the_ledger() {
        let dir = tempdir().unwrap();
        update(dir.path(), "s1", &limits(100.0, 50.0), 1.0);
        update(dir.path(), "s1", &limits(100.0, 50.0), 3.0);
        let mut wobbled = limits(100.0, 50.0);
        wobbled.five_hour.resets_at += 120;
        let spent = update(dir.path(), "s1", &wobbled, 4.0).unwrap();
        assert!((spent - 3.0).abs() < 1e-9, "{spent}");
    }

    #[test]
    fn unsafe_session_ids_are_ignored() {
        let dir = tempdir().unwrap();
        assert_eq!(update(dir.path(), "../x", &limits(100.0, 50.0), 1.0), None);
        assert!(!ledger_dir(dir.path()).exists());
    }
}
