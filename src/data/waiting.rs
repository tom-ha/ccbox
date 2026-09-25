use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::data::iso::parse_iso_to_epoch;
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
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub tool_use_id: String,
    /// Tool name plus input, identifying the call a permission prompt is for.
    #[serde(default)]
    pub tool_key: String,
    /// Set from a hook that names the agent and tool (`PermissionRequest`,
    /// `PreToolUse`). `Notification` carries neither, so its markers clear on
    /// any tool completion instead.
    #[serde(default)]
    pub scoped: bool,
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
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub tool_use_id: String,
    #[serde(default)]
    pub tool_input: serde_json::Value,
}

impl HookInput {
    fn tool_key(&self) -> String {
        format!("{}:{}", self.tool_name, self.tool_input)
    }
}

enum Action {
    Set(Kind),
    ToolDone,
    /// Main-thread markers only: a background subagent can still be waiting
    /// after the main turn ends.
    ClearMain,
    ClearAgent,
    ClearAll,
    Ignore,
}

fn action(h: &HookInput) -> Action {
    match (h.hook_event_name.as_str(), h.notification_type.as_str()) {
        ("PermissionRequest", _) | ("Notification", "permission_prompt") => {
            Action::Set(Kind::Permission)
        }
        ("Notification", "elicitation_dialog" | "elicitation_url_dialog" | "agent_needs_input") => {
            Action::Set(Kind::Question)
        }
        ("Notification", "idle_prompt") => Action::Set(Kind::YourTurn),
        ("PreToolUse", _) if h.tool_name == "AskUserQuestion" => Action::Set(Kind::Question),
        ("PostToolUse" | "PostToolUseFailure" | "PermissionDenied", _) => Action::ToolDone,
        ("UserPromptSubmit" | "Stop", _) => Action::ClearMain,
        ("SessionEnd", _) => Action::ClearAll,
        ("SubagentStop", _) => Action::ClearAgent,
        _ => Action::Ignore,
    }
}

fn dir(claude_dir: &Path) -> PathBuf {
    claude_dir.join("ccbox-cache").join("waiting")
}

fn safe_id(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// One marker per agent, so parallel subagents waiting at once are tracked
/// independently: `<session>.json` for the main thread, `<session>@<agent>.json`
/// for a subagent.
fn marker_path(claude_dir: &Path, session_id: &str, agent_id: &str) -> Option<PathBuf> {
    if !safe_id(session_id) {
        return None;
    }
    let name = if agent_id.is_empty() {
        format!("{session_id}.json")
    } else if safe_id(agent_id) {
        format!("{session_id}@{agent_id}.json")
    } else {
        return None;
    };
    Some(dir(claude_dir).join(name))
}

fn session_markers(claude_dir: &Path, session_id: &str) -> Vec<PathBuf> {
    let prefix = format!("{session_id}@");
    fs::read_dir(dir(claude_dir))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.ends_with(".json")
                    && (n == format!("{session_id}.json") || n.starts_with(&prefix))
            })
        })
        .collect()
}

fn write_marker(claude_dir: &Path, path: &Path, marker: &Marker) {
    let _ = fs::create_dir_all(dir(claude_dir));
    if let Ok(body) = serde_json::to_string(marker) {
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        if fs::write(&tmp, body).is_ok() {
            let _ = fs::rename(&tmp, path);
        }
    }
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
    m.pid.is_none_or(process_alive)
}

/// Hooks always report the main transcript, but a subagent writes to
/// `<session>/subagents/[<subdirs>/]agent-<agent_id>.jsonl`; its prompt is
/// only answered once that file moves on.
fn activity_transcript(m: &Marker) -> Option<PathBuf> {
    let main = PathBuf::from(&m.transcript_path);
    if !m.scoped || m.agent_id.is_empty() {
        return Some(main);
    }
    if !m
        .agent_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    let root = main.with_extension("").join("subagents");
    find_file(&root, &format!("agent-{}.jsonl", m.agent_id), 3)
}

/// Workflow agents nest deeper (`subagents/workflows/<run>/`).
fn find_file(dir: &Path, name: &str, depth: u32) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    if depth == 0 {
        return None;
    }
    fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .find_map(|p| find_file(&p, name, depth - 1))
}

/// Only a conversation record (`user`/`assistant`) written after the prompt
/// counts: Claude Code also appends metadata records while a prompt waits.
fn moved_on(m: &Marker) -> bool {
    const TAIL_BYTES: u64 = 64 * 1024;
    let cutoff = m.since + ACTIVITY_GRACE_SECS;
    let Some(transcript) = activity_transcript(m) else {
        return false;
    };
    let Ok(mut f) = fs::File::open(transcript) else {
        return false;
    };
    let fresh = f
        .metadata()
        .and_then(|md| md.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .is_some_and(|t| t.as_secs_f64() > m.since);
    if !fresh {
        return false;
    }
    let len = f.metadata().map(|md| md.len()).unwrap_or(0);
    let mut bytes = Vec::new();
    if f.seek(SeekFrom::Start(len.saturating_sub(TAIL_BYTES)))
        .is_err()
        || f.read_to_end(&mut bytes).is_err()
    {
        return false;
    }
    let tail = String::from_utf8_lossy(&bytes);
    tail.lines().rev().any(|ln| {
        let is_turn = ln.contains(r#""type":"user""#) || ln.contains(r#""type":"assistant""#);
        if !is_turn {
            return false;
        }
        // Esc (any agent) or a main-thread rejection fires no hook and can land
        // inside the grace window, so those records count from `since` itself.
        // A rejected subagent isn't aborted: it keeps writing records.
        let abandoned = ln.contains("[Request interrupted by user")
            || ln.contains("The user doesn't want to proceed with this tool use");
        let after = if abandoned { m.since } else { cutoff };
        serde_json::from_str::<serde_json::Value>(ln)
            .ok()
            .and_then(|v| v.get("timestamp")?.as_str().map(parse_iso_to_epoch))
            .is_some_and(|ts| ts > after)
    })
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
pub fn apply_hook(
    claude_dir: &Path,
    h: &HookInput,
    now: f64,
    owner_pid: impl FnOnce() -> Option<u32>,
) {
    if !safe_id(&h.session_id) {
        return;
    }
    match action(h) {
        Action::Ignore => {}
        Action::ClearMain => {
            if let Some(p) = marker_path(claude_dir, &h.session_id, "") {
                let _ = fs::remove_file(p);
            }
        }
        Action::ClearAgent => {
            if !h.agent_id.is_empty() {
                if let Some(p) = marker_path(claude_dir, &h.session_id, &h.agent_id) {
                    let _ = fs::remove_file(p);
                }
            }
        }
        Action::ClearAll => {
            for p in session_markers(claude_dir, &h.session_id) {
                let _ = fs::remove_file(p);
            }
        }
        Action::ToolDone => {
            // An unscoped marker clears on any main-thread completion, or any
            // completion at all for a permission prompt (installs without the
            // PermissionRequest hook can't tell whose it is); a scoped one only
            // on its own call. An answered AskUserQuestion
            // comes back with rewritten tool_input, so a known tool_use_id is
            // matched alone.
            let paths = [
                marker_path(claude_dir, &h.session_id, ""),
                marker_path(claude_dir, &h.session_id, &h.agent_id),
            ];
            for path in paths.into_iter().flatten() {
                let finishes = read(&path).is_some_and(|m| {
                    (!m.scoped && (h.agent_id.is_empty() || m.kind == Kind::Permission))
                        || (m.scoped
                            && m.agent_id == h.agent_id
                            && if m.tool_use_id.is_empty() {
                                m.tool_key.is_empty() || m.tool_key == h.tool_key()
                            } else {
                                m.tool_use_id == h.tool_use_id
                            })
                });
                if finishes {
                    let _ = fs::remove_file(&path);
                }
            }
        }
        Action::Set(kind) => {
            let scoped = matches!(
                h.hook_event_name.as_str(),
                "PermissionRequest" | "PreToolUse"
            );
            // Every permission prompt also fires the scoped PermissionRequest,
            // so its Notification is redundant; other Notifications (e.g. an
            // MCP elicitation) have no scoped twin and always get a marker.
            if !scoped && kind == Kind::Permission {
                let covered = session_markers(claude_dir, &h.session_id)
                    .iter()
                    .filter_map(|p| read(p))
                    .any(|m| m.kind == kind && m.scoped);
                if covered {
                    return;
                }
            }
            let agent = if scoped { h.agent_id.as_str() } else { "" };
            let Some(path) = marker_path(claude_dir, &h.session_id, agent) else {
                return;
            };
            let tool_key = if scoped { h.tool_key() } else { String::new() };
            let since = match read(&path) {
                Some(m)
                    if m.kind == kind
                        && m.tool_key == tool_key
                        && m.tool_use_id == h.tool_use_id =>
                {
                    m.since
                }
                _ => now,
            };
            let marker = Marker {
                kind,
                since,
                name: display_name(h),
                pid: owner_pid(),
                transcript_path: h.transcript_path.clone(),
                agent_id: agent.to_string(),
                tool_use_id: h.tool_use_id.clone(),
                tool_key,
                scoped,
            };
            write_marker(claude_dir, &path, &marker);
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
            let stem = path.file_name()?.to_str()?.strip_suffix(".json")?;
            let sid = stem.split('@').next()?.to_string();
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
    // One entry per session: a blocking prompt outranks "your turn", then the
    // oldest wins.
    out.sort_by(|a, b| {
        (a.1.kind == Kind::YourTurn)
            .cmp(&(b.1.kind == Kind::YourTurn))
            .then(a.1.since.total_cmp(&b.1.since))
    });
    let mut seen = std::collections::HashSet::new();
    out.retain(|(sid, _)| seen.insert(sid.clone()));
    out.sort_by(|a, b| a.1.since.total_cmp(&b.1.since).then(a.0.cmp(&b.0)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn hook(event: &str, notif: &str, tool: &str, sid: &str) -> HookInput {
        HookInput {
            hook_event_name: event.into(),
            session_id: sid.into(),
            notification_type: notif.into(),
            tool_name: tool.into(),
            transcript_path: String::new(),
            cwd: "/home/u/api-fix".into(),
            agent_id: String::new(),
            tool_use_id: String::new(),
            tool_input: serde_json::Value::Null,
        }
    }

    #[test]
    fn notification_sets_and_answer_clears() {
        let d = tempdir().unwrap();
        apply_hook(
            d.path(),
            &hook("Notification", "permission_prompt", "", "s1"),
            10.0,
            || None,
        );
        let all = load_all(d.path(), 20.0);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].0, "s1");
        assert_eq!(all[0].1.kind, Kind::Permission);
        assert_eq!(all[0].1.name, "api-fix s1");

        apply_hook(
            d.path(),
            &hook("PostToolUse", "", "Bash", "s1"),
            30.0,
            || None,
        );
        assert!(load_all(d.path(), 40.0).is_empty());
    }

    #[test]
    fn ask_user_question_counts_as_question() {
        let d = tempdir().unwrap();
        apply_hook(
            d.path(),
            &hook("PreToolUse", "", "Bash", "s1"),
            10.0,
            || None,
        );
        assert!(load_all(d.path(), 11.0).is_empty());
        apply_hook(
            d.path(),
            &hook("PreToolUse", "", "AskUserQuestion", "s1"),
            12.0,
            || None,
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
            || None,
        );
        apply_hook(
            d.path(),
            &hook("Notification", "idle_prompt", "", "s1"),
            70.0,
            || None,
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
            || None,
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
            || None,
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
            || None,
        );
        apply_hook(
            d.path(),
            &hook("Notification", "permission_prompt", "", "old"),
            10.0,
            || None,
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
            move || Some(dead),
        );
        apply_hook(
            d.path(),
            &hook("Notification", "permission_prompt", "", "live"),
            10.0,
            || Some(std::process::id()),
        );
        let ids: Vec<String> = load_all(d.path(), 20.0)
            .into_iter()
            .map(|(s, _)| s)
            .collect();
        assert_eq!(ids, vec!["live"]);
    }

    #[test]
    fn only_conversation_records_after_the_prompt_count_as_activity() {
        use chrono::{TimeZone, Utc};
        let d = tempdir().unwrap();
        let t = d.path().join("t.jsonl");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let iso = |secs: f64| Utc.timestamp_opt(secs as i64, 0).unwrap().to_rfc3339();
        let since = now - 60.0;
        fs::write(
            &t,
            format!(
                "{{\"type\":\"assistant\",\"timestamp\":\"{}\"}}\n\
                 {{\"type\":\"ai-title\",\"timestamp\":\"{}\"}}\n\
                 {{\"type\":\"queue-operation\",\"timestamp\":\"{}\"}}\n",
                iso(since - 1.0),
                iso(since + 20.0),
                iso(since + 40.0),
            ),
        )
        .unwrap();
        let mut h = hook("Notification", "permission_prompt", "", "s1");
        h.transcript_path = t.to_string_lossy().into();
        apply_hook(d.path(), &h, since, || None);
        assert_eq!(
            load_all(d.path(), now).len(),
            1,
            "metadata alone is not activity"
        );

        let mut f = fs::OpenOptions::new().append(true).open(&t).unwrap();
        writeln!(
            f,
            "{{\"type\":\"user\",\"timestamp\":\"{}\"}}",
            iso(since + 50.0)
        )
        .unwrap();
        assert!(load_all(d.path(), now).is_empty(), "a later user turn is");
    }

    fn tool(event: &str, name: &str, sid: &str, agent: &str, input: &str) -> HookInput {
        let mut h = hook(event, "", name, sid);
        h.agent_id = agent.into();
        h.tool_input = serde_json::json!({ "command": input });
        h
    }

    #[test]
    fn tail_starting_mid_character_still_detects_activity() {
        use chrono::{TimeZone, Utc};
        let d = tempdir().unwrap();
        let t = d.path().join("t.jsonl");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let since = now - 60.0;
        let ts = Utc
            .timestamp_opt((since + 30.0) as i64, 0)
            .unwrap()
            .to_rfc3339();
        let mut record = format!("\n{{\"type\":\"user\",\"timestamp\":\"{ts}\"}}\n");
        let filler = "é".repeat(40_000);
        let tail_start = filler.len() + record.len() - 64 * 1024;
        if tail_start % 2 == 0 {
            record.insert(0, ' ');
        }
        fs::write(&t, format!("{filler}{record}")).unwrap();
        let len = fs::metadata(&t).unwrap().len() as usize;
        assert_eq!(
            fs::read(&t).unwrap()[len - 64 * 1024] & 0xC0,
            0x80,
            "tail starts mid-char"
        );

        let mut h = hook("Notification", "permission_prompt", "", "s1");
        h.transcript_path = t.to_string_lossy().into();
        apply_hook(d.path(), &h, since, || None);
        assert!(load_all(d.path(), now).is_empty());
    }

    #[test]
    fn subagent_prompt_follows_the_subagent_transcript_not_the_main_one() {
        use chrono::{TimeZone, Utc};
        let d = tempdir().unwrap();
        let main = d.path().join("s1.jsonl");
        let sub_dir = d.path().join("s1").join("subagents");
        fs::create_dir_all(&sub_dir).unwrap();
        let sub = sub_dir.join("agent-a1.jsonl");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let since = now - 60.0;
        let line = |kind: &str| {
            let ts = Utc
                .timestamp_opt((since + 30.0) as i64, 0)
                .unwrap()
                .to_rfc3339();
            format!("{{\"type\":\"{kind}\",\"timestamp\":\"{ts}\"}}\n")
        };
        fs::write(&main, line("assistant")).unwrap();
        fs::write(&sub, "").unwrap();

        let mut h = tool("PermissionRequest", "Bash", "s1", "a1", "rm x");
        h.transcript_path = main.to_string_lossy().into();
        apply_hook(d.path(), &h, since, || None);
        assert_eq!(load_all(d.path(), now).len(), 1, "main-thread activity");

        fs::write(&sub, line("user")).unwrap();
        assert!(
            load_all(d.path(), now).is_empty(),
            "subagent got its answer"
        );
    }

    #[test]
    fn answered_question_matches_by_tool_use_id_despite_rewritten_input() {
        let d = tempdir().unwrap();
        let mut ask = tool("PreToolUse", "AskUserQuestion", "s1", "a1", "q?");
        ask.tool_use_id = "toolu_1".into();
        apply_hook(d.path(), &ask, 10.0, || None);
        let mut done = tool(
            "PostToolUse",
            "AskUserQuestion",
            "s1",
            "a1",
            "q? answers=yes",
        );
        done.tool_use_id = "toolu_1".into();
        apply_hook(d.path(), &done, 11.0, || None);
        assert!(load_all(d.path(), 12.0).is_empty());
    }

    fn marker_files(d: &Path) -> usize {
        fs::read_dir(dir(d)).map(|r| r.count()).unwrap_or(0)
    }

    #[test]
    fn parallel_subagent_prompts_are_tracked_independently() {
        use chrono::{TimeZone, Utc};
        let d = tempdir().unwrap();
        let main = d.path().join("s1.jsonl");
        let sub_dir = d.path().join("s1").join("subagents");
        fs::create_dir_all(&sub_dir).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let t0 = now - 60.0;
        let line = |at: f64| {
            let ts = Utc.timestamp_opt(at as i64, 0).unwrap().to_rfc3339();
            format!("{{\"type\":\"assistant\",\"timestamp\":\"{ts}\"}}\n")
        };
        fs::write(&main, "").unwrap();
        fs::write(sub_dir.join("agent-a1.jsonl"), line(t0 - 1.0)).unwrap();
        let mut a1 = tool("PermissionRequest", "Bash", "s1", "a1", "rm x");
        a1.transcript_path = main.to_string_lossy().into();
        apply_hook(d.path(), &a1, t0, || None);

        // b2 keeps working, then asks at t0+30.
        fs::write(sub_dir.join("agent-b2.jsonl"), line(t0 + 20.0)).unwrap();
        let mut b2 = tool("PermissionRequest", "Bash", "s1", "b2", "rm y");
        b2.transcript_path = main.to_string_lossy().into();
        apply_hook(d.path(), &b2, t0 + 30.0, || None);
        assert_eq!(load_all(d.path(), now).len(), 1, "one entry per session");
        assert_eq!(marker_files(d.path()), 2, "both prompts still pending");

        apply_hook(
            d.path(),
            &tool("PostToolUse", "Bash", "s1", "b2", "rm y"),
            now,
            || None,
        );
        assert_eq!(marker_files(d.path()), 1, "answering b2 leaves a1 waiting");
        assert_eq!(load_all(d.path(), now).len(), 1);

        apply_hook(d.path(), &hook("SessionEnd", "", "", "s1"), now, || None);
        assert_eq!(
            marker_files(d.path()),
            0,
            "session end clears every agent's marker"
        );
    }

    #[test]
    fn workflow_subagent_transcript_is_found() {
        use chrono::{TimeZone, Utc};
        let d = tempdir().unwrap();
        let main = d.path().join("s1.jsonl");
        let wf = d
            .path()
            .join("s1")
            .join("subagents")
            .join("workflows")
            .join("wf_1");
        fs::create_dir_all(&wf).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let t0 = now - 60.0;
        fs::write(&main, "").unwrap();
        let mut h = tool("PermissionRequest", "Bash", "s1", "a1", "rm x");
        h.transcript_path = main.to_string_lossy().into();
        apply_hook(d.path(), &h, t0, || None);
        let ts = Utc
            .timestamp_opt((t0 + 30.0) as i64, 0)
            .unwrap()
            .to_rfc3339();
        fs::write(
            wf.join("agent-a1.jsonl"),
            format!("{{\"type\":\"user\",\"timestamp\":\"{ts}\"}}\n"),
        )
        .unwrap();
        assert!(load_all(d.path(), now).is_empty());
    }

    #[test]
    fn blocking_prompt_outranks_an_older_your_turn() {
        let d = tempdir().unwrap();
        apply_hook(
            d.path(),
            &hook("Notification", "idle_prompt", "", "s1"),
            100.0,
            || None,
        );
        let mut p = tool("PermissionRequest", "Bash", "s1", "bg1", "rm x");
        p.transcript_path = "/nonexistent/s1.jsonl".into();
        apply_hook(d.path(), &p, 200.0, || None);
        let all = load_all(d.path(), 210.0);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].1.kind, Kind::Permission);
    }

    #[test]
    fn main_thread_elicitation_is_not_hidden_by_a_subagent_question() {
        let d = tempdir().unwrap();
        let mut q = tool("PreToolUse", "AskUserQuestion", "s1", "a1", "q?");
        q.tool_use_id = "t1".into();
        q.transcript_path = "/nonexistent/s1.jsonl".into();
        apply_hook(d.path(), &q, 100.0, || None);
        apply_hook(
            d.path(),
            &hook("Notification", "elicitation_dialog", "", "s1"),
            110.0,
            || None,
        );
        let mut done = tool("PostToolUse", "AskUserQuestion", "s1", "a1", "q? a");
        done.tool_use_id = "t1".into();
        apply_hook(d.path(), &done, 120.0, || None);
        assert_eq!(
            load_all(d.path(), 130.0).len(),
            1,
            "elicitation still pending"
        );
    }

    #[test]
    fn main_turn_ending_keeps_a_background_subagents_prompt() {
        let d = tempdir().unwrap();
        let mut p = tool("PermissionRequest", "Bash", "s1", "bg1", "rm x");
        p.transcript_path = "/nonexistent/s1.jsonl".into();
        apply_hook(d.path(), &p, 100.0, || None);
        apply_hook(
            d.path(),
            &hook("Notification", "idle_prompt", "", "s1"),
            110.0,
            || None,
        );
        apply_hook(d.path(), &hook("Stop", "", "", "s1"), 120.0, || None);
        apply_hook(
            d.path(),
            &hook("UserPromptSubmit", "", "", "s1"),
            130.0,
            || None,
        );
        let all = load_all(d.path(), 140.0);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].1.kind, Kind::Permission);
        assert_eq!(marker_files(d.path()), 1, "main-thread idle marker cleared");
    }

    fn record(kind: &str, text: &str, at: f64) -> String {
        use chrono::{TimeZone, Utc};
        let ts = Utc.timestamp_opt(at as i64, 0).unwrap().to_rfc3339();
        format!("{{\"type\":\"{kind}\",\"timestamp\":\"{ts}\",\"text\":\"{text}\"}}\n")
    }

    /// Sets a file's mtime, so a test sees what Claude Code leaves behind
    /// (the interrupt record is the file's last write).
    fn set_mtime(path: &Path, secs: f64) {
        #[repr(C)]
        struct Timeval {
            tv_sec: i64,
            tv_usec: i64,
        }
        extern "C" {
            fn utimes(path: *const std::os::raw::c_char, times: *const Timeval) -> i32;
        }
        let t = Timeval {
            tv_sec: secs as i64,
            tv_usec: 0,
        };
        let c = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();
        let times = [
            Timeval {
                tv_sec: t.tv_sec,
                tv_usec: 0,
            },
            t,
        ];
        assert_eq!(unsafe { utimes(c.as_ptr(), times.as_ptr()) }, 0);
    }

    #[test]
    fn quick_escape_on_a_subagent_prompt_still_clears() {
        let d = tempdir().unwrap();
        let main = d.path().join("s1.jsonl");
        let sub_dir = d.path().join("s1").join("subagents");
        fs::create_dir_all(&sub_dir).unwrap();
        let sub = sub_dir.join("agent-a1.jsonl");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let since = now - 60.0;
        fs::write(&main, "").unwrap();
        fs::write(&sub, "").unwrap();
        let mut h = tool("PermissionRequest", "Bash", "s1", "a1", "rm x");
        h.transcript_path = main.to_string_lossy().into();
        apply_hook(d.path(), &h, since, || None);

        fs::write(&sub, record("assistant", "next block", since + 2.0)).unwrap();
        set_mtime(&sub, since + 2.0);
        assert_eq!(
            load_all(d.path(), now).len(),
            1,
            "ordinary record inside the grace window"
        );

        fs::write(
            &sub,
            record(
                "user",
                "[Request interrupted by user for tool use]",
                since + 2.0,
            ),
        )
        .unwrap();
        set_mtime(&sub, since + 2.0);
        assert!(
            load_all(d.path(), now).is_empty(),
            "Esc at +2 s, as the file's last write"
        );
    }

    #[test]
    fn subagent_stop_clears_that_agents_marker_only() {
        let d = tempdir().unwrap();
        for agent in ["a1", "b2"] {
            let mut p = tool("PermissionRequest", "Bash", "s1", agent, "rm x");
            p.transcript_path = "/nonexistent/s1.jsonl".into();
            apply_hook(d.path(), &p, 100.0, || None);
        }
        let mut stop = hook("SubagentStop", "", "", "s1");
        stop.agent_id = "a1".into();
        apply_hook(d.path(), &stop, 110.0, || None);
        assert_eq!(marker_files(d.path()), 1);
        assert_eq!(load_all(d.path(), 120.0)[0].1.agent_id, "b2");
    }

    #[test]
    fn old_install_permission_marker_clears_on_a_subagent_completion() {
        let d = tempdir().unwrap();
        apply_hook(
            d.path(),
            &hook("Notification", "permission_prompt", "", "s1"),
            100.0,
            || None,
        );
        apply_hook(
            d.path(),
            &tool("PostToolUse", "Bash", "s1", "a1", "rm x"),
            110.0,
            || None,
        );
        assert!(load_all(d.path(), 120.0).is_empty());
    }

    #[test]
    fn question_clears_only_on_its_own_tool_call() {
        let d = tempdir().unwrap();
        let mut ask = hook("PreToolUse", "", "AskUserQuestion", "s1");
        ask.tool_use_id = "toolu_ask".into();
        apply_hook(d.path(), &ask, 10.0, || None);

        let mut other = hook("PostToolUse", "", "Read", "s1");
        other.tool_use_id = "toolu_read".into();
        apply_hook(d.path(), &other, 11.0, || None);
        assert_eq!(load_all(d.path(), 12.0).len(), 1, "parallel tool finishing");

        let mut answered = hook("PostToolUse", "", "AskUserQuestion", "s1");
        answered.tool_use_id = "toolu_ask".into();
        apply_hook(d.path(), &answered, 13.0, || None);
        assert!(load_all(d.path(), 14.0).is_empty());
    }

    #[test]
    fn subagent_permission_prompt_clears_on_its_own_completion() {
        // Real payloads: PermissionRequest and PostToolUse carry agent_id;
        // Notification carries none.
        let d = tempdir().unwrap();
        apply_hook(
            d.path(),
            &tool("PermissionRequest", "Bash", "s1", "agent-1", "rm x"),
            10.0,
            || None,
        );
        apply_hook(
            d.path(),
            &hook("Notification", "permission_prompt", "", "s1"),
            10.5,
            || None,
        );
        let m = &load_all(d.path(), 11.0)[0].1;
        assert!(
            m.scoped,
            "Notification must not downgrade the scoped marker"
        );

        apply_hook(
            d.path(),
            &tool("PostToolUse", "Bash", "s1", "", "ls"),
            11.0,
            || None,
        );
        apply_hook(
            d.path(),
            &tool("PostToolUse", "Bash", "s1", "agent-1", "ls"),
            11.5,
            || None,
        );
        assert_eq!(
            load_all(d.path(), 12.0).len(),
            1,
            "other calls leave it alone"
        );

        apply_hook(
            d.path(),
            &tool("PostToolUse", "Bash", "s1", "agent-1", "rm x"),
            13.0,
            || None,
        );
        assert!(
            load_all(d.path(), 14.0).is_empty(),
            "approved call clears it"
        );
    }

    #[test]
    fn denied_or_failed_permission_clears() {
        let d = tempdir().unwrap();
        for (i, done) in ["PermissionDenied", "PostToolUseFailure"]
            .into_iter()
            .enumerate()
        {
            let sid = format!("s{i}");
            apply_hook(
                d.path(),
                &tool("PermissionRequest", "Bash", &sid, "", "rm x"),
                10.0,
                || None,
            );
            apply_hook(
                d.path(),
                &tool(done, "Bash", &sid, "", "rm x"),
                11.0,
                || None,
            );
        }
        assert!(load_all(d.path(), 12.0).is_empty());
    }

    #[test]
    fn unscoped_non_permission_marker_clears_only_on_main_thread_completion() {
        let d = tempdir().unwrap();
        apply_hook(
            d.path(),
            &hook("Notification", "elicitation_dialog", "", "s1"),
            10.0,
            || None,
        );
        assert!(!load_all(d.path(), 11.0)[0].1.scoped);
        apply_hook(
            d.path(),
            &tool("PostToolUse", "Bash", "s1", "agent-9", "ls"),
            12.0,
            || None,
        );
        assert_eq!(load_all(d.path(), 13.0).len(), 1, "subagent completion");
        apply_hook(
            d.path(),
            &tool("PostToolUse", "Bash", "s1", "", "ls"),
            14.0,
            || None,
        );
        assert!(load_all(d.path(), 15.0).is_empty());
    }

    #[test]
    fn idle_hides_after_ten_minutes_but_blocking_kinds_stay() {
        let d = tempdir().unwrap();
        apply_hook(
            d.path(),
            &hook("Notification", "idle_prompt", "", "idle"),
            0.0,
            || None,
        );
        apply_hook(
            d.path(),
            &hook("Notification", "permission_prompt", "", "perm"),
            0.0,
            || None,
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
