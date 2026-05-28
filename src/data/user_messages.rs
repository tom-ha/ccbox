//! `last_user_prompt_ts` — Unix epoch seconds of the most recent real user
//! prompt in a Claude Code transcript.
//!
//! The Claude Code transcript stores three kinds of lines with `type:"user"`:
//! 1. A real user prompt — `message.content` is a string.
//! 2. A tool result returned to the model — `message.content` is an array of
//!    `{"type":"tool_result", ...}` items.
//! 3. System / meta entries (e.g. `/clear`) carrying `isMeta:true` or a
//!    `command-name` content.
//!
//! Only the first kind anchors a prompt boundary. The other two ride inside
//! an ongoing prompt-response cycle and should not advance the boundary.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde_json::Value;

use crate::data::iso::parse_iso_to_epoch;

/// Returns the Unix epoch seconds of the most recent real user prompt in
/// `transcript_path`, or `0.0` if no prompt is found (or the file is
/// missing/empty/unreadable).
pub fn last_user_prompt_ts(transcript_path: &str) -> f64 {
    if transcript_path.is_empty() {
        return 0.0;
    }
    let p = Path::new(transcript_path);
    if !p.is_file() {
        return 0.0;
    }
    let file = match File::open(p) {
        Ok(f) => f,
        Err(_) => return 0.0,
    };
    let mut latest = 0.0_f64;
    for ln in BufReader::new(file).lines().map_while(Result::ok) {
        // Cheap pre-filter before paying JSON parse.
        if !ln.contains("\"type\":\"user\"") {
            continue;
        }
        let v: Value = match serde_json::from_str(&ln) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("type").and_then(|x| x.as_str()) != Some("user") {
            continue;
        }
        // Skip meta-flagged entries (e.g. /clear lines).
        if v.get("isMeta").and_then(|x| x.as_bool()) == Some(true) {
            continue;
        }
        let content = v.get("message").and_then(|m| m.get("content"));
        let is_real_prompt = match content {
            Some(Value::String(_)) => true,
            Some(Value::Array(arr)) => {
                // An array content is a tool_result envelope; only treat it as
                // a user prompt if no entry is a tool_result.
                !arr.iter().any(|item| {
                    item.get("type").and_then(|t| t.as_str()) == Some("tool_result")
                })
            }
            _ => false,
        };
        if !is_real_prompt {
            continue;
        }
        let ts = v
            .get("timestamp")
            .and_then(|x| x.as_str())
            .map(parse_iso_to_epoch)
            .unwrap_or(0.0);
        if ts > latest {
            latest = ts;
        }
    }
    latest
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::tempdir;

    use super::*;

    fn write_jsonl(path: &Path, lines: &[&str]) {
        let mut f = File::create(path).unwrap();
        for ln in lines {
            writeln!(f, "{ln}").unwrap();
        }
    }

    #[test]
    fn missing_or_empty_path_returns_zero() {
        assert_eq!(last_user_prompt_ts(""), 0.0);
        assert_eq!(last_user_prompt_ts("/no/such/file"), 0.0);
    }

    #[test]
    fn picks_latest_real_user_prompt() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        write_jsonl(
            &p,
            &[
                r#"{"type":"user","message":{"role":"user","content":"first"},"timestamp":"2026-05-27T07:00:00.000Z"}"#,
                r#"{"type":"assistant","message":{"id":"a"}}"#,
                r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"x"}]},"timestamp":"2026-05-27T07:10:00.000Z"}"#,
                r#"{"type":"user","message":{"role":"user","content":"second"},"timestamp":"2026-05-27T07:20:00.000Z"}"#,
            ],
        );
        let ts = last_user_prompt_ts(p.to_str().unwrap());
        let expected = parse_iso_to_epoch("2026-05-27T07:20:00.000Z");
        assert!((ts - expected).abs() < 0.001, "got {ts}, expected {expected}");
    }

    #[test]
    fn ignores_tool_result_user_lines() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        write_jsonl(
            &p,
            &[
                r#"{"type":"user","message":{"role":"user","content":"only-prompt"},"timestamp":"2026-05-27T07:00:00.000Z"}"#,
                r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"r"}]},"timestamp":"2026-05-27T08:00:00.000Z"}"#,
            ],
        );
        let ts = last_user_prompt_ts(p.to_str().unwrap());
        let expected = parse_iso_to_epoch("2026-05-27T07:00:00.000Z");
        assert!((ts - expected).abs() < 0.001);
    }

    #[test]
    fn ignores_meta_lines() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        write_jsonl(
            &p,
            &[
                r#"{"type":"user","message":{"role":"user","content":"real"},"timestamp":"2026-05-27T07:00:00.000Z"}"#,
                r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"/clear"},"timestamp":"2026-05-27T08:00:00.000Z"}"#,
            ],
        );
        let ts = last_user_prompt_ts(p.to_str().unwrap());
        let expected = parse_iso_to_epoch("2026-05-27T07:00:00.000Z");
        assert!((ts - expected).abs() < 0.001);
    }

    #[test]
    fn no_user_lines_returns_zero() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        write_jsonl(
            &p,
            &[r#"{"type":"assistant","message":{"id":"a"},"timestamp":"2026-05-27T07:00:00.000Z"}"#],
        );
        assert_eq!(last_user_prompt_ts(p.to_str().unwrap()), 0.0);
    }
}
