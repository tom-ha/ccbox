//! Persistent marker for "we've seen a populated `rate_limits` payload before".
//!
//! Claude Code only populates `rate_limits.five_hour.resets_at` once usage has
//! been tracked in the current 5-hour window. For a fresh session, a session
//! right after `/clear`, or one rendered during the warmup period after a
//! window reset, the field is zero — and the in-payload signal alone would
//! flip-flop the cost row's visibility. The marker survives those gaps:
//! once any session has seen subscription-shaped `rate_limits`, future
//! renders read the marker and keep cost hidden.

use std::fs;
use std::path::{Path, PathBuf};

const MARKER_FILENAME: &str = "ccbox-subscription";

fn marker_path(claude_dir: &Path) -> PathBuf {
    claude_dir.join(MARKER_FILENAME)
}

/// True if the marker file exists under `claude_dir`. Never panics; an
/// inaccessible `claude_dir` returns `false`.
pub fn exists(claude_dir: &Path) -> bool {
    marker_path(claude_dir).exists()
}

/// Best-effort: create a zero-byte marker file under `claude_dir`. Silently
/// ignores all I/O errors — the in-payload signal still works without the
/// marker, so a read-only `claude_dir` or a race with another invocation
/// must not break the render.
pub fn touch(claude_dir: &Path) {
    let _ = fs::write(marker_path(claude_dir), b"");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn touch_creates_marker_file() {
        let dir = TempDir::new().unwrap();
        assert!(!exists(dir.path()));
        touch(dir.path());
        assert!(exists(dir.path()));
        assert!(dir.path().join(MARKER_FILENAME).is_file());
    }

    #[test]
    fn touch_is_idempotent() {
        let dir = TempDir::new().unwrap();
        touch(dir.path());
        touch(dir.path());
        assert!(exists(dir.path()));
    }

    #[test]
    fn exists_returns_false_when_dir_missing() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("does-not-exist");
        assert!(!exists(&missing));
    }

    #[test]
    fn touch_on_missing_dir_does_not_panic() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("does-not-exist");
        // Should not panic even though the parent dir doesn't exist.
        touch(&missing);
        // And we should still see the no-marker state.
        assert!(!exists(&missing));
    }
}
