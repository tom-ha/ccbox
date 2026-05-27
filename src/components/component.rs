//! `Component` trait, `ComponentOutput`, and `SeparatorPolicy`.

use crate::layout::RowSpec;

use super::context::ComponentContext;

/// How the composition runner should separate this component's first row
/// from the previous component's last row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeparatorPolicy {
    /// No separator row before this component.
    None,
    /// Dim separator (`RowKind::SeparatorDim`) — today's default between rows.
    Dim,
    /// Strong separator (`RowKind::Separator`) — heavier visual break.
    Strong,
    /// Seam separator (`RowKind::SeparatorSeam`) aligned to the listed column
    /// indices. The runner demotes a requested `Dim`/`Strong` to `Seam` when
    /// the previous content row carried `downs` markers.
    Seam(Vec<i32>),
}

/// What a component returns from `render`: the rows to insert plus the
/// separator policy applied before those rows.
#[derive(Debug, Clone)]
pub struct ComponentOutput {
    pub rows: Vec<RowSpec>,
    pub leading_separator: SeparatorPolicy,
    /// Only meaningful for the top-header component: the styled chip text
    /// the runner copies into `LayoutSpec::top_right_chip`. Empty for any
    /// other component.
    pub top_right_chip: String,
    /// Optional styled chip painted at the left of this component's leading
    /// separator row (e.g. `├ OpenSpec ┄┄┄┤`). Empty for no chip.
    pub leading_left_chip: String,
}

impl ComponentOutput {
    pub fn empty() -> Self {
        Self {
            rows: Vec::new(),
            leading_separator: SeparatorPolicy::None,
            top_right_chip: String::new(),
            leading_left_chip: String::new(),
        }
    }
}

/// A composable statusline row family. Implementors are typically zero-sized
/// types referenced from a `static` so a `Composition` can hold
/// `&'static dyn Component` pointers without allocation.
pub trait Component: Send + Sync {
    /// Stable kebab-case identifier (e.g. `"context-row"`).
    fn id(&self) -> &'static str;

    /// Pure visibility predicate. When `false`, the runner emits no rows
    /// and no separator for this component.
    fn is_visible(&self, ctx: &ComponentContext) -> bool;

    /// Produce the rows this component contributes plus the leading
    /// separator policy.
    fn render(&self, ctx: &ComponentContext) -> ComponentOutput;
}
