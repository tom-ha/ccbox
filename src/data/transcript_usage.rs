//! `TranscriptUsage::from_transcript` — sum token counts from a transcript
//! JSONL file.

use std::collections::HashSet;

use serde::Deserialize;
use serde_json::Value;

use crate::data::transcript::lines_containing;

#[derive(Debug, Default, Clone, Copy)]
pub struct TranscriptUsage {
    pub input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Deserialize)]
struct Line {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    usage: Option<Value>,
}

impl TranscriptUsage {
    pub fn billed_in(&self) -> u64 {
        self.input_tokens + self.cache_creation_input_tokens
    }

    pub fn from_transcript(transcript_path: &str) -> Self {
        std::fs::read(transcript_path)
            .map(|buf| Self::from_bytes(&buf))
            .unwrap_or_default()
    }

    /// Sums each assistant message's usage once, however many lines repeat its id.
    pub fn from_bytes(buf: &[u8]) -> Self {
        let mut seen: HashSet<&str> = HashSet::new();
        let mut acc = Self::default();
        let mut owned: Vec<Line> = Vec::new();
        for ln in lines_containing(buf, b"\"usage\"") {
            if memchr::memmem::find(ln.as_bytes(), b"\"assistant\"").is_none() {
                continue;
            }
            if let Ok(line) = serde_json::from_str::<Line>(ln) {
                owned.push(line);
            }
        }
        for line in &owned {
            let Some(mid) = line.message.id.as_ref().and_then(Value::as_str) else {
                continue;
            };
            if mid.is_empty() || !seen.insert(mid) {
                continue;
            }
            let Some(u) = line.message.usage.as_ref().filter(|v| v.is_object()) else {
                continue;
            };
            let g = |k: &str| -> u64 { u.get(k).and_then(Value::as_u64).unwrap_or(0) };
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
