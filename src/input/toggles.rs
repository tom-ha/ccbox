//! Per-session runtime toggles at `<claude_dir>/ccbox-toggles.json`.
//!
//! The file is the most-recently-set visibility override for the tasks and
//! subagents rows; both fields are optional, unknown keys are ignored, and
//! every failure mode (missing file, parse error, IO error) collapses to
//! "no overrides" silently. The `--snapshot` output and `ccbox toggle status`
//! both attribute decisions back to this file when its fields are set.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// Monotonic per-process counter so concurrent writers from the same PID
/// don't collide on the tempfile name.
static TEMPFILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The on-disk shape of `ccbox-toggles.json`. Both fields are optional;
/// `Some(v)` forces the row to render iff `v`, `None` falls through to the
/// env var (and then the density preset). Unknown JSON keys are ignored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Toggles {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_tasks: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_subagents: Option<bool>,
}

/// Path of the state file inside `claude_dir`.
pub fn state_file_path(claude_dir: &Path) -> PathBuf {
    claude_dir.join("ccbox-toggles.json")
}

/// Load the state file, returning `Toggles::default()` for every failure
/// mode: missing file, empty file, malformed JSON, permission errors.
/// Never panics, never writes to stderr.
pub fn load(claude_dir: &Path) -> Toggles {
    let path = state_file_path(claude_dir);
    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Toggles::default(),
    };
    if raw.trim().is_empty() {
        return Toggles::default();
    }
    serde_json::from_str(&raw).unwrap_or_default()
}

/// Atomically persist `toggles` to the state file (tempfile + rename, same
/// pattern as the GitInfo cache). Returns the IO error from the underlying
/// `write`/`rename` if any step fails.
pub fn save(claude_dir: &Path, toggles: &Toggles) -> io::Result<()> {
    fs::create_dir_all(claude_dir)?;
    let final_path = state_file_path(claude_dir);
    let seq = TEMPFILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_path = claude_dir.join(format!(
        ".ccbox-toggles.{}.{}.tmp",
        std::process::id(),
        seq
    ));
    let body = serde_json::to_string_pretty(toggles)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    {
        let mut f = fs::File::create(&tmp_path)?;
        f.write_all(body.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_all().ok();
    }
    fs::rename(&tmp_path, &final_path)
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::thread;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn load_missing_file_is_default() {
        let dir = tempdir().unwrap();
        let t = load(dir.path());
        assert_eq!(t, Toggles::default());
        assert_eq!(t.show_tasks, None);
        assert_eq!(t.show_subagents, None);
    }

    #[test]
    fn load_empty_file_is_default() {
        let dir = tempdir().unwrap();
        std::fs::write(state_file_path(dir.path()), "").unwrap();
        assert_eq!(load(dir.path()), Toggles::default());
    }

    #[test]
    fn load_whitespace_file_is_default() {
        let dir = tempdir().unwrap();
        std::fs::write(state_file_path(dir.path()), "   \n\t  ").unwrap();
        assert_eq!(load(dir.path()), Toggles::default());
    }

    #[test]
    fn load_malformed_json_is_default() {
        let dir = tempdir().unwrap();
        std::fs::write(state_file_path(dir.path()), "{ not json").unwrap();
        assert_eq!(load(dir.path()), Toggles::default());
    }

    #[test]
    fn load_one_field_leaves_other_none() {
        let dir = tempdir().unwrap();
        std::fs::write(state_file_path(dir.path()), r#"{"show_tasks": false}"#).unwrap();
        let t = load(dir.path());
        assert_eq!(t.show_tasks, Some(false));
        assert_eq!(t.show_subagents, None);
    }

    #[test]
    fn load_both_fields() {
        let dir = tempdir().unwrap();
        std::fs::write(
            state_file_path(dir.path()),
            r#"{"show_tasks": true, "show_subagents": false}"#,
        )
        .unwrap();
        let t = load(dir.path());
        assert_eq!(t.show_tasks, Some(true));
        assert_eq!(t.show_subagents, Some(false));
    }

    #[test]
    fn load_unknown_keys_are_ignored() {
        let dir = tempdir().unwrap();
        std::fs::write(
            state_file_path(dir.path()),
            r#"{"show_tasks": true, "future_field": 42}"#,
        )
        .unwrap();
        assert_eq!(load(dir.path()).show_tasks, Some(true));
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempdir().unwrap();
        let t = Toggles {
            show_tasks: Some(false),
            show_subagents: Some(true),
        };
        save(dir.path(), &t).unwrap();
        assert_eq!(load(dir.path()), t);
    }

    #[test]
    fn save_omits_none_fields() {
        let dir = tempdir().unwrap();
        let t = Toggles {
            show_tasks: Some(false),
            show_subagents: None,
        };
        save(dir.path(), &t).unwrap();
        let raw = std::fs::read_to_string(state_file_path(dir.path())).unwrap();
        assert!(
            raw.contains("show_tasks"),
            "expected show_tasks in serialized form: {raw}"
        );
        assert!(
            !raw.contains("show_subagents"),
            "expected show_subagents omitted when None: {raw}"
        );
    }

    #[test]
    fn save_is_atomic_no_partial_files_remain() {
        let dir = tempdir().unwrap();
        let t = Toggles {
            show_tasks: Some(true),
            ..Default::default()
        };
        save(dir.path(), &t).unwrap();
        // No tempfile residue.
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert!(
            entries.iter().any(|n| n == "ccbox-toggles.json"),
            "final file missing: {entries:?}",
        );
        assert!(
            !entries.iter().any(|n| n.starts_with(".ccbox-toggles")),
            "tempfile residue: {entries:?}",
        );
    }

    #[test]
    fn save_creates_missing_parent_dir() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("not-yet-created");
        let t = Toggles {
            show_tasks: Some(true),
            ..Default::default()
        };
        save(&nested, &t).unwrap();
        assert!(nested.join("ccbox-toggles.json").exists());
    }

    #[test]
    fn concurrent_saves_never_observe_partial_json() {
        // Smoke test: many writers + many readers, every read either gets
        // Toggles::default() (file absent before first write) or fully-parsed
        // toggles — never a panic, never a partial-JSON load.
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();

        let writers: Vec<_> = (0..4)
            .map(|i| {
                let p = path.clone();
                thread::spawn(move || {
                    for _ in 0..50 {
                        let t = Toggles {
                            show_tasks: Some(i % 2 == 0),
                            show_subagents: Some(i % 2 == 1),
                        };
                        save(&p, &t).unwrap();
                    }
                })
            })
            .collect();

        let readers: Vec<_> = (0..4)
            .map(|_| {
                let p = path.clone();
                thread::spawn(move || {
                    for _ in 0..200 {
                        let _ = load(&p); // must not panic.
                    }
                })
            })
            .collect();

        for w in writers {
            w.join().unwrap();
        }
        for r in readers {
            r.join().unwrap();
        }
    }

    #[test]
    fn load_unreadable_returns_default() {
        // Best-effort permission test: not all platforms support chmod-0
        // semantics in CI sandboxes, so we just exercise the path and assert
        // we don't panic — the function contract is "never panics, never
        // writes to stderr regardless of IO outcome".
        let dir = tempdir().unwrap();
        let path = state_file_path(dir.path());
        // Make the file a directory: `read_to_string` will return Err.
        std::fs::create_dir_all(&path).unwrap();
        let t = load(dir.path());
        assert_eq!(t, Toggles::default());
    }

    #[test]
    fn load_partial_then_full_object() {
        // Common sequence: status file starts with just one field, then both.
        let dir = tempdir().unwrap();
        let mut f = std::fs::File::create(state_file_path(dir.path())).unwrap();
        f.write_all(br#"{"show_tasks": false}"#).unwrap();
        drop(f);
        assert_eq!(
            load(dir.path()),
            Toggles {
                show_tasks: Some(false),
                show_subagents: None,
            }
        );

        save(
            dir.path(),
            &Toggles {
                show_tasks: Some(false),
                show_subagents: Some(true),
            },
        )
        .unwrap();
        assert_eq!(
            load(dir.path()),
            Toggles {
                show_tasks: Some(false),
                show_subagents: Some(true),
            }
        );
    }
}
