//! `RunningSubagents::from_session` — discover live subagent transcripts.

use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::Value;

use crate::data::iso::parse_iso_to_epoch;

#[derive(Debug, Clone)]
pub enum SubagentActivity {
    ToolUse { name: String, input: Value },
    Thinking,
    Replying,
    None,
}

impl Default for SubagentActivity {
    fn default() -> Self { Self::None }
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

const STALE_SECONDS: f64 = 20.0;

fn project_slug(project_dir: &str) -> String {
    project_dir
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

impl RunningSubagents {
    pub fn from_session(claude_dir: &Path, session_id: &str, project_dir: &str, now: f64) -> Self {
        if session_id.is_empty() || project_dir.is_empty() {
            return Self::default();
        }
        let slug = project_slug(project_dir);
        let subagents_dir = claude_dir.join("projects").join(slug).join(session_id).join("subagents");
        if !subagents_dir.is_dir() {
            return Self::default();
        }
        let mut agents: Vec<RunningSubagent> = Vec::new();
        let entries = match fs::read_dir(&subagents_dir) {
            Ok(e) => e,
            Err(_) => return Self::default(),
        };
        for entry in entries.flatten() {
            let meta_path: PathBuf = entry.path();
            if !meta_path.file_name().and_then(|n| n.to_str()).map(|s| s.ends_with(".meta.json")).unwrap_or(false) {
                continue;
            }
            let (agent_type, description) = match fs::read_to_string(&meta_path) {
                Ok(s) => match serde_json::from_str::<Value>(&s) {
                    Ok(v) => (
                        v.get("agentType").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                        v.get("description").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    ),
                    Err(_) => continue,
                },
                Err(_) => continue,
            };
            let jsonl = meta_path.with_extension("").with_extension("jsonl");
            if !jsonl.is_file() { continue; }
            let mtime = match jsonl.metadata().and_then(|m| m.modified()) {
                Ok(t) => t.duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0),
                Err(_) => continue,
            };
            if now - mtime > STALE_SECONDS { continue; }
            let parsed = parse_subagent_transcript(&jsonl);
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
        agents.sort_by(|a, b| a.first_timestamp.partial_cmp(&b.first_timestamp).unwrap_or(std::cmp::Ordering::Equal));
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
    let mut p = Parsed { billed_in: 0, cache_read_in: 0, output: 0, first_ts: 0.0, model: String::new(), last_activity: SubagentActivity::None };
    let file = match File::open(path) { Ok(f) => f, Err(_) => return p };
    let mut seen: HashSet<String> = HashSet::new();
    for ln in BufReader::new(file).lines().map_while(Result::ok) {
        if p.first_ts == 0.0 && ln.contains("\"timestamp\"") {
            if let Ok(v) = serde_json::from_str::<Value>(&ln) {
                if let Some(ts) = v.get("timestamp").and_then(|t| t.as_str()) {
                    let t = parse_iso_to_epoch(ts);
                    if t > 0.0 { p.first_ts = t; }
                }
            }
        }
        if !ln.contains("\"usage\"") || !ln.contains("\"assistant\"") {
            continue;
        }
        let v: Value = match serde_json::from_str(&ln) { Ok(v) => v, Err(_) => continue };
        let msg = match v.get("message") { Some(m) if m.is_object() => m, _ => continue };
        let mid = match msg.get("id").and_then(|x| x.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        if !seen.insert(mid) { continue; }
        if p.model.is_empty() {
            if let Some(m) = msg.get("model").and_then(|x| x.as_str()) {
                if !m.is_empty() { p.model = m.to_string(); }
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
                            name: item.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                            input: item.get("input").cloned().unwrap_or(Value::Object(Default::default())),
                        };
                    }
                    Some("thinking") => p.last_activity = SubagentActivity::Thinking,
                    Some("text") => p.last_activity = SubagentActivity::Replying,
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
        let r = RunningSubagents::from_session(dir.path(), "session", "/p", 1000.0);
        assert!(r.agents.is_empty());
    }

    #[test]
    fn empty_session_or_project_yields_empty() {
        let dir = tempdir().unwrap();
        assert!(RunningSubagents::from_session(dir.path(), "", "/p", 0.0).agents.is_empty());
        assert!(RunningSubagents::from_session(dir.path(), "s", "", 0.0).agents.is_empty());
    }

    #[test]
    fn project_slug_replaces_non_alnum() {
        assert_eq!(project_slug("/home/user/my-project"), "-home-user-my-project");
        assert_eq!(project_slug("C:\\Users\\Project"), "C--Users-Project");
    }

    #[test]
    fn stale_transcripts_filtered_out() {
        let dir = tempdir().unwrap();
        let slug = project_slug("/p");
        let subagents = dir.path().join("projects").join(&slug).join("s").join("subagents");
        std::fs::create_dir_all(&subagents).unwrap();
        let meta = subagents.join("a.meta.json");
        let jsonl = subagents.join("a.jsonl");
        std::fs::write(&meta, r#"{"agentType":"Explore","description":"look"}"#).unwrap();
        std::fs::File::create(&jsonl).unwrap().write_all(b"").unwrap();
        // pretend it's old by passing a future `now`
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64() + 100.0;
        let r = RunningSubagents::from_session(dir.path(), "s", "/p", now);
        assert!(r.agents.is_empty());
    }

    #[test]
    fn loads_recent_subagent() {
        let dir = tempdir().unwrap();
        let slug = project_slug("/p");
        let subagents = dir.path().join("projects").join(&slug).join("s").join("subagents");
        std::fs::create_dir_all(&subagents).unwrap();
        let meta = subagents.join("a.meta.json");
        let jsonl = subagents.join("a.jsonl");
        std::fs::write(&meta, r#"{"agentType":"Explore","description":"look around"}"#).unwrap();
        std::fs::File::create(&jsonl).unwrap().write_all(b"{}").unwrap();
        // file mtime is "now"; we pass a `now` that keeps it inside the
        // STALE_SECONDS window.
        let now = jsonl.metadata().unwrap().modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_secs_f64() + 5.0;
        let r = RunningSubagents::from_session(dir.path(), "s", "/p", now);
        assert_eq!(r.agents.len(), 1);
        assert_eq!(r.agents[0].agent_type, "Explore");
        assert_eq!(r.agents[0].description, "look around");
    }
}
