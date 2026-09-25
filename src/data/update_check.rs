use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::release::{self, Version, Which};

pub const CHECK_INTERVAL_SECS: f64 = 24.0 * 3600.0;
pub const MAX_BACKOFF_SECS: f64 = 7.0 * 24.0 * 3600.0;
pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(10);
/// A spawned check is presumed still running for this long, so renders don't pile up spawns.
const SPAWN_GRACE_SECS: f64 = 600.0;
const LOCK_STALE_SECS: f64 = 120.0;
const CLOCK_SLACK_SECS: f64 = 5.0;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Cache {
    pub checked_at: f64,
    pub latest: Option<String>,
    pub failures: u32,
}

impl Cache {
    pub fn newer_than_installed(&self) -> Option<Version> {
        let latest = Version::parse(self.latest.as_deref()?)?;
        (latest > Version::installed()).then_some(latest)
    }
}

fn dir(claude_dir: &Path) -> PathBuf {
    claude_dir.join("ccbox-cache")
}

fn cache_path(claude_dir: &Path) -> PathBuf {
    dir(claude_dir).join("update-check.json")
}

fn spawn_marker(claude_dir: &Path) -> PathBuf {
    dir(claude_dir).join("update-check.spawned")
}

fn lock_path(claude_dir: &Path) -> PathBuf {
    dir(claude_dir).join("update-check.lock")
}

pub fn interval_secs(failures: u32) -> f64 {
    (CHECK_INTERVAL_SECS * 2f64.powi(failures.min(16) as i32)).min(MAX_BACKOFF_SECS)
}

/// A check from the future (clock moved back) counts as due.
pub fn check_due(cache: &Cache, now: f64) -> bool {
    let age = now - cache.checked_at;
    age >= interval_secs(cache.failures) || age < -CLOCK_SLACK_SECS
}

fn recent(t: f64, now: f64, window: f64) -> bool {
    (-CLOCK_SLACK_SECS..window).contains(&(now - t))
}

fn mtime_secs(path: &Path) -> Option<f64> {
    let t = fs::metadata(path).ok()?.modified().ok()?;
    Some(t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs_f64())
}

pub fn read_cache(claude_dir: &Path) -> Cache {
    fs::read_to_string(cache_path(claude_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Render path: one read of the cache; when a check is due, a spawn marker and a detached `ccbox update-check`.
pub fn load(claude_dir: &Path, now: f64, enabled: bool) -> Option<Cache> {
    if !enabled {
        return None;
    }
    let cache = read_cache(claude_dir);
    if check_due(&cache, now) {
        let marker = spawn_marker(claude_dir);
        let pending = mtime_secs(&marker).is_some_and(|t| recent(t, now, SPAWN_GRACE_SECS));
        if !pending {
            let _ = fs::create_dir_all(dir(claude_dir));
            if fs::write(&marker, format!("{now}")).is_ok() {
                spawn_check();
            }
        }
    }
    Some(cache)
}

fn spawn_check() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    if exe.file_stem().and_then(|s| s.to_str()) != Some("ccbox") {
        return false;
    }
    let mut cmd = Command::new(exe);
    cmd.arg("update-check")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn().is_ok()
}

/// `ccbox update-check`: under a lock, re-reads the cache and asks the release source only if a check is still due.
pub fn run_check(claude_dir: &Path, now: f64, enabled: bool, base: &str) {
    run_check_with(claude_dir, now, enabled, || {
        release::fetch_release(&release::agent(NETWORK_TIMEOUT), base, Which::Latest)
            .map(|r| r.map(|r| r.version.to_string()))
    })
}

pub fn run_check_with(
    claude_dir: &Path,
    now: f64,
    enabled: bool,
    fetch: impl FnOnce() -> Result<Option<String>, String>,
) {
    if !enabled {
        return;
    }
    let _ = fs::create_dir_all(dir(claude_dir));
    let lock = lock_path(claude_dir);
    if mtime_secs(&lock).is_some_and(|t| !recent(t, now, LOCK_STALE_SECS)) {
        let _ = fs::remove_file(&lock);
    }
    if OpenOptions::new().write(true).create_new(true).open(&lock).is_err() {
        return;
    }
    let mut cache = read_cache(claude_dir);
    if check_due(&cache, now) {
        match fetch() {
            Ok(latest) => {
                cache.latest = latest;
                cache.failures = 0;
            }
            Err(_) => cache.failures = cache.failures.saturating_add(1),
        }
        cache.checked_at = now;
        write_cache(claude_dir, &cache);
    }
    let _ = fs::remove_file(&lock);
}

fn write_cache(claude_dir: &Path, cache: &Cache) {
    let path = cache_path(claude_dir);
    let Ok(body) = serde_json::to_string(cache) else {
        return;
    };
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    if fs::write(&tmp, body).is_ok() && fs::rename(&tmp, &path).is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const DAY: f64 = CHECK_INTERVAL_SECS;
    const T0: f64 = 2_000_000_000.0;

    fn cache(checked_at: f64, failures: u32, latest: Option<&str>) -> Cache {
        Cache {
            checked_at,
            latest: latest.map(str::to_string),
            failures,
        }
    }

    #[test]
    fn backoff_doubles_from_a_day_and_caps_at_a_week() {
        assert_eq!(interval_secs(0), DAY);
        assert_eq!(interval_secs(1), 2.0 * DAY);
        assert_eq!(interval_secs(2), 4.0 * DAY);
        assert_eq!(interval_secs(3), MAX_BACKOFF_SECS);
        assert_eq!(interval_secs(40), MAX_BACKOFF_SECS);
    }

    #[test]
    fn due_once_a_day_with_backoff_and_clock_skew() {
        assert!(check_due(&Cache::default(), T0), "never checked");
        assert!(!check_due(&cache(T0, 0, None), T0 + DAY - 1.0), "checked today");
        assert!(check_due(&cache(T0, 0, None), T0 + DAY), "a day later");
        assert!(!check_due(&cache(T0, 1, None), T0 + DAY + 1.0), "backing off after a failure");
        assert!(check_due(&cache(T0, 1, None), T0 + 2.0 * DAY), "backoff over");
        assert!(!check_due(&cache(T0, 9, None), T0 + 6.9 * DAY), "capped backoff not over");
        assert!(check_due(&cache(T0, 9, None), T0 + 7.0 * DAY), "capped at a week");
        assert!(check_due(&cache(T0 + 3600.0, 0, None), T0), "checked in the future");
        assert!(!check_due(&cache(T0 + 1.0, 0, None), T0), "within clock slack");
    }

    #[test]
    fn chip_only_for_a_strictly_newer_release() {
        let installed = Version::installed();
        let newer = Version { major: installed.major + 1, minor: 0, patch: 0 };
        assert_eq!(cache(T0, 0, Some(&newer.to_string())).newer_than_installed(), Some(newer));
        assert_eq!(cache(T0, 0, Some(&installed.to_string())).newer_than_installed(), None);
        assert_eq!(cache(T0, 0, Some("0.0.1")).newer_than_installed(), None);
        assert_eq!(cache(T0, 0, None).newer_than_installed(), None);
        assert_eq!(cache(T0, 0, Some("garbage")).newer_than_installed(), None);
    }

    #[test]
    fn disabled_reads_and_writes_nothing() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(load(d.path(), T0, false), None);
        run_check_with(d.path(), T0, false, || panic!("no network when disabled"));
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 0);
    }

    #[test]
    fn load_marks_a_spawn_once_while_it_is_pending() {
        let d = tempfile::tempdir().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        assert_eq!(load(d.path(), now, true), Some(Cache::default()));
        let marker = spawn_marker(d.path());
        let first = fs::read_to_string(&marker).unwrap();
        load(d.path(), now + 1.0, true);
        assert_eq!(fs::read_to_string(&marker).unwrap(), first, "spawn still pending");
        load(d.path(), now + SPAWN_GRACE_SECS + 1.0, true);
        assert_ne!(fs::read_to_string(&marker).unwrap(), first, "grace over, spawned again");
    }

    #[test]
    fn load_does_not_spawn_when_the_cache_is_fresh() {
        let d = tempfile::tempdir().unwrap();
        run_check_with(d.path(), T0, true, || Ok(Some("9.9.9".into())));
        assert_eq!(load(d.path(), T0 + 60.0, true), Some(cache(T0, 0, Some("9.9.9"))));
        assert!(!spawn_marker(d.path()).exists());
    }

    #[test]
    fn run_check_asks_once_per_interval() {
        let d = tempfile::tempdir().unwrap();
        let calls = Cell::new(0);
        let fetch = || {
            calls.set(calls.get() + 1);
            Ok(Some("9.9.9".to_string()))
        };
        run_check_with(d.path(), T0, true, fetch);
        for i in 1..50 {
            run_check_with(d.path(), T0 + i as f64 * 60.0, true, fetch);
        }
        assert_eq!(calls.get(), 1);
        run_check_with(d.path(), T0 + DAY, true, fetch);
        assert_eq!(calls.get(), 2);
        assert!(!lock_path(d.path()).exists());
    }

    #[test]
    fn failure_keeps_the_last_answer_and_backs_off() {
        let d = tempfile::tempdir().unwrap();
        run_check_with(d.path(), T0, true, || Ok(Some("9.9.9".into())));
        run_check_with(d.path(), T0 + DAY, true, || Err("offline".into()));
        assert_eq!(read_cache(d.path()), cache(T0 + DAY, 1, Some("9.9.9")));
        run_check_with(d.path(), T0 + 2.0 * DAY, true, || panic!("still backing off"));
        run_check_with(d.path(), T0 + 3.0 * DAY, true, || Ok(None));
        assert_eq!(read_cache(d.path()), cache(T0 + 3.0 * DAY, 0, None));
    }

    #[test]
    fn a_held_lock_skips_the_check_and_a_stale_one_is_broken() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir(d.path())).unwrap();
        fs::write(lock_path(d.path()), "").unwrap();
        let now = mtime_secs(&lock_path(d.path())).unwrap();
        run_check_with(d.path(), now, true, || panic!("lock is held"));
        run_check_with(d.path(), now + LOCK_STALE_SECS + 1.0, true, || Ok(Some("1.2.3".into())));
        assert_eq!(read_cache(d.path()).latest.as_deref(), Some("1.2.3"));
    }
}
