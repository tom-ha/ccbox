//! `TasksRow` — inline or board kanban.

use crate::config::TasksView;
use crate::glyphs::{BOLD, RESET};
use crate::layout::RowSpec;

use super::component::{Component, ComponentOutput, SeparatorPolicy};
use super::context::ComponentContext;

pub struct TasksRow;
pub static TASKS_ROW: TasksRow = TasksRow;

impl Component for TasksRow {
    fn id(&self) -> &'static str {
        "tasks-row"
    }

    fn is_visible(&self, ctx: &ComponentContext) -> bool {
        ctx.env.density.includes_tasks() && ctx.data.task_list(ctx).is_visible(ctx.now)
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let r = ctx.renderer;
        let tasks = ctx.data.task_list(ctx);
        let rows = match ctx.env.tasks_view {
            TasksView::Board => r
                .task_board(tasks, ctx.width)
                .into_iter()
                .map(RowSpec::content)
                .collect(),
            TasksView::Inline => vec![RowSpec::content(r.task_row(tasks, ctx.width, 0))],
        };
        let label = r.theme.label;
        let leading_left_chip = format!(" {label}{BOLD}Tasks{RESET} ");
        ComponentOutput {
            rows,
            leading_separator: SeparatorPolicy::Strong,
            top_right_chip: String::new(),
            leading_left_chip,
        }
    }
}
