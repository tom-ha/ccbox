//! Workspace::plugins — discover enabled plugins from `settings.json`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::input::session::Workspace;

impl Workspace {
    /// Comma-separated list of enabled plugin names. Tolerates missing or
    /// malformed settings files.
    pub fn plugins(&self, claude_dir: &Path) -> String {
        let mut seen: Vec<String> = Vec::new();
        let mut candidates: Vec<PathBuf> = vec![claude_dir.join("settings.json")];
        if !self.project_dir.is_empty() {
            candidates.push(Path::new(&self.project_dir).join(".claude").join("settings.json"));
        }
        for sf in &candidates {
            if !sf.is_file() {
                continue;
            }
            let text = match fs::read_to_string(sf) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let value: serde_json::Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let map = match value.get("enabledPlugins") {
                Some(v) if v.is_object() => v.as_object().unwrap(),
                _ => continue,
            };
            for (key, val) in map {
                if val == &serde_json::Value::Bool(true) {
                    let name = key.split('@').next().unwrap_or("").to_string();
                    if !name.is_empty() && !seen.contains(&name) {
                        seen.push(name);
                    }
                }
            }
        }
        seen.join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn missing_settings_returns_empty() {
        let dir = tempdir().unwrap();
        let w = Workspace::default();
        assert_eq!(w.plugins(dir.path()), "");
    }

    #[test]
    fn malformed_settings_returns_empty() {
        let dir = tempdir().unwrap();
        let mut f = std::fs::File::create(dir.path().join("settings.json")).unwrap();
        write!(f, "not-json").unwrap();
        let w = Workspace::default();
        assert_eq!(w.plugins(dir.path()), "");
    }

    #[test]
    fn collects_enabled_plugins_strips_version() {
        let dir = tempdir().unwrap();
        let mut f = std::fs::File::create(dir.path().join("settings.json")).unwrap();
        write!(f, r#"{{"enabledPlugins":{{"foo@1.0":true,"bar":true,"baz":false}}}}"#).unwrap();
        let w = Workspace::default();
        let plugins = w.plugins(dir.path());
        assert!(plugins.contains("foo"));
        assert!(plugins.contains("bar"));
        assert!(!plugins.contains("baz"));
    }
}
