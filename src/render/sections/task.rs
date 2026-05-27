//! `task_row` (inline kanban) and `task_board` (multi-line kanban).

use crate::data::task_list::{TaskList, TaskStatus};
use crate::glyphs::{BOLD, RESET};
use crate::render::Renderer;
use crate::width::{pad, visible_width};

const BULLET_TODO: &str = "•";
const BULLET_DOING: &str = "▶";
const BULLET_DONE: &str = "✓";
const MAX_BOARD_ROWS: usize = 5;
const BOARD_MIN_WIDTH: i32 = 100;

fn count_status(tasks: &TaskList, status: TaskStatus) -> usize {
    tasks.tasks.iter().filter(|t| t.status == status).count()
}

fn truncate_ellipsis(s: &str, budget: usize) -> String {
    if visible_width(s) <= budget {
        return s.to_string();
    }
    if budget == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut w = 0usize;
    for c in s.chars() {
        let cw = if crate::width::is_wide(c) { 2 } else { 1 };
        if w + cw > budget - 1 {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}

impl Renderer {
    /// Inline kanban single-line row. Pattern:
    /// `todo N   ▶ doing N  <active-subject>   done N ✓`.
    ///
    /// `width` is the box width; the active-subject is truncated to fit, and
    /// dropped entirely at narrow widths.
    pub fn task_row(&self, tasks: &TaskList, width: i32, _step: usize) -> String {
        let t = self.theme;
        let todo = count_status(tasks, TaskStatus::Pending);
        let doing = count_status(tasks, TaskStatus::InProgress);
        let done = count_status(tasks, TaskStatus::Completed);

        let todo_text = format!(" {}todo{RESET} {}{todo}{RESET}", t.label, t.ctx);
        let doing_text = format!(
            "{}{BOLD}{BULLET_DOING}{RESET}  {}doing{RESET} {}{doing}{RESET}",
            t.tok, t.label, t.ctx,
        );
        let done_text = format!(
            "{}done{RESET} {}{done}{RESET} {}{BULLET_DONE}{RESET}",
            t.label, t.ctx, t.dim_green,
        );

        let fixed_w = visible_width(&todo_text) as i32
            + 3 // gap
            + visible_width(&doing_text) as i32
            + 3 // gap
            + visible_width(&done_text) as i32;

        // Content width is width - 3 (lead space + side borders).
        let content_w = width - 3;
        let subject_budget = (content_w - fixed_w - 2).max(0);

        let active = tasks
            .tasks
            .iter()
            .find(|t| t.status == TaskStatus::InProgress);
        let raw_subject = active
            .map(|task| {
                if task.active_form.is_empty() {
                    task.subject.clone()
                } else {
                    task.active_form.clone()
                }
            })
            .unwrap_or_default();

        let show_subject = !raw_subject.is_empty() && subject_budget >= 12 && width >= 60;
        let subject_chunk = if show_subject {
            let truncated = truncate_ellipsis(&raw_subject, subject_budget as usize - 2);
            format!("  {}{truncated}{RESET}", t.ctx)
        } else {
            String::new()
        };

        format!("{todo_text}   {doing_text}{subject_chunk}   {done_text}")
    }

    /// Multi-line kanban board. Returns one string per row (header + up to
    /// `MAX_BOARD_ROWS` content rows). Each row has visible width equal to
    /// `width - 3` (the content width the layout wrapper expects).
    ///
    /// When `width < BOARD_MIN_WIDTH`, returns a single-element vec containing
    /// the inline kanban — callers should treat that as an automatic degrade.
    pub fn task_board(&self, tasks: &TaskList, width: i32) -> Vec<String> {
        if width < BOARD_MIN_WIDTH {
            return vec![self.task_row(tasks, width, 0)];
        }
        let t = self.theme;
        let content_w = width - 3;
        // 2 dividers × ` │ ` (3 cells each) = 6 cells of fixed divider width.
        let total_col_text = (content_w - 6).max(3);
        let col1_w = total_col_text / 3;
        let col2_w = total_col_text / 3;
        let col3_w = total_col_text - col1_w - col2_w;

        let todo: Vec<_> = tasks
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .collect();
        let doing: Vec<_> = tasks
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::InProgress)
            .collect();
        let done: Vec<_> = tasks
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Completed)
            .collect();

        let header = |label: &str, count: usize, w: i32| -> String {
            let raw = format!("{label} {count}");
            let truncated = truncate_ellipsis(&raw, w as usize);
            let pad = pad(w, visible_width(&truncated) as i32);
            format!("{}{BOLD}{truncated}{RESET}{}", t.label, " ".repeat(pad))
        };

        let cell_for = |i: usize,
                        list: &[&crate::data::task_list::Task],
                        col_w: i32,
                        bullet: &str,
                        bullet_clr: &str,
                        text_clr: &str,
                        overflow: usize|
         -> String {
            // Last visible row shows "+N more" when overflow > 0.
            let is_last =
                i + 1 == MAX_BOARD_ROWS.min(list.len() + if overflow > 0 { 1 } else { 0 });
            if overflow > 0 && is_last {
                let raw = format!("+{overflow} more");
                let truncated = truncate_ellipsis(&raw, col_w as usize);
                let pad = pad(col_w, visible_width(&truncated) as i32);
                return format!("{}{truncated}{RESET}{}", t.label, " ".repeat(pad));
            }
            if i >= list.len() {
                return " ".repeat(col_w as usize);
            }
            let task = list[i];
            let subj_src = if task.active_form.is_empty() {
                &task.subject
            } else {
                &task.active_form
            };
            // bullet + space + subject; bullet is 1 cell wide for •/✓, and 1 for ▶ (a triangle).
            let subj_budget = (col_w as usize).saturating_sub(2);
            let subj = truncate_ellipsis(subj_src, subj_budget);
            let cell_raw = format!("{bullet} {subj}");
            let pad = pad(col_w, visible_width(&cell_raw) as i32);
            format!(
                "{bullet_clr}{bullet}{RESET} {text_clr}{subj}{RESET}{}",
                " ".repeat(pad)
            )
        };

        let divider = format!(" {}│{RESET} ", t.label);
        let mut out = Vec::with_capacity(1 + MAX_BOARD_ROWS);

        // Header row
        let h1 = header("TODO", todo.len(), col1_w);
        let h2 = header("DOING", doing.len(), col2_w);
        let h3 = header("DONE", done.len(), col3_w);
        out.push(format!("{h1}{divider}{h2}{divider}{h3}"));

        // How many content rows we'll emit (max of column sizes, capped).
        let row_count = [todo.len(), doing.len(), done.len()]
            .iter()
            .copied()
            .max()
            .unwrap_or(0)
            .min(MAX_BOARD_ROWS);

        let todo_overflow = todo.len().saturating_sub(MAX_BOARD_ROWS);
        let doing_overflow = doing.len().saturating_sub(MAX_BOARD_ROWS);
        let done_overflow = done.len().saturating_sub(MAX_BOARD_ROWS);

        for i in 0..row_count {
            let c1 = cell_for(i, &todo, col1_w, BULLET_TODO, t.dirty, t.ctx, todo_overflow);
            let c2 = cell_for(
                i,
                &doing,
                col2_w,
                BULLET_DOING,
                t.tok,
                t.ctx,
                doing_overflow,
            );
            let c3 = cell_for(
                i,
                &done,
                col3_w,
                BULLET_DONE,
                t.dim_green,
                t.ctx_dim,
                done_overflow,
            );
            out.push(format!("{c1}{divider}{c2}{divider}{c3}"));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::strip_ansi;
    use crate::data::task_list::{Task, TaskStatus};

    fn make(status: TaskStatus, subject: &str) -> Task {
        Task {
            id: 1,
            subject: subject.into(),
            active_form: String::new(),
            status,
        }
    }
    fn make_af(status: TaskStatus, subject: &str, af: &str) -> Task {
        Task {
            id: 1,
            subject: subject.into(),
            active_form: af.into(),
            status,
        }
    }
    fn list(tasks: Vec<Task>) -> TaskList {
        TaskList {
            tasks,
            last_event_ts: 0.0,
        }
    }

    #[test]
    fn task_row_inline_includes_three_count_clusters() {
        let r = Renderer::default();
        let l = list(vec![
            make(TaskStatus::Pending, "a"),
            make(TaskStatus::Pending, "b"),
            make(TaskStatus::InProgress, "c"),
            make(TaskStatus::Completed, "d"),
            make(TaskStatus::Completed, "e"),
            make(TaskStatus::Completed, "f"),
            make(TaskStatus::Completed, "g"),
            make(TaskStatus::Completed, "h"),
        ]);
        let row = r.task_row(&l, 140, 0);
        let plain = strip_ansi(&row);
        assert!(plain.contains("todo 2"), "{plain}");
        assert!(plain.contains("doing 1"), "{plain}");
        assert!(plain.contains("done 5"), "{plain}");
        assert!(plain.contains(BULLET_DOING), "{plain}");
        assert!(plain.contains(BULLET_DONE), "{plain}");
    }

    #[test]
    fn task_row_inline_includes_active_subject() {
        let r = Renderer::default();
        let l = list(vec![make_af(
            TaskStatus::InProgress,
            "ship-the-feature",
            "shipping the feature",
        )]);
        let row = r.task_row(&l, 140, 0);
        let plain = strip_ansi(&row);
        assert!(plain.contains("shipping the feature"), "{plain}");
    }

    #[test]
    fn task_row_truncates_long_subject() {
        let r = Renderer::default();
        let long = "a-very-very-very-very-long-subject-that-will-not-fit-easily";
        let l = list(vec![make_af(TaskStatus::InProgress, long, long)]);
        let row = r.task_row(&l, 60, 0);
        let plain = strip_ansi(&row);
        assert!(
            plain.contains('…'),
            "expected ellipsis when subject too long: {plain}"
        );
    }

    #[test]
    fn task_row_narrow_drops_subject() {
        let r = Renderer::default();
        let l = list(vec![make_af(TaskStatus::InProgress, "alpha", "alpha")]);
        let row = r.task_row(&l, 50, 0);
        let plain = strip_ansi(&row);
        assert!(
            !plain.contains("alpha"),
            "narrow row should not include subject: {plain}"
        );
    }

    #[test]
    fn task_board_returns_multi_row_at_wide_width() {
        let r = Renderer::default();
        let l = list(vec![
            make(TaskStatus::Pending, "cleanup"),
            make(TaskStatus::Pending, "follow-up"),
            make(TaskStatus::InProgress, "ship-it"),
            make(TaskStatus::Completed, "first"),
            make(TaskStatus::Completed, "tests"),
        ]);
        let rows = r.task_board(&l, 140);
        assert!(
            rows.len() >= 3,
            "expected header + content rows: {}",
            rows.len()
        );
        let header_plain = strip_ansi(&rows[0]);
        assert!(header_plain.contains("TODO 2"), "{header_plain}");
        assert!(header_plain.contains("DOING 1"), "{header_plain}");
        assert!(header_plain.contains("DONE 2"), "{header_plain}");
    }

    #[test]
    fn task_board_overflow_indicator() {
        let r = Renderer::default();
        let mut tasks = Vec::new();
        for i in 0..(MAX_BOARD_ROWS + 2) {
            tasks.push(make(TaskStatus::Pending, &format!("task-{i}")));
        }
        let l = list(tasks);
        let rows = r.task_board(&l, 140);
        let last_row_plain = strip_ansi(rows.last().unwrap());
        assert!(last_row_plain.contains("+2 more"), "{last_row_plain}");
    }

    #[test]
    fn task_board_narrow_degrades_to_inline() {
        let r = Renderer::default();
        let l = list(vec![make(TaskStatus::InProgress, "ship-the-feature")]);
        let rows = r.task_board(&l, 80);
        assert_eq!(
            rows.len(),
            1,
            "narrow should auto-degrade to inline: {rows:?}"
        );
    }

    #[test]
    fn task_board_row_widths_match_content_width() {
        let r = Renderer::default();
        let l = list(vec![
            make(TaskStatus::Pending, "a"),
            make(TaskStatus::InProgress, "b"),
            make(TaskStatus::Completed, "c"),
        ]);
        for &width in &[100i32, 140, 200] {
            let rows = r.task_board(&l, width);
            for (i, row) in rows.iter().enumerate() {
                assert_eq!(
                    visible_width(row) as i32,
                    width - 3,
                    "row {i} width mismatch at box width {width}: {:?}",
                    strip_ansi(row),
                );
            }
        }
    }

    #[test]
    fn task_board_empty_columns_render_blank_cells() {
        let r = Renderer::default();
        let l = list(vec![make(TaskStatus::InProgress, "ship")]);
        let rows = r.task_board(&l, 120);
        // Header should still show "TODO 0" and "DONE 0".
        let header_plain = strip_ansi(&rows[0]);
        assert!(header_plain.contains("TODO 0"), "{header_plain}");
        assert!(header_plain.contains("DONE 0"), "{header_plain}");
    }
}
