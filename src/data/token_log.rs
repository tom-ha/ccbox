//! `TokenLog` — read/update `~/.claude/statusline-tokens.log`.
//!
//! Line format:
//!   `<YYYY-MM-DD> <session_id> <day_in> <day_cache_read> <day_out>`
//!
//! Older 4-field lines (no cache column) are accepted on read.

use std::fs;
use std::path::Path;

#[derive(Debug, Default, Clone, Copy)]
pub struct TokenLog {
    pub day_in: u64,
    pub day_cache_read: u64,
    pub day_out: u64,
}

impl TokenLog {
    /// Rewrite the log dropping any line matching `session_id`, append a fresh
    /// entry for today, and return the aggregated day totals.
    pub fn update(
        claude_dir: &Path,
        session_id: &str,
        today: &str,
        total_in: u64,
        cache_read: u64,
        total_out: u64,
    ) -> Self {
        let log = claude_dir.join("statusline-tokens.log");
        let mut lines: Vec<String> = Vec::new();
        if let Ok(contents) = fs::read_to_string(&log) {
            for ln in contents.lines() {
                let parts: Vec<&str> = ln.split_ascii_whitespace().collect();
                if parts.len() >= 2 && parts[1] == session_id {
                    continue;
                }
                lines.push(ln.to_string());
            }
        }
        let any = total_in > 0 || cache_read > 0 || total_out > 0;
        if !session_id.is_empty() && any {
            lines.push(format!(
                "{today} {session_id} {total_in} {cache_read} {total_out}"
            ));
            if let Some(parent) = log.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&log, format!("{}\n", lines.join("\n")));
        }

        let mut totals = Self::default();
        for ln in &lines {
            let parts: Vec<&str> = ln.split_ascii_whitespace().collect();
            if parts.len() < 4 || parts[0] != today {
                continue;
            }
            match parts.len() {
                6 => {
                    totals.day_in += parts[2].parse().unwrap_or(0);
                    totals.day_out += parts[3].parse().unwrap_or(0);
                }
                n if n >= 5 => {
                    totals.day_in += parts[2].parse().unwrap_or(0);
                    totals.day_cache_read += parts[3].parse().unwrap_or(0);
                    totals.day_out += parts[4].parse().unwrap_or(0);
                }
                _ => {
                    totals.day_in += parts[2].parse().unwrap_or(0);
                    totals.day_out += parts[3].parse().unwrap_or(0);
                }
            }
        }
        totals
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn appends_and_aggregates() {
        let dir = tempdir().unwrap();
        let totals = TokenLog::update(dir.path(), "sess1", "2025-01-01", 100, 50, 25);
        assert_eq!(totals.day_in, 100);
        assert_eq!(totals.day_cache_read, 50);
        assert_eq!(totals.day_out, 25);

        // Append a new session — old line should be preserved.
        let totals = TokenLog::update(dir.path(), "sess2", "2025-01-01", 10, 5, 2);
        assert_eq!(totals.day_in, 110);
        assert_eq!(totals.day_out, 27);
    }

    #[test]
    fn rewrites_existing_session() {
        let dir = tempdir().unwrap();
        TokenLog::update(dir.path(), "sess1", "2025-01-01", 100, 0, 25);
        let totals = TokenLog::update(dir.path(), "sess1", "2025-01-01", 200, 0, 50);
        // Old 100/25 should not double-count.
        assert_eq!(totals.day_in, 200);
        assert_eq!(totals.day_out, 50);
    }

    #[test]
    fn empty_session_id_skips_write() {
        let dir = tempdir().unwrap();
        let totals = TokenLog::update(dir.path(), "", "2025-01-01", 100, 0, 25);
        assert_eq!(totals.day_in, 0);
        assert!(!dir.path().join("statusline-tokens.log").exists());
    }

    #[test]
    fn ignores_other_dates() {
        let dir = tempdir().unwrap();
        TokenLog::update(dir.path(), "sess1", "2025-01-01", 100, 0, 25);
        let totals = TokenLog::update(dir.path(), "sess2", "2025-01-02", 10, 0, 2);
        assert_eq!(totals.day_in, 10);
        assert_eq!(totals.day_out, 2);
    }
}
