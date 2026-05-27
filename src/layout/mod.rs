//! Responsive layout dispatch + ancillary time helpers.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Env;
use crate::glyphs::RESET;
use crate::input::session::SessionInfo;
use crate::render::pill::{Edge, Pill};
use crate::render::sections::context_bar::effective_soft_limit;
use crate::render::Renderer;
use crate::theme::Theme;

/// Wall-clock-since-transcript-mtime formatter — `Mm` or `NhMm`, empty if
/// the transcript path is missing/empty.
pub fn session_elapsed(transcript_path: &str, now: Option<f64>) -> String {
    if transcript_path.is_empty() {
        return String::new();
    }
    let p = Path::new(transcript_path);
    if !p.is_file() {
        return String::new();
    }
    let mtime = match std::fs::metadata(p).and_then(|m| m.modified()) {
        Ok(t) => t,
        Err(_) => return String::new(),
    };
    let mtime_secs = mtime
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let now_secs = now.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    });
    let mut secs = (now_secs - mtime_secs) as i64;
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

/// Build the styled top-right border chip: `<session-id>  <clock-glyph> <elapsed>`.
///
/// The full session id is rendered (no truncation). The elapsed cluster is
/// prefixed with a clock glyph. Returns empty when both inputs are empty.
pub fn session_id_chip(theme: &Theme, session_id: &str, elapsed: &str) -> String {
    use crate::glyphs::GLYPH_CLOCK;
    if session_id.is_empty() && elapsed.is_empty() {
        return String::new();
    }
    let id_part = if session_id.is_empty() {
        String::new()
    } else {
        format!("{}{session_id}{RESET}", theme.session)
    };
    let time_part = if elapsed.is_empty() {
        String::new()
    } else {
        format!("{}{GLYPH_CLOCK} {elapsed}{RESET}", theme.time)
    };
    match (id_part.is_empty(), time_part.is_empty()) {
        (false, false) => format!(" {id_part}  {time_part} "),
        (false, true) => format!(" {id_part} "),
        (true, false) => format!(" {time_part} "),
        (true, true) => String::new(),
    }
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
            RowKind::Separator | RowKind::SeparatorSeam => out.push(border.border_separator(
                spec.width,
                &row.ups,
                spec.fill,
                &row.left_chip,
            )),
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

/// Compute the `fill` from a session's context-window totals, using the
/// per-session effective limit so the gradient fill scales with the model's
/// reported `context_window_size`.
pub fn fill_ratio(session: &SessionInfo) -> f64 {
    let ctx = &session.context_window;
    let total = ctx.total_input_tokens + ctx.total_output_tokens;
    let limit = effective_soft_limit(ctx.context_window_size);
    ((total as f64) / (limit as f64)).min(1.0)
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
    use std::time::Duration;
    use tempfile::tempdir;

    fn touch_mtime(path: &Path, mtime: SystemTime) {
        let secs = mtime.duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let times = [
            LibcTimeval {
                tv_sec: secs,
                tv_usec: 0,
            },
            LibcTimeval {
                tv_sec: secs,
                tv_usec: 0,
            },
        ];
        let c_path = std::ffi::CString::new(path.as_os_str().to_string_lossy().as_bytes()).unwrap();
        unsafe {
            let r = libc_utimes(c_path.as_ptr(), times.as_ptr());
            assert_eq!(r, 0, "utimes failed");
        }
    }
    #[repr(C)]
    struct LibcTimeval {
        tv_sec: i64,
        tv_usec: i64,
    }
    extern "C" {
        #[link_name = "utimes"]
        fn libc_utimes(path: *const i8, times: *const LibcTimeval) -> i32;
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
        File::create(&path).unwrap().write_all(b"{}").unwrap();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000_000);
        touch_mtime(&path, now - Duration::from_secs(300));
        assert_eq!(
            session_elapsed(path.to_str().unwrap(), Some(2_000_000_000.0)),
            "5m"
        );
    }
    #[test]
    fn two_hours_two_minutes_old() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        File::create(&path).unwrap().write_all(b"{}").unwrap();
        let now_secs: i64 = 2_000_000_000;
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(now_secs as u64);
        touch_mtime(&path, now - Duration::from_secs(7320));
        assert_eq!(
            session_elapsed(path.to_str().unwrap(), Some(now_secs as f64)),
            "2h2m"
        );
    }

    #[test]
    fn session_id_chip_empty_when_inputs_empty() {
        assert_eq!(
            session_id_chip(&crate::theme::builtin::CLAUDE_DARK, "", ""),
            ""
        );
    }

    #[test]
    fn session_id_chip_renders_full_id_and_clock_elapsed() {
        let s = session_id_chip(
            &crate::theme::builtin::CLAUDE_DARK,
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
        let s = session_id_chip(&crate::theme::builtin::CLAUDE_DARK, "abc", "");
        let plain = crate::ansi::strip_ansi(&s);
        assert!(plain.contains("abc"));
        assert!(!plain.contains('…'));
    }
}
