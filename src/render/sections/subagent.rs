//! `subagent_activity`, `subagent_row` — single-line table row per agent.

use crate::data::running_subagents::{RunningSubagent, SubagentActivity};
use crate::glyphs::{
    BOLD, GLYPH_HOURGLASS, GLYPH_REPLYING, GLYPH_SUBAGENT_ROW, GLYPH_TASKS, GLYPH_THINKING, RESET,
};
use crate::render::format::{fmt_dur, fmt_tok};
use crate::render::palette::rainbow_at;
use crate::render::Renderer;
use crate::width::{pad, visible_width};

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
    if n == 0 {
        return String::new();
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

    /// Render a single subagent table row. Columns:
    /// `⫷ type  description · activity  ⌛ tokens  duration`. `type_w` is the
    /// column width caller pre-computes across all subagents so multiple rows
    /// align as a table.
    pub fn subagent_row(
        &self,
        sub: &RunningSubagent,
        width: i32,
        type_w: usize,
        idx: usize,
        now: f64,
    ) -> String {
        let dur = if sub.first_timestamp > 0.0 {
            (now - sub.first_timestamp).max(0.0)
        } else {
            0.0
        };
        let dur_s = format!("{:>5}", fmt_dur(dur));
        let tok_s = format!("{:>5}", fmt_tok(sub.total_input));

        let ctx_clr = self.risk_zone_color(sub.total_input);
        let c_marker = rainbow_at(idx, 12);
        let type_text = if sub.agent_type.is_empty() {
            "?"
        } else {
            &sub.agent_type
        };
        let t = self.theme;
        let target_w = (width - 4).max(0);

        // Right cluster: ⌛ <tokens>   <duration>
        let right = format!(
            "{ctx_clr}{GLYPH_HOURGLASS} {tok_s}{RESET}   {}{dur_s}{RESET}",
            t.ctx,
        );
        let right_w = visible_width(&right) as i32;

        // Left lead: ⫷ <type:type_w>
        let type_pad = type_w.saturating_sub(visible_width(type_text));
        let lead = format!(
            "{c_marker}{BOLD}{GLYPH_SUBAGENT_ROW}{RESET}  {}{type_text}{}{RESET}",
            t.skills,
            " ".repeat(type_pad),
        );
        let lead_w = 3 + type_w as i32;

        // Middle: description and activity. The two columns share whatever
        // budget is left between lead and right cluster.
        let middle_budget = (target_w - lead_w - right_w - 2).max(0) as usize;
        let activity = self.subagent_activity(&sub.last_activity);
        let activity_w = visible_width(&activity);
        let desc_raw = sub.description.as_str();
        let desc_w_raw = visible_width(desc_raw);

        let middle = if middle_budget == 0 {
            String::new()
        } else if activity.is_empty() {
            let desc = truncate_to_width(desc_raw, middle_budget);
            format!("{}{desc}{RESET}", t.ctx)
        } else if desc_raw.is_empty() {
            let act = truncate_to_width(&activity, middle_budget);
            format!("{}{act}{RESET}", t.ctx_dim)
        } else {
            let sep_w = 3; // " · "
            if desc_w_raw + sep_w + activity_w <= middle_budget {
                format!(
                    "{}{desc_raw}{RESET} {}·{RESET} {}{activity}{RESET}",
                    t.ctx, t.label, t.ctx_dim,
                )
            } else {
                // Prefer activity over description when both can't fit.
                let act_budget = activity_w.min(middle_budget.saturating_sub(sep_w + 4));
                let act = truncate_to_width(&activity, act_budget.max(1));
                let act_visible = visible_width(&act);
                let desc_budget = middle_budget.saturating_sub(act_visible + sep_w);
                if desc_budget >= 3 {
                    let desc = truncate_to_width(desc_raw, desc_budget);
                    format!(
                        "{}{desc}{RESET} {}·{RESET} {}{act}{RESET}",
                        t.ctx, t.label, t.ctx_dim,
                    )
                } else {
                    let act = truncate_to_width(&activity, middle_budget);
                    format!("{}{act}{RESET}", t.ctx_dim)
                }
            }
        };

        let middle_visible = visible_width(&middle) as i32;
        let pad = pad(target_w, lead_w + 2 + middle_visible + right_w).max(1);
        format!("{lead}  {middle}{}{right}", " ".repeat(pad))
    }
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
        assert!(r
            .subagent_activity(&SubagentActivity::Thinking)
            .contains("(thinking)"));
        assert!(r
            .subagent_activity(&SubagentActivity::Replying)
            .contains("(replying)"));
        assert!(r.subagent_activity(&SubagentActivity::None).is_empty());
    }

    #[test]
    fn subagent_row_is_single_line() {
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
        for w in [80, 100, 130] {
            let s = r.subagent_row(&sub, w, 7, 0, 1_000_000_060.0);
            assert!(!s.contains('\n'), "width={w}: row should be single-line");
        }
    }

    #[test]
    fn subagent_row_drops_tpm_share_model_output() {
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
        let row = r.subagent_row(&sub, 130, 7, 0, 1_000_000_060.0);
        let plain = crate::ansi::strip_ansi(&row);
        assert!(!plain.contains("t/m"), "tpm should be dropped: {plain:?}");
        assert!(
            !plain.contains('↑'),
            "output marker should be dropped: {plain:?}",
        );
        assert!(!plain.contains('%'), "share % should be dropped: {plain:?}");
        assert!(
            !plain.to_lowercase().contains("sonnet"),
            "model name should be dropped: {plain:?}",
        );
    }
}
