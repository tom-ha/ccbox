use serde_json::Value;

use crate::data::iso::parse_iso_to_epoch;
use crate::data::transcript::lines_containing_rev;

/// Returns the Unix epoch seconds of the most recent real user prompt in
/// `transcript_path`, or `0.0` if no prompt is found (or the file is
/// missing/empty/unreadable).
pub fn last_user_prompt_ts(transcript_path: &str) -> f64 {
    std::fs::read(transcript_path)
        .map(|buf| last_user_prompt_ts_in(&buf))
        .unwrap_or(0.0)
}

/// Scans from the end, so it reads only back to the most recent prompt.
pub fn last_user_prompt_ts_in(buf: &[u8]) -> f64 {
    for ln in lines_containing_rev(buf, b"\"type\":\"user\"") {
        let v: Value = match serde_json::from_str(ln) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("type").and_then(|x| x.as_str()) != Some("user") {
            continue;
        }
        if v.get("isMeta").and_then(|x| x.as_bool()) == Some(true)
            || v.get("isCompactSummary").and_then(|x| x.as_bool()) == Some(true)
        {
            continue;
        }
        // A finished background agent's notification is a string-content user
        // line too; moving the boundary to it would hide its running siblings.
        let origin = v
            .get("origin")
            .and_then(|o| o.get("kind"))
            .and_then(|x| x.as_str());
        if origin.is_some_and(|kind| kind != "human") {
            continue;
        }
        let is_real_prompt = match v.get("message").and_then(|m| m.get("content")) {
            // Agent-team messages carry no `origin`; the envelope is the only marker.
            Some(Value::String(s)) => origin.is_some() || !s.contains("<teammate-message"),
            Some(Value::Array(arr)) => !arr
                .iter()
                .any(|item| item.get("type").and_then(|t| t.as_str()) == Some("tool_result")),
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
        if ts > 0.0 {
            return ts;
        }
    }
    0.0
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;
    use std::path::Path;

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

    fn anchor_of(lines: &[&str]) -> f64 {
        last_user_prompt_ts_in(lines.join("\n").as_bytes())
    }

    const PROMPT_AT_7: &str = r#"{"type":"user","message":{"role":"user","content":"real"},"origin":{"kind":"human"},"promptSource":"typed","timestamp":"2026-05-27T07:00:00.000Z"}"#;

    #[test]
    fn ignores_task_notifications() {
        let ts = anchor_of(&[
            PROMPT_AT_7,
            r#"{"type":"user","message":{"role":"user","content":"<task-notification>\n<task-id>a1</task-id>\n<status>completed</status>\n</task-notification>"},"origin":{"kind":"task-notification","producer":"session-task"},"promptSource":"system","timestamp":"2026-05-27T08:00:00.000Z"}"#,
        ]);
        assert_eq!(ts, parse_iso_to_epoch("2026-05-27T07:00:00.000Z"));
    }

    #[test]
    fn ignores_every_origin_but_human() {
        let ts = anchor_of(&[
            PROMPT_AT_7,
            r#"{"type":"user","message":{"role":"user","content":"2 background agents were stopped by the user."},"origin":{"kind":"task-notification"},"promptSource":"system","timestamp":"2026-05-27T08:00:00.000Z"}"#,
            r#"{"type":"user","message":{"role":"user","content":"hello"},"origin":{"kind":"peer"},"timestamp":"2026-05-27T09:00:00.000Z"}"#,
        ]);
        assert_eq!(ts, parse_iso_to_epoch("2026-05-27T07:00:00.000Z"));
    }

    #[test]
    fn ignores_teammate_messages() {
        let ts = anchor_of(&[
            r#"{"type":"user","message":{"role":"user","content":"real"},"timestamp":"2026-05-27T07:00:00.000Z"}"#,
            r#"{"type":"user","message":{"role":"user","content":"Another Claude session sent a message:\n<teammate-message teammate_id=\"t\">done</teammate-message>"},"timestamp":"2026-05-27T08:00:00.000Z"}"#,
        ]);
        assert_eq!(ts, parse_iso_to_epoch("2026-05-27T07:00:00.000Z"));
    }

    #[test]
    fn human_prompt_quoting_a_teammate_message_still_anchors() {
        let ts = anchor_of(&[
            PROMPT_AT_7,
            r#"{"type":"user","message":{"role":"user","content":"why does <teammate-message> hide agents?"},"origin":{"kind":"human"},"promptSource":"typed","timestamp":"2026-05-27T08:00:00.000Z"}"#,
        ]);
        assert_eq!(ts, parse_iso_to_epoch("2026-05-27T08:00:00.000Z"));
    }

    #[test]
    fn ignores_compact_summaries() {
        let ts = anchor_of(&[
            PROMPT_AT_7,
            r#"{"type":"user","message":{"role":"user","content":"This session is being continued from a previous conversation."},"isCompactSummary":true,"isVisibleInTranscriptOnly":true,"timestamp":"2026-05-27T08:00:00.000Z"}"#,
        ]);
        assert_eq!(ts, parse_iso_to_epoch("2026-05-27T07:00:00.000Z"));
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
