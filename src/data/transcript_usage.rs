//! `TranscriptUsage::from_transcript` — sum token counts from a transcript
//! JSONL file.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Default, Clone, Copy)]
pub struct TranscriptUsage {
    pub input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub output_tokens: u64,
}

impl TranscriptUsage {
    pub fn billed_in(&self) -> u64 { self.input_tokens + self.cache_creation_input_tokens }

    pub fn from_transcript(transcript_path: &str) -> Self {
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
        let mut seen: HashSet<String> = HashSet::new();
        let mut acc = Self::default();
        for ln in BufReader::new(file).lines().map_while(Result::ok) {
            // Cheap prefilter — only assistant-with-usage lines matter.
            if memchr::memmem::find(ln.as_bytes(), b"\"usage\"").is_none()
                || memchr::memmem::find(ln.as_bytes(), b"\"assistant\"").is_none()
            {
                continue;
            }
            let value: serde_json::Value = match serde_json::from_str(&ln) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let msg = match value.get("message") {
                Some(serde_json::Value::Object(_)) => &value["message"],
                _ => continue,
            };
            let mid = match msg.get("id").and_then(|v| v.as_str()) {
                Some(s) if !s.is_empty() => s.to_string(),
                _ => continue,
            };
            if !seen.insert(mid) {
                continue;
            }
            let u = match msg.get("usage") {
                Some(v) if v.is_object() => v,
                _ => continue,
            };
            let g = |k: &str| -> u64 { u.get(k).and_then(|v| v.as_u64()).unwrap_or(0) };
            acc.input_tokens += g("input_tokens");
            acc.cache_creation_input_tokens += g("cache_creation_input_tokens");
            acc.cache_read_input_tokens += g("cache_read_input_tokens");
            acc.output_tokens += g("output_tokens");
        }
        acc
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn empty_path_yields_default() {
        let u = TranscriptUsage::from_transcript("");
        assert_eq!(u.input_tokens, 0);
    }

    #[test]
    fn nonexistent_file_yields_default() {
        let u = TranscriptUsage::from_transcript("/nope/nonexistent.jsonl");
        assert_eq!(u.input_tokens, 0);
    }

    #[test]
    fn aggregates_per_message() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, r#"{{"role":"assistant","message":{{"id":"a","usage":{{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":2,"cache_read_input_tokens":3}}}}}}"#).unwrap();
        // duplicate id — should not double-count
        writeln!(f, r#"{{"role":"assistant","message":{{"id":"a","usage":{{"input_tokens":99,"output_tokens":99}}}}}}"#).unwrap();
        writeln!(f, r#"{{"role":"assistant","message":{{"id":"b","usage":{{"input_tokens":5,"output_tokens":7}}}}}}"#).unwrap();
        writeln!(f, r#"{{"role":"user","message":{{"content":"hi"}}}}"#).unwrap();
        let u = TranscriptUsage::from_transcript(path.to_str().unwrap());
        assert_eq!(u.input_tokens, 15);
        assert_eq!(u.output_tokens, 27);
        assert_eq!(u.cache_creation_input_tokens, 2);
        assert_eq!(u.cache_read_input_tokens, 3);
    }
}
