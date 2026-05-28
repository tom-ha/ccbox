//! `subagent_activity`, `subagent_row` — single-line table row per agent.

use crate::data::running_subagents::{RunningSubagent, SubagentActivity};
use crate::glyphs::{
    BOLD, GLYPH_HOURGLASS, GLYPH_RESPONDING, GLYPH_SUBAGENT_ROW, GLYPH_TASKS, GLYPH_THINKING, RESET,
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
    /// Render the table header row that sits above the per-agent rows.
    /// Each label is left-aligned at the very first cell of its column slot
    /// (under the glyph / leading character of the data row).
    pub fn subagent_header_row(&self, width: i32, type_w: usize, out_w: usize) -> String {
        let t = self.theme;
        let dim = t.ctx_dim;
        let target_w = (width - 4).max(0);

        // Lead column: 3 + type_w cells, header `type` starts at cell 0 so
        // it sits under the `▶` glyph of the data row.
        let type_text = "type";
        let lead_w_usize = (3 + type_w) as usize;
        let type_pad = lead_w_usize.saturating_sub(visible_width(type_text));
        let lead = format!("{dim}{type_text}{}{RESET}", " ".repeat(type_pad));
        let lead_w = lead_w_usize as i32;

        // Right cluster: each glyph cluster is `<glyph> <value>` = 2 + value_w
        // cells; header label sits at cell 0 of the cluster (under the glyph).
        let in_cluster_w = 2 + 5; // "↓ " + 5-wide tok_s
        let out_cluster_w = 2 + out_w.max(1); // "↑ " + out_w
        let dur_cluster_w = 2 + 5; // "⌛ " + 5-wide dur_s
        let in_hdr = format!("{:<w$}", "in", w = in_cluster_w);
        let out_hdr = format!("{:<w$}", "out", w = out_cluster_w);
        let dur_hdr = format!("{:<w$}", "dur", w = dur_cluster_w);
        let right = format!(
            "{dim}{in_hdr}{RESET}  {dim}{out_hdr}{RESET}  {dim}{dur_hdr}{RESET}",
        );
        let right_w = visible_width(&right) as i32;

        // Middle: "description" header, truncated to fit the remaining budget.
        let middle_budget = (target_w - lead_w - right_w - 2).max(0) as usize;
        let mid_label = "description";
        let mid = if visible_width(mid_label) <= middle_budget {
            mid_label.to_string()
        } else {
            truncate_to_width(mid_label, middle_budget)
        };
        let middle = if mid.is_empty() {
            String::new()
        } else {
            format!("{dim}{mid}{RESET}")
        };
        let middle_visible = visible_width(&middle) as i32;

        let pad_n = pad(target_w, lead_w + 2 + middle_visible + right_w).max(1);
        format!("{lead}  {middle}{}{right}", " ".repeat(pad_n))
    }

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
            SubagentActivity::Responding => format!("{GLYPH_RESPONDING} (responding)"),
            SubagentActivity::None => String::new(),
        }
    }

    /// Render a single subagent table row. Columns:
    /// `▶ type  description · activity  ↓ input  ↑ output  ⌛ duration`.
    /// `type_w` and `out_w` are column widths the caller pre-computes across
    /// all subagents so multiple rows align as a table.
    pub fn subagent_row(
        &self,
        sub: &RunningSubagent,
        width: i32,
        type_w: usize,
        out_w: usize,
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
        let out_s = format!("{:>width$}", fmt_tok(sub.output), width = out_w.max(1));

        let ctx_clr = self.risk_zone_color(sub.total_input);
        let c_marker = rainbow_at(idx, 12);
        let type_text = if sub.agent_type.is_empty() {
            "?"
        } else {
            &sub.agent_type
        };
        let t = self.theme;
        let target_w = (width - 4).max(0);

        // Right cluster: ↓ <input>  ↑ <output>  ⌛ <duration>
        let right = format!(
            "{ctx_clr}↓ {tok_s}{RESET}  {}↑ {out_s}{RESET}  {}{GLYPH_HOURGLASS} {dur_s}{RESET}",
            t.ctx_dim, t.ctx,
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
    fn activity_thinking_and_responding() {
        let r = Renderer::default();
        assert!(r
            .subagent_activity(&SubagentActivity::Thinking)
            .contains("(thinking)"));
        assert!(r
            .subagent_activity(&SubagentActivity::Responding)
            .contains("(responding)"));
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
            let s = r.subagent_row(&sub, w, 7, 3, 0, 1_000_000_060.0);
            assert!(!s.contains('\n'), "width={w}: row should be single-line");
        }
    }

    #[test]
    fn subagent_row_drops_tpm_share_model() {
        let r = Renderer::default();
        let sub = RunningSubagent {
            agent_type: "Explore".into(),
            description: "look around".into(),
            model: "claude-sonnet-4-6".into(),
            total_input: 1000,
            output: 200,
            first_timestamp: 1_000_000_000.0,
            last_activity: SubagentActivity::Responding,
            ..Default::default()
        };
        let row = r.subagent_row(&sub, 130, 7, 3, 0, 1_000_000_060.0);
        let plain = crate::ansi::strip_ansi(&row);
        assert!(!plain.contains("t/m"), "tpm should be dropped: {plain:?}");
        assert!(!plain.contains('%'), "share % should be dropped: {plain:?}");
        assert!(
            !plain.to_lowercase().contains("sonnet"),
            "model name should be dropped: {plain:?}",
        );
    }

    #[test]
    fn subagent_row_has_output_column() {
        let r = Renderer::default();
        let sub = RunningSubagent {
            agent_type: "Explore".into(),
            description: "look around".into(),
            total_input: 1000,
            output: 1234,
            first_timestamp: 1_000_000_000.0,
            last_activity: SubagentActivity::Thinking,
            ..Default::default()
        };
        let row = r.subagent_row(&sub, 130, 7, 5, 0, 1_000_000_060.0);
        let plain = crate::ansi::strip_ansi(&row);
        let expected = fmt_tok(1234);
        assert!(
            plain.contains(&expected),
            "output column should render fmt_tok(1234) = {expected:?}: {plain:?}",
        );
        assert!(
            plain.contains('↑'),
            "output column should include the ↑ marker: {plain:?}",
        );
    }

    #[test]
    fn subagent_header_row_columns_align_with_data_row() {
        let r = Renderer::default();
        let sub = RunningSubagent {
            agent_type: "Explore".into(),
            description: "look around".into(),
            total_input: 1000,
            output: 200,
            first_timestamp: 1_000_000_000.0,
            last_activity: SubagentActivity::Thinking,
            ..Default::default()
        };
        let type_w = 7;
        let out_w = 4;
        let width = 130;
        let header = r.subagent_header_row(width, type_w, out_w);
        let row = r.subagent_row(&sub, width, type_w, out_w, 0, 1_000_000_060.0);
        let h_plain = crate::ansi::strip_ansi(&header);
        let r_plain = crate::ansi::strip_ansi(&row);
        for label in ["type", "description", "in", "out", "dur"] {
            assert!(
                h_plain.contains(label),
                "header missing {label:?}: {h_plain:?}",
            );
        }
        // The header row must have the same total visible width as the data
        // row so columns line up vertically when rendered above each other.
        assert_eq!(
            visible_width(&header),
            visible_width(&row),
            "header and data row visible widths differ\nheader: {h_plain:?}\nrow:    {r_plain:?}",
        );
        // New alignment contract: each header label starts at the same
        // visible column as the corresponding data-row glyph.
        let label_col =
            |s: &str, needle: &str| visible_width(&s[..s.rfind(needle).unwrap()]);
        let glyph_col = |s: &str, glyph: char| {
            visible_width(&s[..s.find(glyph).unwrap()])
        };
        assert_eq!(
            label_col(&h_plain, "in"),
            glyph_col(&r_plain, '↓'),
            "header 'in' must start under the ↓ glyph\nheader: {h_plain:?}\nrow:    {r_plain:?}",
        );
        assert_eq!(
            label_col(&h_plain, "out"),
            glyph_col(&r_plain, '↑'),
            "header 'out' must start under the ↑ glyph\nheader: {h_plain:?}\nrow:    {r_plain:?}",
        );
        // "dur" header sits under the ⌛ glyph (\u{f253}).
        assert_eq!(
            label_col(&h_plain, "dur"),
            glyph_col(&r_plain, '\u{f253}'),
            "header 'dur' must start under the ⌛ glyph\nheader: {h_plain:?}\nrow:    {r_plain:?}",
        );
        // "type" header sits under the ▶ glyph at column 0.
        assert_eq!(label_col(&h_plain, "type"), glyph_col(&r_plain, '▶'));
    }

    #[test]
    fn subagent_rows_align_output_column() {
        let r = Renderer::default();
        let sub_small = RunningSubagent {
            agent_type: "Explore".into(),
            description: "first".into(),
            total_input: 1000,
            output: 5,
            first_timestamp: 1_000_000_000.0,
            last_activity: SubagentActivity::Thinking,
            ..Default::default()
        };
        let sub_large = RunningSubagent {
            agent_type: "Explore".into(),
            description: "second".into(),
            total_input: 1000,
            output: 12_000,
            first_timestamp: 1_000_000_000.0,
            last_activity: SubagentActivity::Thinking,
            ..Default::default()
        };
        // Pre-compute out_w the way the component does, so both rows share the
        // same column width.
        let out_w = visible_width(&fmt_tok(sub_small.output))
            .max(visible_width(&fmt_tok(sub_large.output)));
        let row_small = r.subagent_row(&sub_small, 130, 7, out_w, 0, 1_000_000_060.0);
        let row_large = r.subagent_row(&sub_large, 130, 7, out_w, 1, 1_000_000_060.0);
        let plain_small = crate::ansi::strip_ansi(&row_small);
        let plain_large = crate::ansi::strip_ansi(&row_large);
        let pos_small = plain_small
            .find('↑')
            .expect("small row must contain ↑ output marker");
        let pos_large = plain_large
            .find('↑')
            .expect("large row must contain ↑ output marker");
        assert_eq!(
            pos_small, pos_large,
            "↑ output marker must start at the same column across rows\nsmall: {plain_small:?}\nlarge: {plain_large:?}",
        );
    }
}
