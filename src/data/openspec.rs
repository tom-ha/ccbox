//! OpenSpec scanner — count `[ ]` / `[x]` lines in every
//! `openspec/changes/*/tasks.md` under the workspace root.

use std::fs;
use std::path::PathBuf;

#[derive(Debug, Default, Clone)]
pub struct OpenSpec {
    pub changes: Vec<(String, u32, u32)>, // (change_name, completed, total)
}

impl OpenSpec {
    pub fn from_cwd(cwd: &str) -> Self {
        let root = match Self::find_root(cwd) {
            Some(r) => r,
            None => return Self::default(),
        };
        let changes_dir = root.join("changes");
        if !changes_dir.is_dir() {
            // Some workspaces use a flat openspec/ — fall back to walking.
            return Self::default();
        }
        let mut out: Vec<(String, u32, u32)> = Vec::new();
        let entries = match fs::read_dir(&changes_dir) {
            Ok(e) => e,
            Err(_) => return Self::default(),
        };
        let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for entry in entries {
            if !entry.is_dir() {
                continue;
            }
            let name = entry
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if name == "archive" {
                continue;
            }
            let tasks = entry.join("tasks.md");
            if !tasks.is_file() {
                continue;
            }
            let text = match fs::read_to_string(&tasks) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let mut done = 0u32;
            let mut open = 0u32;
            for ln in text.lines() {
                let t = ln.trim_start();
                if t.starts_with("- [x]") || t.starts_with("- [X]") {
                    done += 1;
                } else if t.starts_with("- [ ]") {
                    open += 1;
                }
            }
            let total = done + open;
            if total == 0 {
                continue;
            }
            out.push((name, done, total));
        }
        Self { changes: out }
    }

    fn find_root(cwd: &str) -> Option<PathBuf> {
        let mut curr = Some(PathBuf::from(cwd));
        while let Some(c) = curr {
            if c.join("openspec").is_dir() {
                return Some(c.join("openspec"));
            }
            curr = c.parent().map(|p| p.to_path_buf()).filter(|p| p != &c);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::Path;
    use tempfile::tempdir;

    fn make_change(root: &Path, name: &str, body: &str) {
        let p = root.join("openspec").join("changes").join(name);
        std::fs::create_dir_all(&p).unwrap();
        let mut f = std::fs::File::create(p.join("tasks.md")).unwrap();
        write!(f, "{body}").unwrap();
    }

    #[test]
    fn missing_root_yields_empty() {
        let dir = tempdir().unwrap();
        let s = OpenSpec::from_cwd(dir.path().to_str().unwrap());
        assert!(s.changes.is_empty());
    }

    #[test]
    fn counts_done_and_total() {
        let dir = tempdir().unwrap();
        make_change(
            dir.path(),
            "feature-a",
            "- [x] one\n- [x] two\n- [ ] three\n",
        );
        let s = OpenSpec::from_cwd(dir.path().to_str().unwrap());
        assert_eq!(s.changes.len(), 1);
        assert_eq!(s.changes[0].0, "feature-a");
        assert_eq!(s.changes[0].1, 2);
        assert_eq!(s.changes[0].2, 3);
    }

    #[test]
    fn skips_empty_tasks_files() {
        let dir = tempdir().unwrap();
        make_change(dir.path(), "no-tasks", "## intro\n");
        let s = OpenSpec::from_cwd(dir.path().to_str().unwrap());
        assert!(s.changes.is_empty());
    }
}
