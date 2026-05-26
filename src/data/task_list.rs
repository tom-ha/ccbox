//! `Task` and `TaskList::from_transcript` — latest TodoWrite/TaskCreate state
//! from the main transcript.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde_json::Value;

use crate::data::iso::parse_iso_to_epoch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
}

impl TaskStatus {
    pub fn parse(s: &str) -> Self {
        match s {
            "in_progress" | "InProgress" | "in-progress" => Self::InProgress,
            "completed" | "Completed" | "done" => Self::Completed,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: u64,
    pub subject: String,
    pub active_form: String,
    pub status: TaskStatus,
}

#[derive(Debug, Default, Clone)]
pub struct TaskList {
    pub tasks: Vec<Task>,
    pub last_event_ts: f64,
}

const FRESHNESS_CAP: f64 = 120.0;
const GRACE_SECONDS: f64 = 20.0;

impl TaskList {
    pub fn total(&self) -> usize { self.tasks.len() }
    pub fn completed(&self) -> usize {
        self.tasks.iter().filter(|t| t.status == TaskStatus::Completed).count()
    }
    pub fn active(&self) -> Option<&Task> {
        self.tasks.iter().rev().find(|t| t.status == TaskStatus::InProgress)
    }
    pub fn next_pending(&self) -> Option<&Task> {
        self.tasks.iter().find(|t| t.status == TaskStatus::Pending)
    }

    pub fn is_visible(&self, now: f64) -> bool {
        if self.tasks.is_empty() || self.last_event_ts <= 0.0 {
            return false;
        }
        let age = now - self.last_event_ts;
        if age > FRESHNESS_CAP {
            return false;
        }
        if self.completed() == self.total() {
            return age <= GRACE_SECONDS;
        }
        true
    }

    /// Reconstruct the task list from `TaskCreate` / `TaskUpdate` tool-use
    /// events in the transcript.
    pub fn from_session(transcript_path: &str) -> Self {
        if transcript_path.is_empty() {
            return Self::default();
        }
        let p = Path::new(transcript_path);
        if !p.is_file() {
            return Self::default();
        }
        let file = match File::open(p) {
            Ok(f) => f,
            Err(_) => return Self::default(),
        };
        let mut by_id: BTreeMap<u64, Task> = BTreeMap::new();
        let mut next_id: u64 = 1;
        let mut last_ts: f64 = 0.0;
        for ln in BufReader::new(file).lines().map_while(Result::ok) {
            if !ln.contains("\"TaskCreate\"") && !ln.contains("\"TaskUpdate\"") {
                continue;
            }
            let value: Value = match serde_json::from_str(&ln) { Ok(v) => v, Err(_) => continue };
            let ts = parse_iso_to_epoch(value.get("timestamp").and_then(|v| v.as_str()).unwrap_or(""));
            let content = match value.get("message").and_then(|m| m.get("content")) {
                Some(Value::Array(a)) => a.clone(),
                _ => continue,
            };
            for c in content {
                let obj = match c.as_object() { Some(o) => o, None => continue };
                if obj.get("type").and_then(|v| v.as_str()) != Some("tool_use") { continue; }
                let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let inp = obj.get("input").cloned().unwrap_or(Value::Object(Default::default()));
                if name == "TaskCreate" {
                    let subj = inp.get("subject").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let af = inp
                        .get("activeForm").and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| subj.clone());
                    by_id.insert(next_id, Task { id: next_id, subject: subj, active_form: af, status: TaskStatus::Pending });
                    next_id += 1;
                    if ts > last_ts { last_ts = ts; }
                } else if name == "TaskUpdate" {
                    let tid: u64 = match inp.get("taskId") {
                        Some(Value::Number(n)) => n.as_u64().unwrap_or(0),
                        Some(Value::String(s)) => s.parse().unwrap_or(0),
                        _ => 0,
                    };
                    let task = match by_id.get_mut(&tid) { Some(t) => t, None => continue };
                    if let Some(s) = inp.get("status").and_then(|v| v.as_str()) {
                        if matches!(s, "pending" | "in_progress" | "completed") {
                            task.status = TaskStatus::parse(s);
                        }
                    }
                    if let Some(af) = inp.get("activeForm").and_then(|v| v.as_str()) {
                        if !af.is_empty() { task.active_form = af.to_string(); }
                    }
                    if let Some(sj) = inp.get("subject").and_then(|v| v.as_str()) {
                        if !sj.is_empty() { task.subject = sj.to_string(); }
                    }
                    if ts > last_ts { last_ts = ts; }
                }
            }
        }
        let tasks: Vec<Task> = by_id.into_values().collect();
        Self { tasks, last_event_ts: last_ts }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn make(status: TaskStatus, subject: &str) -> Task {
        Task { id: 1, subject: subject.into(), active_form: String::new(), status }
    }

    #[test]
    fn counts() {
        let list = TaskList {
            tasks: vec![
                make(TaskStatus::Completed, "a"),
                make(TaskStatus::InProgress, "b"),
                make(TaskStatus::Pending, "c"),
                make(TaskStatus::Pending, "d"),
            ],
            last_event_ts: 0.0,
        };
        assert_eq!(list.total(), 4);
        assert_eq!(list.completed(), 1);
        assert_eq!(list.active().map(|t| t.subject.as_str()), Some("b"));
        assert_eq!(list.next_pending().map(|t| t.subject.as_str()), Some("c"));
    }

    #[test]
    fn task_status_parse() {
        assert_eq!(TaskStatus::parse("in_progress"), TaskStatus::InProgress);
        assert_eq!(TaskStatus::parse("completed"), TaskStatus::Completed);
        assert_eq!(TaskStatus::parse("pending"), TaskStatus::Pending);
        assert_eq!(TaskStatus::parse("???"), TaskStatus::Pending);
    }

    #[test]
    fn reconstructs_from_transcript() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        // create two tasks
        writeln!(f, r#"{{"timestamp":"2025-01-01T00:00:00Z","message":{{"content":[{{"type":"tool_use","name":"TaskCreate","input":{{"subject":"first","activeForm":"Doing first"}}}}]}}}}"#).unwrap();
        writeln!(f, r#"{{"timestamp":"2025-01-01T00:00:01Z","message":{{"content":[{{"type":"tool_use","name":"TaskCreate","input":{{"subject":"second"}}}}]}}}}"#).unwrap();
        // mark first as completed
        writeln!(f, r#"{{"timestamp":"2025-01-01T00:00:02Z","message":{{"content":[{{"type":"tool_use","name":"TaskUpdate","input":{{"taskId":1,"status":"completed"}}}}]}}}}"#).unwrap();
        // mark second as in_progress
        writeln!(f, r#"{{"timestamp":"2025-01-01T00:00:03Z","message":{{"content":[{{"type":"tool_use","name":"TaskUpdate","input":{{"taskId":2,"status":"in_progress"}}}}]}}}}"#).unwrap();
        let list = TaskList::from_session(path.to_str().unwrap());
        assert_eq!(list.total(), 2);
        assert_eq!(list.completed(), 1);
        assert_eq!(list.active().map(|t| t.subject.as_str()), Some("second"));
        assert!(list.last_event_ts > 0.0);
    }

    #[test]
    fn empty_path_yields_default() {
        let l = TaskList::from_session("");
        assert_eq!(l.total(), 0);
    }
}
