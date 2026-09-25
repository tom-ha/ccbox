use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::data::session_name;

const STALE_SECS: f64 = 24.0 * 3600.0;
const IDLE_HIDE_SECS: f64 = 10.0 * 60.0;
/// Claude Code writes one record just after raising a prompt; anything
/// later means the session moved on.
const ACTIVITY_GRACE_SECS: f64 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Permission,
    Question,
    YourTurn,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Permission => "permission",
            Kind::Question => "question",
            Kind::YourTurn => "your turn",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    pub kind: Kind,
    pub since: f64,
    pub name: String,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub transcript_path: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct HookInput {
    #[serde(default)]
    pub hook_event_name: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub notification_type: String,
    #[serde(default)]
    pub tool_name: String,
    #[serde(default)]
    pub transcript_path: String,
    #[serde(default)]
    pub cwd: String,
}

enum Action {
    Set(Kind),
    Clear,
    Ignore,
}

fn action(h: &HookInput) -> Action {
    match (h.hook_event_name.as_str(), h.notification_type.as_str()) {
        ("Notification", "permission_prompt") => Action::Set(Kind::Permission),
        ("Notification", "elicitation_dialog" | "elicitation_url_dialog" | "agent_needs_input") => {
            Action::Set(Kind::Question)
        }
        ("Notification", "idle_prompt") => Action::Set(Kind::YourTurn),
        ("PreToolUse", _) if h.tool_name == "AskUserQuestion" => Action::Set(Kind::Question),
        ("PostToolUse" | "UserPromptSubmit" | "Stop" | "SessionEnd", _) => Action::Clear,
        _ => Action::Ignore,
    }
}

fn dir(claude_dir: &Path) -> PathBuf {
    claude_dir.join("ccbox-cache").join("waiting")
}

fn marker_path(claude_dir: &Path, session_id: &str) -> Option<PathBuf> {
    let safe = !session_id.is_empty()
        && session_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    safe.then(|| dir(claude_dir).join(format!("{session_id}.json")))
}

fn display_name(h: &HookInput) -> String {
    if let Some(n) = session_name::from_transcript(&h.transcript_path) {
        return n;
    }
    let base = Path::new(&h.cwd)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let short: String = h.session_id.chars().take(8).collect();
    if base.is_empty() {
        short
    } else {
        format!("{base} {short}")
    }
}

fn owner_alive(m: &Marker) -> bool {
    m.pid.map_or(true, process_alive)
}

fn moved_on(m: &Marker) -> bool {
    let mtime = fs::metadata(&m.transcript_path)
        .and_then(|md| md.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
    mtime.is_some_and(|t| t.as_secs_f64() > m.since + ACTIVITY_GRACE_SECS)
}

#[cfg(unix)]
pub fn process_alive(pid: u32) -> bool {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    const EPERM: i32 = 1;
    // Signal 0 checks existence without delivering anything.
    let rc = unsafe { kill(pid as i32, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(EPERM)
}

#[cfg(not(unix))]
pub fn process_alive(_pid: u32) -> bool {
    true
}

/// The Claude Code process that ran this hook: the first ancestor that
/// isn't the shell Claude Code wraps hook commands in.
#[cfg(unix)]
pub fn owner_pid() -> Option<u32> {
    const SHELLS: &[&str] = &["sh", "bash", "zsh", "dash", "fish"];
    let mut pid = std::os::unix::process::parent_id();
    for _ in 0..5 {
        let out = std::process::Command::new("ps")
            .args(["-o", "ppid=,comm=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let (ppid, comm) = text.trim().split_once(char::is_whitespace)?;
        let name = Path::new(comm.trim())
            .file_name()?
            .to_str()?
            .trim_start_matches('-');
        if !SHELLS.contains(&name) {
            return Some(pid);
        }
        pid = ppid.trim().parse().ok()?;
    }
    None
}

#[cfg(not(unix))]
pub fn owner_pid() -> Option<u32> {
    None
}

fn read(path: &Path) -> Option<Marker> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// Errors are swallowed: a hook must never fail the session it runs in.
pub fn apply_hook(claude_dir: &Path, h: &HookInput, now: f64, owner_pid: Option<u32>) {
    let Some(path) = marker_path(claude_dir, &h.session_id) else {
        return;
    };
    match action(h) {
        Action::Ignore => {}
        Action::Clear => {
            let _ = fs::remove_file(&path);
        }
        Action::Set(kind) => {
            let prev = read(&path);
            let since = match &prev {
                Some(m) if m.kind == kind => m.since,
                _ => now,
            };
            let marker = Marker {
                kind,
                since,
                name: display_name(h),
                pid: owner_pid,
                transcript_path: h.transcript_path.clone(),
            };
            let _ = fs::create_dir_all(dir(claude_dir));
            if let Ok(body) = serde_json::to_string(&marker) {
                let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
                if fs::write(&tmp, body).is_ok() {
                    let _ = fs::rename(&tmp, &path);
                }
            }
        }
    }
}

/// Deletes markers from sessions that died without a `SessionEnd`.
pub fn load_all(claude_dir: &Path, now: f64) -> Vec<(String, Marker)> {
    let Ok(entries) = fs::read_dir(dir(claude_dir)) else {
        return Vec::new();
    };
    let mut out: Vec<(String, Marker)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let path = e.path();
            let sid = path
                .file_name()?
                .to_str()?
                .strip_suffix(".json")?
                .to_string();
            let m = read(&path)?;
            if now - m.since > STALE_SECS || !owner_alive(&m) || moved_on(&m) {
                let _ = fs::remove_file(&path);
                return None;
            }
            if m.kind == Kind::YourTurn && now - m.since > IDLE_HIDE_SECS {
                return None;
            }
            Some((sid, m))
        })
        .collect();
    out.sort_by(|a, b| a.1.since.total_cmp(&b.1.since));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn hook(event: &str, notif: &str, tool: &str, sid: &str) -> HookInput {
        HookInput {
            hook_event_name: event.into(),
            session_id: sid.into(),
            notification_type: notif.into(),
            tool_name: tool.into(),
            transcript_path: String::new(),
            cwd: "/home/u/api-fix".into(),
        }
    }

    #[test]
    fn notification_sets_and_answer_clears() {
        let d = tempdir().unwrap();
        apply_hook(
            d.path(),
            &hook("Notification", "permission_prompt", "", "s1"),
            10.0,
            None,
        );
        let all = load_all(d.path(), 20.0);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].0, "s1");
        assert_eq!(all[0].1.kind, Kind::Permission);
        assert_eq!(all[0].1.name, "api-fix s1");

        apply_hook(d.path(), &hook("PostToolUse", "", "Bash", "s1"), 30.0, None);
        assert!(load_all(d.path(), 40.0).is_empty());
    }

    #[test]
    fn ask_user_question_counts_as_question() {
        let d = tempdir().unwrap();
        apply_hook(d.path(), &hook("PreToolUse", "", "Bash", "s1"), 10.0, None);
        assert!(load_all(d.path(), 11.0).is_empty());
        apply_hook(
            d.path(),
            &hook("PreToolUse", "", "AskUserQuestion", "s1"),
            12.0,
            None,
        );
        assert_eq!(load_all(d.path(), 13.0)[0].1.kind, Kind::Question);
    }

    #[test]
    fn repeated_notification_keeps_original_start() {
        let d = tempdir().unwrap();
        apply_hook(
            d.path(),
            &hook("Notification", "idle_prompt", "", "s1"),
            10.0,
            None,
        );
        apply_hook(
            d.path(),
            &hook("Notification", "idle_prompt", "", "s1"),
            70.0,
            None,
        );
        assert_eq!(load_all(d.path(), 80.0)[0].1.since, 10.0);
    }

    #[test]
    fn stale_markers_are_dropped() {
        let d = tempdir().unwrap();
        apply_hook(
            d.path(),
            &hook("Notification", "idle_prompt", "", "s1"),
            0.0,
            None,
        );
        assert!(load_all(d.path(), STALE_SECS + 1.0).is_empty());
        assert!(fs::read_dir(dir(d.path())).unwrap().next().is_none());
    }

    #[test]
    fn unsafe_session_ids_are_ignored() {
        let d = tempdir().unwrap();
        apply_hook(
            d.path(),
            &hook("Notification", "idle_prompt", "", "../x"),
            0.0,
            None,
        );
        assert!(!dir(d.path()).exists());
    }

    #[test]
    fn sorted_oldest_first() {
        let d = tempdir().unwrap();
        apply_hook(
            d.path(),
            &hook("Notification", "idle_prompt", "", "new"),
            50.0,
            None,
        );
        apply_hook(
            d.path(),
            &hook("Notification", "permission_prompt", "", "old"),
            10.0,
            None,
        );
        let ids: Vec<String> = load_all(d.path(), 60.0)
            .into_iter()
            .map(|(s, _)| s)
            .collect();
        assert_eq!(ids, vec!["old", "new"]);
    }

    #[test]
    fn dead_owner_drops_marker() {
        let d = tempdir().unwrap();
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let dead = child.id();
        child.wait().unwrap();
        apply_hook(
            d.path(),
            &hook("Notification", "permission_prompt", "", "gone"),
            10.0,
            Some(dead),
        );
        apply_hook(
            d.path(),
            &hook("Notification", "permission_prompt", "", "live"),
            10.0,
            Some(std::process::id()),
        );
        let ids: Vec<String> = load_all(d.path(), 20.0)
            .into_iter()
            .map(|(s, _)| s)
            .collect();
        assert_eq!(ids, vec!["live"]);
    }

    #[test]
    fn transcript_activity_after_prompt_clears_marker() {
        let d = tempdir().unwrap();
        let t = d.path().join("t.jsonl");
        fs::write(&t, "{}").unwrap();
        let written = fs::metadata(&t)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let mut h = hook("Notification", "permission_prompt", "", "s1");
        h.transcript_path = t.to_string_lossy().into();

        apply_hook(d.path(), &h, written - 1.0, None);
        assert_eq!(load_all(d.path(), written + 60.0).len(), 1, "within grace");

        apply_hook(d.path(), &hook("Stop", "", "", "s1"), 0.0, None);
        apply_hook(d.path(), &h, written - 30.0, None);
        assert!(load_all(d.path(), written + 60.0).is_empty(), "moved on");
    }

    #[test]
    fn idle_hides_after_ten_minutes_but_blocking_kinds_stay() {
        let d = tempdir().unwrap();
        apply_hook(
            d.path(),
            &hook("Notification", "idle_prompt", "", "idle"),
            0.0,
            None,
        );
        apply_hook(
            d.path(),
            &hook("Notification", "permission_prompt", "", "perm"),
            0.0,
            None,
        );
        let ids = |now| -> Vec<String> {
            load_all(d.path(), now)
                .into_iter()
                .map(|(s, _)| s)
                .collect()
        };
        assert_eq!(ids(IDLE_HIDE_SECS - 1.0), vec!["idle", "perm"]);
        assert_eq!(ids(IDLE_HIDE_SECS + 1.0), vec!["perm"]);
    }
}
