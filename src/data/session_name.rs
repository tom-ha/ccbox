use std::fs::File;
use std::io::{BufRead, BufReader};

/// `custom-title` records come from `/rename`. `ai-title` is skipped because
/// nearly every session has one.
pub fn from_transcript(transcript_path: &str) -> Option<String> {
    let file = File::open(transcript_path).ok()?;
    let (mut custom, mut agent) = (None, None);
    for ln in BufReader::new(file).lines().map_while(Result::ok) {
        let (slot, key) = if ln.starts_with(r#"{"type":"custom-title""#) {
            (&mut custom, "customTitle")
        } else if ln.starts_with(r#"{"type":"agent-name""#) {
            (&mut agent, "agentName")
        } else {
            continue;
        };
        if let Some(name) = serde_json::from_str::<serde_json::Value>(&ln)
            .ok()
            .and_then(|v| v.get(key)?.as_str().map(str::trim).map(String::from))
            .filter(|s| !s.is_empty())
        {
            *slot = Some(name);
        }
    }
    custom.or(agent)
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
