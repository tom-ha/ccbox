use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::data::iso::parse_iso_to_epoch;

pub const REFRESH_SECS: f64 = 300.0;
const MAX_AGE_SECS: f64 = 3600.0;
const LOCK_STALE_SECS: f64 = 120.0;
const MAX_BACKOFF_SECS: f64 = 6.0 * 3600.0;
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelLimit {
    pub name: String,
    pub used_pct: f64,
    pub resets_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtraUsage {
    pub used: f64,
    pub limit: f64,
    pub currency: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AccountUsage {
    pub fetched_at: f64,
    pub model_limits: Vec<ModelLimit>,
    pub extra_usage: Option<ExtraUsage>,
}

fn cache_path(claude_dir: &Path) -> PathBuf {
    claude_dir.join("ccbox-cache").join("account-usage.json")
}

fn lock_path(claude_dir: &Path) -> PathBuf {
    claude_dir.join("ccbox-cache").join("account-usage.lock")
}

fn attempt_path(claude_dir: &Path) -> PathBuf {
    claude_dir
        .join("ccbox-cache")
        .join("account-usage-attempt.json")
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Attempt {
    at: f64,
    failures: u32,
}

fn read_attempt(claude_dir: &Path) -> Attempt {
    fs::read_to_string(attempt_path(claude_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_attempt(claude_dir: &Path, a: &Attempt) {
    let path = attempt_path(claude_dir);
    let _ = fs::create_dir_all(path.parent().unwrap_or(claude_dir));
    if let Ok(body) = serde_json::to_string(a) {
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        if fs::write(&tmp, body).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
    }
}

/// Failed refreshes back off exponentially, so an unusable `claude` doesn't
/// get relaunched on every render.
fn backoff_secs(failures: u32) -> f64 {
    (REFRESH_SECS * 2f64.powi(failures.min(16) as i32)).min(MAX_BACKOFF_SECS)
}

fn mtime_secs(path: &Path) -> Option<f64> {
    let t = fs::metadata(path).ok()?.modified().ok()?;
    Some(t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs_f64())
}

/// A clock that jumped backwards leaves timestamps in the future; treat those
/// as due rather than waiting for the clock to catch up.
fn refresh_due(cache_age: f64, attempt: &Attempt, now: f64) -> bool {
    let cache_due = cache_age >= REFRESH_SECS || cache_age < 0.0;
    let backoff_over = now - attempt.at >= backoff_secs(attempt.failures) || attempt.at > now;
    cache_due && backoff_over
}

/// Refreshes in a detached process so the statusline never waits on it.
pub fn load(claude_dir: &Path, now: f64) -> Option<AccountUsage> {
    let cached: Option<AccountUsage> = fs::read_to_string(cache_path(claude_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    let age = cached
        .as_ref()
        .map_or(f64::INFINITY, |c| now - c.fetched_at);
    let refreshing = mtime_secs(&lock_path(claude_dir)).is_some_and(|t| now - t < LOCK_STALE_SECS);
    let attempt = read_attempt(claude_dir);
    let due = refresh_due(age, &attempt, now);
    if due && !refreshing && spawn_refresh() {
        write_attempt(
            claude_dir,
            &Attempt {
                at: now,
                failures: attempt.failures,
            },
        );
    }
    cached.filter(|_| age < MAX_AGE_SECS)
}

fn spawn_refresh() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    if exe.file_stem().and_then(|s| s.to_str()) != Some("ccbox") {
        return false;
    }
    let mut cmd = Command::new(exe);
    cmd.arg("usage-refresh")
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

pub fn refresh(claude_dir: &Path, now: f64) {
    let dir = claude_dir.join("ccbox-cache");
    let _ = fs::create_dir_all(&dir);
    let lock = lock_path(claude_dir);
    if mtime_secs(&lock).is_some_and(|t| now - t >= LOCK_STALE_SECS) {
        let _ = fs::remove_file(&lock);
    }
    if OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .is_err()
    {
        return;
    }
    let usage = probe().and_then(|v| parse_usage(&v, now));
    let failures = match &usage {
        Some(_) => 0,
        None => read_attempt(claude_dir).failures.saturating_add(1),
    };
    write_attempt(claude_dir, &Attempt { at: now, failures });
    if let Some(body) = usage.and_then(|u| serde_json::to_string(&u).ok()) {
        let path = cache_path(claude_dir);
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        if fs::write(&tmp, body).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
    }
    let _ = fs::remove_file(&lock);
}

/// Asks Claude Code for its `/usage` data over the SDK control protocol.
/// No prompt is sent, so no model call is made; user settings are skipped so
/// the probe runs none of the user's hooks and leaves no transcript.
fn probe() -> Option<Value> {
    let mut child = Command::new("claude")
        .args([
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--setting-sources",
            "",
            "--no-session-persistence",
            "--strict-mcp-config",
        ])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;
    for req in [
        json!({"type": "control_request", "request_id": "init", "request": {"subtype": "initialize"}}),
        json!({"type": "control_request", "request_id": "usage", "request": {"subtype": "get_usage", "skip_behaviors": true}}),
    ] {
        let _ = writeln!(stdin, "{req}");
    }
    let _ = stdin.flush();

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if v["response"]["request_id"] == "usage" {
                let _ = tx.send(v["response"]["response"].clone());
                return;
            }
        }
    });
    let answer = rx.recv_timeout(PROBE_TIMEOUT).ok();
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    answer
}

pub fn parse_usage(v: &Value, now: f64) -> Option<AccountUsage> {
    let rl = v.get("rate_limits").filter(|r| r.is_object())?;
    let model_limits = rl["model_scoped"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|r| {
                    Some(ModelLimit {
                        name: r["display_name"].as_str()?.to_string(),
                        used_pct: r["utilization"].as_f64()?,
                        resets_at: parse_iso_to_epoch(r["resets_at"].as_str()?) as i64,
                    })
                })
                .filter(|m| m.resets_at > 0)
                .collect()
        })
        .unwrap_or_default();
    let eu = &rl["extra_usage"];
    let extra_usage = (eu["is_enabled"] == true).then(|| {
        let scale = 10f64.powi(eu["decimal_places"].as_i64().unwrap_or(2) as i32);
        ExtraUsage {
            used: eu["used_credits"].as_f64().unwrap_or(0.0) / scale,
            limit: eu["monthly_limit"].as_f64().unwrap_or(0.0) / scale,
            currency: eu["currency"].as_str().unwrap_or("USD").to_string(),
        }
    });
    Some(AccountUsage {
        fetched_at: now,
        model_limits,
        extra_usage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Value {
        json!({
            "rate_limits": {
                "five_hour": {"utilization": 13},
                "model_scoped": [
                    {"display_name": "Fable", "utilization": 82, "resets_at": "2026-09-28T09:00:00.377398+00:00"},
                    {"display_name": "Broken", "utilization": null, "resets_at": null}
                ],
                "extra_usage": {
                    "is_enabled": true, "monthly_limit": 50000, "used_credits": 13096,
                    "currency": "USD", "decimal_places": 2
                }
            }
        })
    }

    #[test]
    fn parses_model_limits_and_extra_usage() {
        let u = parse_usage(&sample(), 1.0).unwrap();
        assert_eq!(
            u.model_limits,
            vec![ModelLimit {
                name: "Fable".into(),
                used_pct: 82.0,
                resets_at: 1_790_586_000,
            }]
        );
        let e = u.extra_usage.unwrap();
        assert!((e.used - 130.96).abs() < 1e-9 && (e.limit - 500.0).abs() < 1e-9);
    }

    #[test]
    fn disabled_extra_usage_is_none() {
        let mut v = sample();
        v["rate_limits"]["extra_usage"]["is_enabled"] = json!(false);
        assert_eq!(parse_usage(&v, 1.0).unwrap().extra_usage, None);
    }

    #[test]
    fn missing_rate_limits_is_none() {
        assert_eq!(parse_usage(&json!({"rate_limits": null}), 1.0), None);
    }

    #[test]
    fn stale_cache_is_ignored_but_fresh_cache_is_used() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("ccbox-cache")).unwrap();
        fs::write(lock_path(d.path()), "").unwrap();
        let u = parse_usage(&sample(), 1_000_000.0).unwrap();
        fs::write(cache_path(d.path()), serde_json::to_string(&u).unwrap()).unwrap();
        assert!(load(d.path(), 1_000_000.0 + 10.0).is_some());
        assert!(load(d.path(), 1_000_000.0 + MAX_AGE_SECS + 1.0).is_none());
    }

    #[test]
    fn backoff_doubles_and_caps() {
        assert_eq!(backoff_secs(0), REFRESH_SECS);
        assert_eq!(backoff_secs(1), 2.0 * REFRESH_SECS);
        assert_eq!(backoff_secs(3), 8.0 * REFRESH_SECS);
        assert_eq!(backoff_secs(40), MAX_BACKOFF_SECS);
    }

    #[test]
    fn refresh_due_respects_backoff_and_clock_skew() {
        let a = |at, failures| Attempt { at, failures };
        assert!(
            refresh_due(f64::INFINITY, &a(0.0, 0), 1_000.0),
            "no cache yet"
        );
        assert!(!refresh_due(10.0, &a(0.0, 0), 1_000.0), "fresh cache");
        assert!(!refresh_due(400.0, &a(900.0, 2), 1_000.0), "backing off");
        assert!(refresh_due(400.0, &a(0.0, 2), 1_300.0), "backoff over");
        assert!(
            refresh_due(-50.0, &a(0.0, 0), 1_000.0),
            "cache from the future"
        );
        assert!(
            refresh_due(400.0, &a(5_000.0, 3), 1_000.0),
            "attempt from the future"
        );
    }
}
