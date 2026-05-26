//! `LoadedSkills::from_transcript` — deduped skill loads from a transcript.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Default, Clone)]
pub struct LoadedSkills {
    pub names: Vec<String>,
}

static SKILL_PAT: Lazy<Regex> = Lazy::new(|| Regex::new(r#""name"\s*:\s*"Skill"[^}]*?"skill"\s*:\s*"([^"]+)""#).unwrap());
static READ_PAT: Lazy<Regex> = Lazy::new(|| Regex::new(r#""name"\s*:\s*"Read"[^}]*?"file_path"\s*:\s*"([^"]+)""#).unwrap());
static SKILL_PATH_PAT: Lazy<Regex> = Lazy::new(|| Regex::new(r#"/skills/([^/"]+)/SKILL\.md$"#).unwrap());

impl LoadedSkills {
    pub fn from_transcript(transcript_path: &str) -> Self {
        if transcript_path.is_empty() {
            return Self::default();
        }
        let p = Path::new(transcript_path);
        if !p.is_file() {
            return Self::default();
        }
        let contents = match fs::read_to_string(p) {
            Ok(s) => s,
            Err(_) => return Self::default(),
        };
        let mut seen: HashSet<String> = HashSet::new();
        let mut order: Vec<String> = Vec::new();
        for ln in contents.lines() {
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
        writeln!(f, r#"{{"content":[{{"type":"tool_use","name":"Skill","input":{{"skill":"init"}}}}]}}"#).unwrap();
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
