//! Responsive layout dispatch + ancillary time helpers.

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Env;
use crate::glyphs::RESET;
use crate::input::session::SessionInfo;
use crate::render::pill::{Edge, Pill};
use crate::render::Renderer;
use crate::theme::Theme;

/// Cap on how much of the transcript we scan before giving up on finding a
/// timestamped record. Real transcripts hit a timestamped event within the
/// first few lines (the `mode` / `permission-mode` / `file-history-snapshot`
/// prelude is short); these bounds just protect us from pathological inputs.
const TRANSCRIPT_SCAN_LINE_LIMIT: usize = 64;
const TRANSCRIPT_SCAN_BYTE_LIMIT: u64 = 64 * 1024;

/// Read the JSONL transcript and return the Unix-seconds value of the first
/// record whose object contains a parseable RFC-3339 `timestamp` field.
///
/// Leading records that lack the field (e.g. `mode`, `permission-mode`,
/// `file-history-snapshot`) are skipped. Malformed lines are skipped, not
/// fatal. Returns `None` if no timestamped record is found within the scan
/// bounds, or if the file cannot be opened.
fn transcript_start_secs(path: &Path) -> Option<f64> {
    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file.take(TRANSCRIPT_SCAN_BYTE_LIMIT));
    for line in reader.lines().take(TRANSCRIPT_SCAN_LINE_LIMIT) {
        let line = match line {
            Ok(l) => l,
            Err(_) => return None,
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let Some(ts) = v.get("timestamp").and_then(|t| t.as_str()) else {
            continue;
        };
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
            return Some(dt.timestamp() as f64 + f64::from(dt.timestamp_subsec_micros()) / 1.0e6);
        }
    }
    None
}

/// Wall-clock-since-session-start formatter — `Mm` or `NhMm`, empty if the
/// transcript is missing/empty or contains no timestamped record.
pub fn session_elapsed(transcript_path: &str, now: Option<f64>) -> String {
    if transcript_path.is_empty() {
        return String::new();
    }
    let p = Path::new(transcript_path);
    if !p.is_file() {
        return String::new();
    }
    let start_secs = match transcript_start_secs(p) {
        Some(s) => s,
        None => return String::new(),
    };
    let now_secs = now.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    });
    let mut secs = (now_secs - start_secs) as i64;
    if secs < 0 {
        secs = 0;
    }
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    if hours > 0 {
        format!("{hours}h{mins}m")
    } else {
        format!("{mins}m")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    TopBorder,
    BottomBorder,
    Separator,
    SeparatorSeam,
    SeparatorDim,
    Content,
}

#[derive(Debug, Clone)]
pub struct RowSpec {
    pub kind: RowKind,
    pub content: String,
    pub bg_lead: String,
    pub bg_trail: String,
    pub pill_flush: bool,
    pub ups: Vec<i32>,
    pub downs: Vec<i32>,
    pub pill: Option<Pill>,
    pub pill_edge: Edge,
    pub right_pill: String,
    /// Left-anchored chip painted on a separator row (e.g. `├ OpenSpec ┄┄┄┤`).
    /// Empty for plain separators. Honoured by Separator / SeparatorDim /
    /// SeparatorSeam rows only.
    pub left_chip: String,
}

impl Default for RowSpec {
    fn default() -> Self {
        Self {
            kind: RowKind::Content,
            content: String::new(),
            bg_lead: String::new(),
            bg_trail: String::new(),
            pill_flush: false,
            ups: Vec::new(),
            downs: Vec::new(),
            pill: None,
            pill_edge: Edge::Bottom,
            right_pill: String::new(),
            left_chip: String::new(),
        }
    }
}

impl RowSpec {
    pub fn new(kind: RowKind) -> Self {
        Self {
            kind,
            ..Default::default()
        }
    }
    pub fn content(s: impl Into<String>) -> Self {
        let mut r = Self::new(RowKind::Content);
        r.content = s.into();
        r
    }
}

#[derive(Debug, Default)]
pub struct LayoutSpec {
    pub width: i32,
    pub fill: f64,
    /// Fully-styled chip painted flush against the top border's right corner.
    /// Empty for a plain top border. Built by `session_id_chip`.
    pub top_right_chip: String,
    pub rows: Vec<RowSpec>,
}

/// `<name>  <session-id>  <clock-glyph> <elapsed>`, skipping empty parts.
pub fn session_id_chip(theme: &Theme, name: &str, session_id: &str, elapsed: &str) -> String {
    use crate::glyphs::{BOLD, GLYPH_CLOCK};
    const MAX_NAME_CHARS: usize = 32;
    let name = if name.chars().count() > MAX_NAME_CHARS {
        let head: String = name.chars().take(MAX_NAME_CHARS - 1).collect();
        format!("{head}…")
    } else {
        name.to_string()
    };
    let parts: Vec<String> = [
        (!name.is_empty()).then(|| format!("{BOLD}{}{name}{RESET}", theme.session)),
        (!session_id.is_empty()).then(|| format!("{}{session_id}{RESET}", theme.session)),
        (!elapsed.is_empty()).then(|| format!("{}{GLYPH_CLOCK} {elapsed}{RESET}", theme.time)),
    ]
    .into_iter()
    .flatten()
    .collect();
    if parts.is_empty() {
        return String::new();
    }
    format!(" {} ", parts.join("  "))
}

/// Render an assembled `LayoutSpec` into one string per row.
pub fn render_layout(spec: &LayoutSpec, r: &Renderer) -> Vec<String> {
    let border = r.border();
    let mut out = Vec::with_capacity(spec.rows.len());
    for row in &spec.rows {
        match row.kind {
            RowKind::TopBorder => out.push(border.border_top(
                spec.width,
                &spec.top_right_chip,
                &row.downs,
                spec.fill,
                row.pill.as_ref(),
            )),
            RowKind::BottomBorder => {
                out.push(border.border_bottom(spec.width, &row.ups, spec.fill))
            }
            RowKind::Separator | RowKind::SeparatorSeam => {
                out.push(border.border_separator(spec.width, &row.ups, spec.fill, &row.left_chip))
            }
            RowKind::SeparatorDim => out.push(border.border_separator_dim(
                spec.width,
                &row.downs,
                &row.ups,
                spec.fill,
                row.pill.as_ref(),
                row.pill_edge,
                &row.left_chip,
            )),
            RowKind::Content => out.push(border.border_line(
                &row.content,
                spec.width,
                spec.fill,
                &row.bg_lead,
                &row.bg_trail,
                row.pill_flush,
                &row.right_pill,
            )),
        }
    }
    out
}

pub fn fill_ratio(session: &SessionInfo) -> f64 {
    session.context_window.used_pct().unwrap_or(0.0) / 100.0
}

/// Top-level entry: pick the right composition, run it, return joined lines.
pub fn render(session: &SessionInfo, env: &Env, width: i32, r: &Renderer) -> String {
    use crate::components::{compose, ComponentContext, Composition, RenderCache};
    use crate::consts::{MEDIUM_WIDTH, MIN_WIDTH, NARROW_WIDTH};
    if width < MIN_WIDTH as i32 {
        return String::new();
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let comp = if width < NARROW_WIDTH as i32 {
        Composition::narrow()
    } else if width < MEDIUM_WIDTH as i32 {
        Composition::medium()
    } else {
        Composition::wide()
    };
    let data = RenderCache::new();
    let ctx = ComponentContext::new(session, env, r, width, now, &data);
    let spec = compose(&comp, &ctx);
    render_layout(&spec, r).join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    /// Format a Unix epoch second value as an RFC-3339 string with no
    /// fractional component, e.g. `1999-09-09T01:46:40Z`. Used by the
    /// transcript tests below.
    fn rfc3339_from_unix(secs: i64) -> String {
        chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
            .unwrap()
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }

    fn write_transcript(path: &Path, contents: &str) {
        File::create(path)
            .unwrap()
            .write_all(contents.as_bytes())
            .unwrap();
    }

    #[test]
    fn empty_path_returns_empty() {
        assert_eq!(session_elapsed("", None), "");
    }

    #[test]
    fn missing_file_returns_empty() {
        let dir = tempdir().unwrap();
        assert_eq!(
            session_elapsed(dir.path().join("nope").to_str().unwrap(), None),
            ""
        );
    }

    #[test]
    fn five_minutes_old() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let now_secs: i64 = 2_000_000_000;
        let start = rfc3339_from_unix(now_secs - 300);
        write_transcript(
            &path,
            &format!(r#"{{"type":"user","timestamp":"{start}"}}{}"#, "\n"),
        );
        assert_eq!(
            session_elapsed(path.to_str().unwrap(), Some(now_secs as f64)),
            "5m"
        );
    }

    #[test]
    fn two_hours_two_minutes_old() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let now_secs: i64 = 2_000_000_000;
        let start = rfc3339_from_unix(now_secs - 7320);
        write_transcript(
            &path,
            &format!(r#"{{"type":"user","timestamp":"{start}"}}{}"#, "\n"),
        );
        assert_eq!(
            session_elapsed(path.to_str().unwrap(), Some(now_secs as f64)),
            "2h2m"
        );
    }

    #[test]
    fn prelude_records_without_timestamp_are_skipped() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let now_secs: i64 = 2_000_000_000;
        let start = rfc3339_from_unix(now_secs - 600);
        let body = format!(
            r#"{{"type":"mode","mode":"normal","sessionId":"abc"}}
{{"type":"permission-mode","permissionMode":"default","sessionId":"abc"}}
{{"type":"file-history-snapshot","messageId":"m","snapshot":{{}},"isSnapshotUpdate":false}}
{{"type":"user","timestamp":"{start}"}}
"#
        );
        write_transcript(&path, &body);
        assert_eq!(
            session_elapsed(path.to_str().unwrap(), Some(now_secs as f64)),
            "10m"
        );
    }

    #[test]
    fn malformed_leading_line_is_skipped() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let now_secs: i64 = 2_000_000_000;
        let start = rfc3339_from_unix(now_secs - 60);
        let body = format!("this is not json\n{{\"type\":\"user\",\"timestamp\":\"{start}\"}}\n");
        write_transcript(&path, &body);
        assert_eq!(
            session_elapsed(path.to_str().unwrap(), Some(now_secs as f64)),
            "1m"
        );
    }

    #[test]
    fn no_timestamped_records_returns_empty() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        write_transcript(
            &path,
            "{\"type\":\"mode\",\"mode\":\"normal\"}\n{\"type\":\"permission-mode\"}\n",
        );
        assert_eq!(
            session_elapsed(path.to_str().unwrap(), Some(2_000_000_000.0)),
            ""
        );
    }

    #[test]
    fn future_timestamp_clamps_to_zero() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let now_secs: i64 = 2_000_000_000;
        let start = rfc3339_from_unix(now_secs + 30);
        write_transcript(
            &path,
            &format!(r#"{{"type":"user","timestamp":"{start}"}}{}"#, "\n"),
        );
        assert_eq!(
            session_elapsed(path.to_str().unwrap(), Some(now_secs as f64)),
            "0m"
        );
    }

    #[test]
    fn session_id_chip_empty_when_inputs_empty() {
        assert_eq!(
            session_id_chip(&crate::theme::builtin::CLAUDE_DARK, "", "", ""),
            ""
        );
    }

    #[test]
    fn session_id_chip_renders_full_id_and_clock_elapsed() {
        let s = session_id_chip(
            &crate::theme::builtin::CLAUDE_DARK,
            "",
            "51e977df-abc123def",
            "1h23m",
        );
        let plain = crate::ansi::strip_ansi(&s);
        // No brackets.
        assert!(!plain.contains('['));
        assert!(!plain.contains(']'));
        // Full id, no ellipsis.
        assert!(plain.contains("51e977df-abc123def"), "plain: {plain}");
        assert!(!plain.contains('…'));
        // Clock glyph + elapsed.
        assert!(plain.contains(crate::glyphs::GLYPH_CLOCK), "plain: {plain}");
        assert!(plain.contains("1h23m"));
    }

    #[test]
    fn session_id_chip_short_id_renders_verbatim() {
        let s = session_id_chip(&crate::theme::builtin::CLAUDE_DARK, "", "abc", "");
        let plain = crate::ansi::strip_ansi(&s);
        assert!(plain.contains("abc"));
        assert!(!plain.contains('…'));
    }

    #[test]
    fn session_id_chip_puts_name_left_of_id() {
        let s = session_id_chip(
            &crate::theme::builtin::CLAUDE_DARK,
            "usage-graph",
            "51e977df",
            "5m",
        );
        let plain = crate::ansi::strip_ansi(&s);
        assert!(plain.starts_with(" usage-graph  51e977df  "), "{plain}");
    }

    #[test]
    fn session_id_chip_truncates_long_names() {
        let long = "a".repeat(50);
        let s = session_id_chip(&crate::theme::builtin::CLAUDE_DARK, &long, "id", "");
        let plain = crate::ansi::strip_ansi(&s);
        assert!(plain.contains(&format!("{}…  id", "a".repeat(31))), "{plain}");
    }
}
