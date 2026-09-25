//! Composable statusline components.
//!
//! Built-in components and their density gates:
//!
//! | id                 | shown at densities             | notes                       |
//! |--------------------|--------------------------------|-----------------------------|
//! | `top-header`       | always                         | path/branch/venv/model row  |
//! | `attention-row`    | always, when a session waits   | needs-you badge + others    |
//! | `context-row`      | always                         | context-line / compact form |
//! | `tokens-cost-row`  | always (medium/wide only)      | tokens/cost/usage limits    |
//! | `tasks-row`        | Standard, Verbose              | inline or board kanban      |
//! | `subagents-row`    | Standard, Verbose              | live subagent transcripts   |
//! | `openspec-row`     | Standard, Verbose              | openspec progress bars      |
//! | `plugins-skills-row` | Verbose                      | plugins/skills summary      |
//! | `footer`           | always                         | bottom border               |
//!
//! All built-in components are zero-sized; the registry stores
//! `&'static dyn Component` references to long-lived `static` instances.

pub mod attention_row;
pub mod component;
pub mod composition;
pub mod context;
pub mod context_row;
pub mod footer;
pub mod openspec_row;
pub mod plugins_skills_row;
pub mod render_cache;
pub mod subagents_row;
pub mod tasks_row;
pub mod tokens_cost_row;
pub mod top_header;

pub use component::{Component, ComponentOutput, SeparatorPolicy};
pub use composition::{compose, Composition};
pub use context::ComponentContext;
pub use render_cache::RenderCache;

pub use context_row::{ContextRow, CONTEXT_ROW};
pub use footer::{Footer, FOOTER};
pub use openspec_row::{OpenspecRow, OPENSPEC_ROW};
pub use plugins_skills_row::{PluginsSkillsRow, PLUGINS_SKILLS_ROW};
pub use subagents_row::{SubagentsRow, SUBAGENTS_ROW};
pub use tasks_row::{TasksRow, TASKS_ROW};
pub use tokens_cost_row::{TokensCostRow, TOKENS_COST_ROW};
pub use top_header::{TopHeader, TOP_HEADER};
