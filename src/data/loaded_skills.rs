//! `LoadedSkills::from_transcript` — deduped skill loads from a transcript.

use std::collections::HashSet;
use std::fs;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::data::transcript::lines_containing;

#[derive(Debug, Default, Clone)]
pub struct LoadedSkills {
    pub names: Vec<String>,
}

static SKILL_PAT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#""name"\s*:\s*"Skill"[^}]*?"skill"\s*:\s*"([^"]+)""#).unwrap());
static READ_PAT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#""name"\s*:\s*"Read"[^}]*?"file_path"\s*:\s*"([^"]+)""#).unwrap());
static SKILL_PATH_PAT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"/skills/([^/"]+)/SKILL\.md$"#).unwrap());

impl LoadedSkills {
    pub fn from_transcript(transcript_path: &str) -> Self {
        fs::read(transcript_path)
            .map(|buf| Self::from_bytes(&buf))
            .unwrap_or_default()
    }

    pub fn from_bytes(buf: &[u8]) -> Self {
        let mut seen: HashSet<String> = HashSet::new();
        let mut order: Vec<String> = Vec::new();
        let mut lines: Vec<&str> = lines_containing(buf, br#""Skill""#)
            .chain(lines_containing(buf, b"SKILL.md"))
            .collect();
        lines.sort_by_key(|l| l.as_ptr());
        lines.dedup_by_key(|l| l.as_ptr());
        for ln in lines {
            if ln.contains(r#""Skill""#) {
                for m in SKILL_PAT.captures_iter(ln) {
                    if let Some(name) = m.get(1) {
                        let n = name.as_str().to_string();
                        if seen.insert(n.clone()) {
                            order.push(n);
                        }
                    }
                }
            }
            if ln.contains(r#""Read""#) && ln.contains("SKILL.md") {
                for m in READ_PAT.captures_iter(ln) {
                    if let Some(fp) = m.get(1) {
                        if let Some(sm) = SKILL_PATH_PAT.captures(fp.as_str()) {
                            if let Some(name) = sm.get(1) {
                                let n = name.as_str().to_string();
                                if seen.insert(n.clone()) {
                                    order.push(n);
                                }
                            }
                        }
                    }
                }
            }
        }
        Self { names: order }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn missing_path_yields_empty() {
        let s = LoadedSkills::from_transcript("/nope/x.jsonl");
        assert!(s.names.is_empty());
    }

    #[test]
    fn picks_up_skill_invocations() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, r#"{{"content":[{{"type":"tool_use","name":"Skill","input":{{"skill":"code-review"}}}}]}}"#).unwrap();
        writeln!(f, r#"{{"content":[{{"type":"tool_use","name":"Skill","input":{{"skill":"code-review"}}}}]}}"#).unwrap();
        writeln!(
            f,
            r#"{{"content":[{{"type":"tool_use","name":"Skill","input":{{"skill":"init"}}}}]}}"#
        )
        .unwrap();
        let s = LoadedSkills::from_transcript(path.to_str().unwrap());
        assert_eq!(s.names, vec!["code-review".to_string(), "init".to_string()]);
    }

    #[test]
    fn picks_up_skill_md_reads() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, r#"{{"content":[{{"type":"tool_use","name":"Read","input":{{"file_path":"/path/skills/format/SKILL.md"}}}}]}}"#).unwrap();
        let s = LoadedSkills::from_transcript(path.to_str().unwrap());
        assert_eq!(s.names, vec!["format".to_string()]);
    }
}
