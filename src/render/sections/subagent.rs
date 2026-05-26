//! `subagent_activity`, `subagent_row` (narrow + wide variants).

use crate::cost::burndown::{subagent_avg_tpm, subagent_share};
use crate::data::running_subagents::{RunningSubagent, SubagentActivity};
use crate::glyphs::{
    BOLD, GLYPH_CONTINUATION, GLYPH_HOURGLASS, GLYPH_PIE, GLYPH_REPLYING, GLYPH_SUBAGENT_ROW,
    GLYPH_TASKS, GLYPH_THINKING, RESET,
};
use crate::render::format::{fmt_dur, fmt_tok};
use crate::render::palette::{model_key, rainbow_at};
use crate::render::Renderer;
use crate::width::visible_width;

/// Maps a tool name to the JSON key whose value is shown next to the tool in
/// the subagent activity line.
fn tool_arg_key(name: &str) -> Option<&'static str> {
    match name {
        "Bash" => Some("command"),
        "Read" | "Edit" | "Write" | "NotebookEdit" => Some("file_path"),
        "Grep" | "Glob" => Some("pattern"),
        "Task" => Some("subagent_type"),
        _ => None,
    }
}

fn truncate_to_width(s: &str, n: usize) -> String {
    if visible_width(s) <= n {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = if crate::width::is_wide(c) { 2 } else { 1 };
        if w + cw >= n {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}

impl Renderer {
    pub fn subagent_activity(&self, activity: &SubagentActivity) -> String {
        match activity {
            SubagentActivity::ToolUse { name, input } => {
                let raw = if let Some(key) = tool_arg_key(name) {
                    if let Some(v) = input.get(key) {
                        let mut s = match v {
                            serde_json::Value::String(s) => s.clone(),
                            _ => v.to_string(),
                        };
                        if key == "file_path" {
                            if let Some(stem) = std::path::Path::new(&s).file_name() {
                                s = stem.to_string_lossy().to_string();
                            }
                        }
                        s
                    } else if let Some((_, v)) = input.as_object().and_then(|m| m.iter().next()) {
                        match v {
                            serde_json::Value::String(s) => s.clone(),
                            _ => v.to_string(),
                        }
                    } else {
                        String::new()
                    }
                } else if let Some((_, v)) = input.as_object().and_then(|m| m.iter().next()) {
                    match v {
                        serde_json::Value::String(s) => s.clone(),
                        _ => v.to_string(),
                    }
                } else {
                    String::new()
                };
                let raw = if visible_width(&raw) > 36 {
                    let mut s: String = raw.chars().take(36).collect();
                    s.push('…');
                    s
                } else {
                    raw
                };
                format!("{GLYPH_TASKS} {name}[{raw}]")
            }
            SubagentActivity::Thinking => format!("{GLYPH_THINKING} (thinking)"),
            SubagentActivity::Replying => format!("{GLYPH_REPLYING} (replying)"),
            SubagentActivity::None => String::new(),
        }
    }

    /// Render a single subagent row. Wide variant (width > 100) emits two
    /// lines joined by `\n`; narrow variant emits one.
    pub fn subagent_row(&self, sub: &RunningSubagent, width: i32, session_inout: i64, step: usize, now: f64) -> String {
        let dur = if sub.first_timestamp > 0.0 {
            (now - sub.first_timestamp).max(0.0)
        } else { 0.0 };
        let dur_s = format!("{:>5}", fmt_dur(dur));
        let out_s = fmt_tok(sub.output);
        let tok_s = fmt_tok(sub.total_input);

        let short_model = model_key(&sub.model).to_key();
        let model_clr = self.model_colour(&sub.model);
        let ctx_clr = self.risk_zone_color(sub.total_input);

        let c_marker = rainbow_at(step, 12);
        let type_text = if sub.agent_type.is_empty() { "?" } else { &sub.agent_type };

        let target_w = (width - 4).max(0);
        let t = self.theme;

        if width > 100 {
            let head1_w = 3 + visible_width(type_text) as i32 + 3;
            let desc_budget = (target_w - head1_w).max(0) as usize;
            let desc_text = if visible_width(&sub.description) > desc_budget {
                if desc_budget == 0 { String::new() } else { truncate_to_width(&sub.description, desc_budget) }
            } else {
                sub.description.clone()
            };
            let left1 = format!(
                "{c_marker}{BOLD}{GLYPH_SUBAGENT_ROW}{RESET}  {}{type_text}{RESET} {}·{RESET} {}{desc_text}{RESET}",
                t.skills, t.label, t.ctx,
            );
            let left1_w = head1_w + visible_width(&desc_text) as i32;
            let pad1 = (target_w - left1_w).max(1) as usize;
            let line1 = format!("{left1}{}", " ".repeat(pad1));

            let tpm = subagent_avg_tpm(sub.total_input, sub.output, sub.first_timestamp, now, 3.0);
            let share = subagent_share((sub.total_input + sub.output) as i64, session_inout);

            let sep = format!(" {}·{RESET} ", t.label);
            let tok_field = format!("{:>5}", fmt_tok(sub.total_input));
            let out_plain = format!("↑ {}", out_s);
            let out_pad = " ".repeat(6usize.saturating_sub(out_plain.chars().count()));

            let tpm_str = tpm.map(|v| format!("{:>5}", format_with_commas(v))).unwrap_or_default();
            let (share_clr, share_str) = match share {
                Some(s) => {
                    let c = self.gradient().gradient_color(s, 1.0);
                    let pct = s * 100.0;
                    (c, format!("{:>6}", format!("{pct:.1}%")))
                }
                None => (String::new(), String::new()),
            };

            let activity = self.subagent_activity(&sub.last_activity);
            let left2_w = 6 + visible_width(&activity) as i32;
            let left2 = format!(
                "   {}{GLYPH_CONTINUATION}{RESET}  {}{activity}{RESET}",
                t.ctx_dim, t.ctx_dim,
            );

            let cluster = |show_tpm: bool, show_share: bool, show_out: bool| -> String {
                let mut frags: Vec<String> = Vec::new();
                if show_tpm && !tpm_str.is_empty() {
                    frags.push(format!("{}{tpm_str}{RESET}{} t/m{RESET}", t.tok, t.label));
                }
                if show_share && !share_str.is_empty() {
                    frags.push(format!("{share_clr}{GLYPH_PIE} {share_str}{RESET}"));
                }
                let mut tok_seg = format!("{ctx_clr}{tok_field}{RESET}");
                if show_out {
                    tok_seg.push_str(&format!(
                        " {out_pad}{}{BOLD}↑ {RESET}{}{}{RESET}",
                        t.label, t.ctx, out_s,
                    ));
                }
                frags.push(tok_seg);
                frags.push(format!("{}{dur_s}{RESET}", t.ctx));
                frags.push(format!("{model_clr}{:>6}{RESET}", short_model));
                frags.join(&sep)
            };

            let mut show_tpm = tpm.is_some();
            let mut show_share = share.is_some();
            let mut show_out = true;
            let fits = |st: bool, sh: bool, so: bool| {
                left2_w + visible_width(&cluster(st, sh, so)) as i32 + 1 <= target_w
            };
            if !fits(show_tpm, show_share, show_out) && show_share { show_share = false; }
            if !fits(show_tpm, show_share, show_out) && show_out { show_out = false; }
            if !fits(show_tpm, show_share, show_out) && show_tpm { show_tpm = false; }
            let right2 = cluster(show_tpm, show_share, show_out);
            let pad2 = (target_w - left2_w - visible_width(&right2) as i32).max(1) as usize;
            let line2 = format!("{left2}{}{right2}", " ".repeat(pad2));
            return format!("{line1}\n{line2}");
        }

        // narrow single-line
        let tool_verb = match &sub.last_activity {
            SubagentActivity::ToolUse { name, .. } => name.clone(),
            SubagentActivity::Thinking => "(thinking)".into(),
            SubagentActivity::Replying => "(replying)".into(),
            SubagentActivity::None => String::new(),
        };
        let right_n = format!(
            "{ctx_clr}{GLYPH_HOURGLASS} {tok_s}{RESET}  {}{BOLD}↑{RESET}{}{out_s}{RESET}  {}{dur_s}{RESET}",
            t.label, t.ctx, t.ctx,
        );
        let right_n_w = visible_width(&right_n) as i32;
        let left_n = format!(
            "{c_marker}{BOLD}{GLYPH_SUBAGENT_ROW}{RESET}  {}{type_text}{RESET}  {model_clr}{short_model}{RESET}  {}{tool_verb}{RESET}",
            t.skills, t.ctx,
        );
        let left_n_w = visible_width(&left_n) as i32;
        let pad_n = (target_w - left_n_w - right_n_w).max(1) as usize;
        format!("{left_n}{}{right_n}", " ".repeat(pad_n))
    }
}

fn format_with_commas(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, &c) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn activity_tool_use_with_file_path_uses_basename() {
        let r = Renderer::default();
        let a = SubagentActivity::ToolUse {
            name: "Read".into(),
            input: json!({"file_path": "/Users/a/projects/foo/bar.rs"}),
        };
        let s = r.subagent_activity(&a);
        assert!(s.contains("bar.rs"), "{s}");
        assert!(!s.contains("/Users"), "{s}");
    }

    #[test]
    fn activity_thinking_and_replying() {
        let r = Renderer::default();
        assert!(r.subagent_activity(&SubagentActivity::Thinking).contains("(thinking)"));
        assert!(r.subagent_activity(&SubagentActivity::Replying).contains("(replying)"));
        assert!(r.subagent_activity(&SubagentActivity::None).is_empty());
    }

    #[test]
    fn subagent_row_narrow_single_line() {
        let r = Renderer::default();
        let sub = RunningSubagent {
            agent_type: "Explore".into(),
            description: "look around".into(),
            model: "claude-sonnet-4-6".into(),
            total_input: 1000,
            output: 200,
            first_timestamp: 1_000_000_000.0,
            last_activity: SubagentActivity::Thinking,
            ..Default::default()
        };
        let s = r.subagent_row(&sub, 80, 5000, 0, 1_000_000_060.0);
        assert!(!s.contains('\n'));
    }

    #[test]
    fn subagent_row_wide_two_lines() {
        let r = Renderer::default();
        let sub = RunningSubagent {
            agent_type: "Explore".into(),
            description: "look around".into(),
            model: "claude-sonnet-4-6".into(),
            total_input: 1000,
            output: 200,
            first_timestamp: 1_000_000_000.0,
            last_activity: SubagentActivity::Replying,
            ..Default::default()
        };
        let s = r.subagent_row(&sub, 130, 5000, 0, 1_000_000_060.0);
        assert!(s.contains('\n'));
    }

    #[test]
    fn format_with_commas_thousands() {
        assert_eq!(format_with_commas(1234), "1,234");
        assert_eq!(format_with_commas(1_234_567), "1,234,567");
        assert_eq!(format_with_commas(0), "0");
        assert_eq!(format_with_commas(999), "999");
    }
}
