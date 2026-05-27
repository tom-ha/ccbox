//! Active-theme resolution: cli > env > `$CLAUDE_DIR/statusline-theme` > default.

use std::path::Path;

use super::{by_name, Theme, DEFAULT_THEME};

/// Resolve the active theme from layered sources.
///
/// `cli_arg` corresponds to the `--theme` flag; `env_var` to a value pulled
/// from the process environment (typically `STATUSLINE_THEME` or whatever
/// the binary chooses). `claude_dir` may be `None` if the caller wants to
/// skip the file lookup.
///
/// Returns `(theme, unknown_name)`: when every source resolves to a name
/// not recognised by [`by_name`], `unknown_name` is `Some(name)` so the
/// caller can emit a diagnostic; otherwise it is `None`.
pub fn resolve_theme(
    cli_arg: Option<&str>,
    env_var: Option<&str>,
    claude_dir: Option<&Path>,
) -> (&'static Theme, Option<String>) {
    let mut last_unknown: Option<String> = None;

    for src in [cli_arg, env_var].into_iter().flatten() {
        let s = src.trim();
        if s.is_empty() {
            continue;
        }
        if let Some(t) = by_name(s) {
            return (t, None);
        }
        last_unknown = Some(s.to_string());
    }

    if let Some(dir) = claude_dir {
        let path = dir.join("statusline-theme");
        if let Ok(contents) = std::fs::read_to_string(&path) {
            let name = contents.trim();
            if !name.is_empty() {
                if let Some(t) = by_name(name) {
                    return (t, None);
                }
                last_unknown = Some(name.to_string());
            }
        }
    }

    (DEFAULT_THEME, last_unknown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn cli_wins_over_env_and_file() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("statusline-theme"), "catppuccin-latte").unwrap();
        let (theme, unknown) = resolve_theme(
            Some("catppuccin-mocha"),
            Some("claude-light"),
            Some(dir.path()),
        );
        assert_eq!(theme.name, "catppuccin-mocha");
        assert!(unknown.is_none());
    }

    #[test]
    fn env_used_when_cli_absent() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("statusline-theme"), "catppuccin-latte").unwrap();
        let (theme, _) = resolve_theme(None, Some("catppuccin-mocha"), Some(dir.path()));
        assert_eq!(theme.name, "catppuccin-mocha");
    }

    #[test]
    fn file_used_when_cli_and_env_absent() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("statusline-theme"), "catppuccin-latte\n").unwrap();
        let (theme, _) = resolve_theme(None, None, Some(dir.path()));
        assert_eq!(theme.name, "catppuccin-latte");
    }

    #[test]
    fn default_when_no_sources() {
        let (theme, unknown) = resolve_theme(None, None, None);
        assert_eq!(theme.name, "claude-dark");
        assert!(unknown.is_none());
    }

    #[test]
    fn unknown_name_falls_through_to_default_and_reports() {
        let (theme, unknown) = resolve_theme(Some("not-a-theme"), None, None);
        assert_eq!(theme.name, "claude-dark");
        assert_eq!(unknown.as_deref(), Some("not-a-theme"));
    }

    #[test]
    fn empty_strings_skipped() {
        let (theme, _) = resolve_theme(Some(""), Some("   "), None);
        assert_eq!(theme.name, "claude-dark");
    }
}
