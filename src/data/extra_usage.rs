use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::input::session::RateLimits;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Ledger {
    window_resets_at: i64,
    /// session id → (cost when first seen over the limit, latest cost).
    sessions: HashMap<String, (f64, f64)>,
}

fn ledger_path(claude_dir: &Path) -> PathBuf {
    claude_dir.join("ccbox-cache").join("extra-usage.json")
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
    if session_id.is_empty() {
        return None;
    }
    let path = ledger_path(claude_dir);
    let mut ledger: Ledger = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if ledger.window_resets_at != window {
        ledger = Ledger {
            window_resets_at: window,
            ..Default::default()
        };
    }
    let entry = ledger
        .sessions
        .entry(session_id.to_string())
        .or_insert((session_cost, session_cost));
    if session_cost < entry.0 {
        entry.0 = session_cost;
    }
    entry.1 = session_cost;

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(body) = serde_json::to_string(&ledger) {
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        if fs::write(&tmp, body).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
    }

    let spent: f64 = ledger
        .sessions
        .values()
        .map(|(base, last)| last - base)
        .sum();
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
        assert!(!ledger_path(dir.path()).exists());
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
}
