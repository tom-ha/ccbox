//! `RunningSubagents::from_session` — discover live subagent transcripts.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::Value;

use crate::data::iso::parse_iso_to_epoch;
use crate::data::transcript::{lines_containing, lines_containing_rev};

#[derive(Debug, Clone)]
pub enum SubagentActivity {
    ToolUse { name: String, input: Value },
    Thinking,
    Responding,
    None,
}

impl Default for SubagentActivity {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Default, Clone)]
pub struct RunningSubagent {
    pub session_id: String,
    pub agent_type: String,
    pub description: String,
    pub model: String,
    pub billed_in: u64,
    pub cache_read_in: u64,
    pub total_input: u64,
    pub output: u64,
    pub first_timestamp: f64,
    pub last_activity: SubagentActivity,
}

#[derive(Debug, Default, Clone)]
pub struct RunningSubagents {
    pub agents: Vec<RunningSubagent>,
}

/// Fallback retention window used when the caller can't supply a
/// `last_prompt_ts` (e.g. brand-new session with no user message yet).
const STALE_SECONDS: f64 = 600.0;

/// Claude Code runs SessionEnd hooks before it stops running agents, which can
/// still write a last line while shutting down.
const SESSION_END_GRACE: f64 = 30.0;

fn safe_id(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn finished_marker(claude_dir: &Path, session_id: &str, agent_id: &str) -> Option<PathBuf> {
    (safe_id(session_id) && safe_id(agent_id)).then(|| {
        claude_dir
            .join("ccbox-cache")
            .join("subagents")
            .join(session_id)
            .join(format!("{agent_id}.done"))
    })
}

fn session_end_marker(claude_dir: &Path, session_id: &str) -> Option<PathBuf> {
    safe_id(session_id).then(|| {
        claude_dir
            .join("ccbox-cache")
            .join("subagents")
            .join(format!("{session_id}.ended"))
    })
}

fn modified_secs(path: &Path) -> Option<f64> {
    let t = path.metadata().and_then(|m| m.modified()).ok()?;
    Some(
        t.duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0),
    )
}

pub fn record_hook(claude_dir: &Path, event: &str, session_id: &str, agent_id: &str) {
    if event == "SessionEnd" {
        if let Some(ended) = session_end_marker(claude_dir, session_id) {
            let _ = fs::remove_dir_all(ended.with_file_name(session_id));
            if let Some(parent) = ended.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(ended, []);
        }
        return;
    }
    let Some(marker) = finished_marker(claude_dir, session_id, agent_id) else {
        return;
    };
    match event {
        "SubagentStart" => {
            let _ = fs::remove_file(marker);
        }
        "SubagentStop" => {
            if let Some(parent) = marker.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(marker, []);
        }
        _ => {}
    }
}

/// Latest notification time per task id; an agent continued after finishing is
/// notified again. Claude Code writes a notification as a user line, or as a
/// queued-command attachment when it arrives mid-turn.
fn notified_task_ids(transcript: &[u8]) -> HashMap<String, f64> {
    let mut latest: HashMap<String, f64> = HashMap::new();
    for ln in lines_containing(transcript, b"<task-id>") {
        let Ok(v) = serde_json::from_str::<Value>(ln) else {
            continue;
        };
        let text = match v.get("type").and_then(|x| x.as_str()) {
            Some("user")
                if v.pointer("/origin/kind") == Some(&Value::from("task-notification")) =>
            {
                v.pointer("/message/content")
            }
            Some("attachment")
                if v.pointer("/attachment/commandMode")
                    == Some(&Value::from("task-notification")) =>
            {
                v.pointer("/attachment/prompt")
            }
            _ => None,
        };
        let (Some(text), Some(ts)) = (
            text.and_then(|x| x.as_str()),
            v.get("timestamp")
                .and_then(|x| x.as_str())
                .map(parse_iso_to_epoch),
        ) else {
            continue;
        };
        let mut rest = text;
        while let Some(start) = rest.find("<task-id>") {
            rest = &rest[start + "<task-id>".len()..];
            let Some(end) = rest.find("</task-id>") else {
                break;
            };
            let at = latest.entry(rest[..end].trim().to_string()).or_insert(ts);
            *at = at.max(ts);
            rest = &rest[end..];
        }
    }
    latest
}

fn last_timestamp(path: &Path) -> f64 {
    let buf = fs::read(path).unwrap_or_default();
    let last = lines_containing_rev(&buf, b"\"timestamp\"")
        .filter_map(|ln| serde_json::from_str::<Value>(ln).ok())
        .find_map(|v| {
            v.get("timestamp")
                .and_then(|t| t.as_str())
                .map(parse_iso_to_epoch)
        });
    last.unwrap_or(0.0)
}

fn project_slug(project_dir: &str) -> String {
    project_dir
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

impl RunningSubagents {
    /// Agents started since `last_prompt_ts` (or within `STALE_SECONDS` when it is
    /// `0.0`) that have not finished: no completion-hook marker, no task notification
    /// in `transcript` at or after their last line, and written since the session last ended.
    pub fn from_session(
        claude_dir: &Path,
        session_id: &str,
        project_dir: &str,
        now: f64,
        last_prompt_ts: f64,
        transcript: &[u8],
    ) -> Self {
        if session_id.is_empty() || project_dir.is_empty() {
            return Self::default();
        }
        let slug = project_slug(project_dir);
        let subagents_dir = claude_dir
            .join("projects")
            .join(slug)
            .join(session_id)
            .join("subagents");
        if !subagents_dir.is_dir() {
            return Self::default();
        }
        let notified = notified_task_ids(transcript);
        let ended_at = session_end_marker(claude_dir, session_id)
            .and_then(|p| modified_secs(&p))
            .map(|t| t + SESSION_END_GRACE);
        let mut agents: Vec<RunningSubagent> = Vec::new();
        let entries = match fs::read_dir(&subagents_dir) {
            Ok(e) => e,
            Err(_) => return Self::default(),
        };
        for entry in entries.flatten() {
            let meta_path: PathBuf = entry.path();
            if !meta_path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.ends_with(".meta.json"))
                .unwrap_or(false)
            {
                continue;
            }
            let (agent_type, description) = match fs::read_to_string(&meta_path) {
                Ok(s) => match serde_json::from_str::<Value>(&s) {
                    Ok(v) => (
                        v.get("agentType")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        v.get("description")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                    ),
                    Err(_) => continue,
                },
                Err(_) => continue,
            };
            let jsonl = meta_path.with_extension("").with_extension("jsonl");
            if !jsonl.is_file() {
                continue;
            }
            let agent_name = meta_path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix(".meta.json"))
                .unwrap_or("");
            let Some(mtime) = modified_secs(&jsonl) else {
                continue;
            };
            let ids = [Some(agent_name), agent_name.strip_prefix("agent-")];
            let marked = ids
                .into_iter()
                .flatten()
                .any(|id| finished_marker(claude_dir, session_id, id).is_some_and(|p| p.exists()));
            // Agents run inside the Claude Code process, so after a resume any
            // agent that has not written since the session ended is gone.
            let ended =
                || ended_at.is_some_and(|end| mtime <= end || last_timestamp(&jsonl) <= end);
            let notified_since_last_write = || {
                ids.into_iter()
                    .flatten()
                    .filter_map(|id| notified.get(id))
                    .any(|&at| at >= last_timestamp(&jsonl))
            };
            if marked || ended() || notified_since_last_write() {
                continue;
            }
            let parsed = parse_subagent_transcript(&jsonl);
            // Anchor visibility to the current prompt cycle when we know it;
            // otherwise fall back to the recency window.
            let keep = if last_prompt_ts > 0.0 && parsed.first_ts > 0.0 {
                parsed.first_ts >= last_prompt_ts
            } else {
                now - mtime < STALE_SECONDS
            };
            if !keep {
                continue;
            }
            agents.push(RunningSubagent {
                session_id: session_id.to_string(),
                agent_type,
                description,
                model: parsed.model,
                billed_in: parsed.billed_in,
                cache_read_in: parsed.cache_read_in,
                total_input: parsed.billed_in + parsed.cache_read_in,
                output: parsed.output,
                first_timestamp: parsed.first_ts,
                last_activity: parsed.last_activity,
            });
        }
        agents.sort_by(|a, b| {
            a.first_timestamp
                .partial_cmp(&b.first_timestamp)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Self { agents }
    }
}

struct Parsed {
    billed_in: u64,
    cache_read_in: u64,
    output: u64,
    first_ts: f64,
    model: String,
    last_activity: SubagentActivity,
}

fn parse_subagent_transcript(path: &Path) -> Parsed {
    let mut p = Parsed {
        billed_in: 0,
        cache_read_in: 0,
        output: 0,
        first_ts: 0.0,
        model: String::new(),
        last_activity: SubagentActivity::None,
    };
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return p,
    };
    let mut seen: HashSet<String> = HashSet::new();
    for ln in BufReader::new(file).lines().map_while(Result::ok) {
        if p.first_ts == 0.0 && ln.contains("\"timestamp\"") {
            if let Ok(v) = serde_json::from_str::<Value>(&ln) {
                if let Some(ts) = v.get("timestamp").and_then(|t| t.as_str()) {
                    let t = parse_iso_to_epoch(ts);
                    if t > 0.0 {
                        p.first_ts = t;
                    }
                }
            }
        }
        if !ln.contains("\"usage\"") || !ln.contains("\"assistant\"") {
            continue;
        }
        let v: Value = match serde_json::from_str(&ln) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let msg = match v.get("message") {
            Some(m) if m.is_object() => m,
            _ => continue,
        };
        let mid = match msg.get("id").and_then(|x| x.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        if !seen.insert(mid) {
            continue;
        }
        if p.model.is_empty() {
            if let Some(m) = msg.get("model").and_then(|x| x.as_str()) {
                if !m.is_empty() {
                    p.model = m.to_string();
                }
            }
        }
        if let Some(u) = msg.get("usage") {
            let g = |k: &str| u.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
            p.billed_in += g("input_tokens") + g("cache_creation_input_tokens");
            p.cache_read_in += g("cache_read_input_tokens");
            p.output += g("output_tokens");
        }
        if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
            if let Some(item) = content.last().and_then(|x| x.as_object()) {
                match item.get("type").and_then(|t| t.as_str()) {
                    Some("tool_use") => {
                        p.last_activity = SubagentActivity::ToolUse {
                            name: item
                                .get("name")
                                .and_then(|x| x.as_str())
                                .unwrap_or("")
                                .to_string(),
                            input: item
                                .get("input")
                                .cloned()
                                .unwrap_or(Value::Object(Default::default())),
                        };
                    }
                    Some("thinking") => p.last_activity = SubagentActivity::Thinking,
                    Some("text") => p.last_activity = SubagentActivity::Responding,
                    _ => {}
                }
            }
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::SystemTime;
    use tempfile::tempdir;

    #[test]
    fn missing_subagents_dir_yields_empty() {
        let dir = tempdir().unwrap();
        let r = RunningSubagents::from_session(dir.path(), "session", "/p", 1000.0, 0.0, b"");
        assert!(r.agents.is_empty());
    }

    #[test]
    fn empty_session_or_project_yields_empty() {
        let dir = tempdir().unwrap();
        assert!(
            RunningSubagents::from_session(dir.path(), "", "/p", 0.0, 0.0, b"")
                .agents
                .is_empty()
        );
        assert!(
            RunningSubagents::from_session(dir.path(), "s", "", 0.0, 0.0, b"")
                .agents
                .is_empty()
        );
    }

    #[test]
    fn project_slug_replaces_non_alnum() {
        assert_eq!(
            project_slug("/home/user/my-project"),
            "-home-user-my-project"
        );
        assert_eq!(project_slug("C:\\Users\\Project"), "C--Users-Project");
    }

    #[test]
    fn stale_transcripts_filtered_out() {
        let dir = tempdir().unwrap();
        let slug = project_slug("/p");
        let subagents = dir
            .path()
            .join("projects")
            .join(&slug)
            .join("s")
            .join("subagents");
        std::fs::create_dir_all(&subagents).unwrap();
        let meta = subagents.join("a.meta.json");
        let jsonl = subagents.join("a.jsonl");
        std::fs::write(&meta, r#"{"agentType":"Explore","description":"look"}"#).unwrap();
        std::fs::File::create(&jsonl)
            .unwrap()
            .write_all(b"")
            .unwrap();
        // pretend it's old by passing a future `now` well past the
        // fallback STALE_SECONDS window; no prompt anchor available.
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64()
            + (STALE_SECONDS + 1000.0);
        let r = RunningSubagents::from_session(dir.path(), "s", "/p", now, 0.0, b"");
        assert!(r.agents.is_empty());
    }

    #[test]
    fn loads_recent_subagent() {
        let dir = tempdir().unwrap();
        let slug = project_slug("/p");
        let subagents = dir
            .path()
            .join("projects")
            .join(&slug)
            .join("s")
            .join("subagents");
        std::fs::create_dir_all(&subagents).unwrap();
        let meta = subagents.join("a.meta.json");
        let jsonl = subagents.join("a.jsonl");
        std::fs::write(
            &meta,
            r#"{"agentType":"Explore","description":"look around"}"#,
        )
        .unwrap();
        std::fs::File::create(&jsonl)
            .unwrap()
            .write_all(b"{}")
            .unwrap();
        // file mtime is "now"; we pass a `now` that keeps it inside the
        // STALE_SECONDS window.
        let now = jsonl
            .metadata()
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64()
            + 5.0;
        let r = RunningSubagents::from_session(dir.path(), "s", "/p", now, 0.0, b"");
        assert_eq!(r.agents.len(), 1);
        assert_eq!(r.agents[0].agent_type, "Explore");
        assert_eq!(r.agents[0].description, "look around");
    }

    /// Two subagents: one whose `first_timestamp` predates the last user
    /// prompt (previous cycle) and one whose `first_timestamp` is after it
    /// (current cycle). Only the latter should be retained, regardless of
    /// how old the jsonl mtimes are.
    #[test]
    fn prompt_anchor_keeps_only_current_cycle_agents() {
        let dir = tempdir().unwrap();
        let slug = project_slug("/p");
        let subagents = dir
            .path()
            .join("projects")
            .join(&slug)
            .join("s")
            .join("subagents");
        std::fs::create_dir_all(&subagents).unwrap();

        // Previous-cycle agent.
        std::fs::write(
            subagents.join("old.meta.json"),
            r#"{"agentType":"Explore","description":"old"}"#,
        )
        .unwrap();
        std::fs::File::create(subagents.join("old.jsonl"))
            .unwrap()
            .write_all(
                br#"{"timestamp":"2026-05-27T07:00:00.000Z","message":{}}
"#,
            )
            .unwrap();

        // Current-cycle agent.
        std::fs::write(
            subagents.join("new.meta.json"),
            r#"{"agentType":"Plan","description":"new"}"#,
        )
        .unwrap();
        std::fs::File::create(subagents.join("new.jsonl"))
            .unwrap()
            .write_all(
                br#"{"timestamp":"2026-05-27T07:30:00.000Z","message":{}}
"#,
            )
            .unwrap();

        // Last user prompt: 07:15 — sits between the two agents.
        let last_prompt_ts = crate::data::iso::parse_iso_to_epoch("2026-05-27T07:15:00.000Z");
        // `now` deliberately far in the future so the STALE_SECONDS fallback
        // would have rejected both files — proving the anchor path is what
        // retains the new agent.
        let now = last_prompt_ts + 100_000.0;
        let r = RunningSubagents::from_session(dir.path(), "s", "/p", now, last_prompt_ts, b"");
        assert_eq!(r.agents.len(), 1, "got: {:?}", r.agents);
        assert_eq!(r.agents[0].agent_type, "Plan");
    }

    #[test]
    fn stopped_agent_disappears_before_the_next_prompt() {
        let dir = tempdir().unwrap();
        let subagents = dir
            .path()
            .join("projects")
            .join(project_slug("/p"))
            .join("s")
            .join("subagents");
        fs::create_dir_all(&subagents).unwrap();
        for name in ["agent-finished", "agent-raw", "agent-running"] {
            fs::write(
                subagents.join(format!("{name}.meta.json")),
                r#"{"agentType":"Explore","description":"look"}"#,
            )
            .unwrap();
            fs::write(
                subagents.join(format!("{name}.jsonl")),
                b"{\"timestamp\":\"2026-05-27T07:30:00.000Z\"}\n",
            )
            .unwrap();
        }
        let prompt = parse_iso_to_epoch("2026-05-27T07:15:00.000Z");
        let load =
            || RunningSubagents::from_session(dir.path(), "s", "/p", prompt + 60.0, prompt, b"");
        assert_eq!(load().agents.len(), 3);
        record_hook(dir.path(), "SubagentStop", "s", "finished");
        record_hook(dir.path(), "SubagentStop", "s", "agent-raw");
        assert_eq!(load().agents.len(), 1);
        record_hook(dir.path(), "SubagentStart", "s", "finished");
        record_hook(dir.path(), "SubagentStart", "s", "agent-raw");
        assert_eq!(load().agents.len(), 3);
        record_hook(dir.path(), "SubagentStop", "s", "finished");
        record_hook(dir.path(), "SessionEnd", "s", "");
        assert_eq!(load().agents.len(), 0);
    }

    #[test]
    fn session_end_finishes_agents_until_they_write_again() {
        let dir = tempdir().unwrap();
        let subagents = dir
            .path()
            .join("projects")
            .join(project_slug("/p"))
            .join("s")
            .join("subagents");
        fs::create_dir_all(&subagents).unwrap();
        for id in ["continued", "shutdown", "touched"] {
            fs::write(
                subagents.join(format!("agent-{id}.meta.json")),
                format!(r#"{{"agentType":"Explore","description":"{id}"}}"#),
            )
            .unwrap();
            fs::write(
                subagents.join(format!("agent-{id}.jsonl")),
                b"{\"timestamp\":\"2026-05-27T07:30:00.000Z\"}\n",
            )
            .unwrap();
        }
        record_hook(dir.path(), "SessionEnd", "s", "");
        let end = modified_secs(&session_end_marker(dir.path(), "s").unwrap()).unwrap();
        let write = |id: &str, at: f64, line: bool| {
            let mut f = fs::OpenOptions::new()
                .append(true)
                .open(subagents.join(format!("agent-{id}.jsonl")))
                .unwrap();
            if line {
                let ts = chrono::DateTime::from_timestamp(at as i64, 0)
                    .unwrap()
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
                writeln!(f, "{{\"timestamp\":\"{ts}\"}}").unwrap();
            }
            f.set_modified(UNIX_EPOCH + std::time::Duration::from_secs_f64(at))
                .unwrap();
        };
        write("shutdown", end + 5.0, true);
        write("touched", end + 120.0, false);
        write("continued", end + 120.0, true);

        let prompt = parse_iso_to_epoch("2026-05-27T07:15:00.000Z");
        let running: Vec<String> =
            RunningSubagents::from_session(dir.path(), "s", "/p", prompt + 60.0, prompt, b"")
                .agents
                .into_iter()
                .map(|a| a.description)
                .collect();
        assert_eq!(running, ["continued"]);
    }

    #[test]
    fn notified_task_ids_reads_only_notifications() {
        let transcript = [
            r#"{"type":"user","message":{"role":"user","content":"<task-notification>\n<task-id>a1</task-id>\n<status>completed</status>\n</task-notification>"},"origin":{"kind":"task-notification"},"timestamp":"2026-05-27T08:00:00.000Z"}"#,
            r#"{"type":"attachment","attachment":{"type":"queued_command","prompt":"<task-notification>\n<task-id>a2</task-id>\n</task-notification>","commandMode":"task-notification"},"timestamp":"2026-05-27T08:01:00.000Z"}"#,
            r#"{"type":"user","message":{"role":"user","content":"<task-notification>\n<task-id>a1</task-id>\n<status>completed</status>\n</task-notification>"},"origin":{"kind":"task-notification"},"timestamp":"2026-05-27T09:00:00.000Z"}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"<task-id>a3</task-id>"}]},"timestamp":"2026-05-27T09:01:00.000Z"}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"<task-id>a4</task-id>"}]},"timestamp":"2026-05-27T09:02:00.000Z"}"#,
        ]
        .join("\n");
        assert_eq!(
            notified_task_ids(transcript.as_bytes()),
            HashMap::from([
                (
                    "a1".to_string(),
                    parse_iso_to_epoch("2026-05-27T09:00:00.000Z")
                ),
                (
                    "a2".to_string(),
                    parse_iso_to_epoch("2026-05-27T08:01:00.000Z")
                ),
            ])
        );
    }

    #[test]
    fn notified_agent_is_finished_until_it_writes_again() {
        let dir = tempdir().unwrap();
        let subagents = dir
            .path()
            .join("projects")
            .join(project_slug("/p"))
            .join("s")
            .join("subagents");
        fs::create_dir_all(&subagents).unwrap();
        for (id, start) in [("a1", "07:30"), ("a2", "07:31")] {
            fs::write(
                subagents.join(format!("agent-{id}.meta.json")),
                format!(r#"{{"agentType":"Explore","description":"{id}"}}"#),
            )
            .unwrap();
            fs::write(
                subagents.join(format!("agent-{id}.jsonl")),
                format!("{{\"timestamp\":\"2026-05-27T{start}:00.000Z\"}}\n"),
            )
            .unwrap();
        }
        let prompt = parse_iso_to_epoch("2026-05-27T07:15:00.000Z");
        let transcript = br#"{"type":"user","message":{"role":"user","content":"<task-notification>\n<task-id>a1</task-id>\n</task-notification>"},"origin":{"kind":"task-notification"},"timestamp":"2026-05-27T07:40:00.000Z"}"#;
        let running = || {
            RunningSubagents::from_session(dir.path(), "s", "/p", prompt + 60.0, prompt, transcript)
                .agents
                .into_iter()
                .map(|a| a.description)
                .collect::<Vec<_>>()
        };
        assert_eq!(running(), ["a2"]);

        let mut continued = fs::OpenOptions::new()
            .append(true)
            .open(subagents.join("agent-a1.jsonl"))
            .unwrap();
        writeln!(continued, "{{\"timestamp\":\"2026-05-27T07:50:00.000Z\"}}").unwrap();
        assert_eq!(running(), ["a1", "a2"]);
    }
}
