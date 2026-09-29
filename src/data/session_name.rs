use crate::data::transcript::lines_containing;

pub fn from_transcript(transcript_path: &str) -> Option<String> {
    from_bytes(&std::fs::read(transcript_path).ok()?)
}

/// `custom-title` records come from `/rename`. `ai-title` is skipped because
/// nearly every session has one.
pub fn from_bytes(buf: &[u8]) -> Option<String> {
    let last = |prefix: &str, key: &str| {
        lines_containing(buf, prefix.as_bytes())
            .filter(|ln| ln.starts_with(prefix))
            .filter_map(|ln| {
                serde_json::from_str::<serde_json::Value>(ln)
                    .ok()?
                    .get(key)?
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
            })
            .last()
    };
    last(r#"{"type":"custom-title""#, "customTitle")
        .or_else(|| last(r#"{"type":"agent-name""#, "agentName"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn transcript(lines: &[&str]) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        f
    }

    fn name(f: &tempfile::NamedTempFile) -> Option<String> {
        from_transcript(f.path().to_str().unwrap())
    }

    #[test]
    fn latest_custom_title_wins() {
        let f = transcript(&[
            r#"{"type":"agent-name","agentName":"probe","sessionId":"s"}"#,
            r#"{"type":"custom-title","customTitle":"first","sessionId":"s"}"#,
            r#"{"type":"user","message":{"content":"hi"}}"#,
            r#"{"type":"custom-title","customTitle":"second","sessionId":"s"}"#,
        ]);
        assert_eq!(name(&f).as_deref(), Some("second"));
    }

    #[test]
    fn falls_back_to_agent_name() {
        let f = transcript(&[r#"{"type":"agent-name","agentName":"probe","sessionId":"s"}"#]);
        assert_eq!(name(&f).as_deref(), Some("probe"));
    }

    #[test]
    fn ai_title_alone_is_not_a_name() {
        let f = transcript(&[r#"{"type":"ai-title","aiTitle":"Auto","sessionId":"s"}"#]);
        assert_eq!(name(&f), None);
    }

    #[test]
    fn missing_transcript_has_no_name() {
        assert_eq!(from_transcript(""), None);
    }
}
