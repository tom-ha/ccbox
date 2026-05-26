//! Render facade and rendering primitives.
//!
//! [`Renderer`] holds borrowed `&'static Theme` references and lends them
//! to the section renderers in [`sections`]. Themes are set once at
//! construction — no mutable `_apply_theme()` shim.

pub mod border;
pub mod format;
pub mod gradient;
pub mod palette;
pub mod pill;
pub mod sections;

use crate::theme::{Theme, DEFAULT_THEME};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BgShift {
    Warm,
    Cool,
}

/// Cross-section renderer state: the active theme + a chosen pill bg-shift.
/// Borrowed references mean cloning the renderer is essentially free.
pub struct Renderer {
    pub theme: &'static Theme,
    pub bg_shift: BgShift,
}

impl Default for Renderer {
    fn default() -> Self {
        Self { theme: DEFAULT_THEME, bg_shift: BgShift::Warm }
    }
}

impl Renderer {
    pub fn new(theme: &'static Theme, bg_shift: BgShift) -> Self {
        Self { theme, bg_shift }
    }

    pub fn border(&self) -> border::BorderRenderer<'_> {
        border::BorderRenderer::new(self.theme)
    }
    pub fn gradient(&self) -> gradient::GradientEngine<'_> {
        gradient::GradientEngine::new(self.theme)
    }

    /// Vertical separator block embedded in a content row.
    ///
    /// `col` is 1-indexed column of the visible `│`. `leader = true` reserves
    /// one trailing space for a column to the right; `leader = false` two.
    pub fn vsep_block(&self, col: i32, width: i32, fill: f64, leader: bool) -> String {
        let color = self.gradient().grad_at(col - 1, width, 1.0, fill);
        let trailing = if leader { " " } else { "  " };
        format!("  {color}│{}{trailing}", crate::glyphs::RESET)
    }
}
