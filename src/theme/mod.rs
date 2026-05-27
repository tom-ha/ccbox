//! Theme value type, model-colour table, and built-in theme constants.

use crate::ansi::Rgb;
use crate::render::palette::ModelKind;

pub mod builtin;
pub mod resolve;

/// Per-model identity colours used by the model pill and the burndown trend.
#[derive(Debug, Clone, Copy)]
pub struct ModelColors {
    pub anchor: Rgb,
    pub warm_shift: Rgb,
    pub cool_shift: Rgb,
    pub label: &'static str,
}

/// A theme — every colour slot the renderer touches.
///
/// All ANSI escape slots are pre-built `&'static str` literals so a theme
/// constant can live in `.rodata` and the renderer never allocates to look
/// up a colour.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub name: &'static str,

    // Decorative slots
    pub border: &'static str,
    pub border_off: &'static str,
    pub pwd: &'static str,
    pub branch: &'static str,
    pub commit: &'static str,
    pub session: &'static str,
    pub skills: &'static str,
    pub time: &'static str,
    pub tok: &'static str,
    pub tok_dim: &'static str,
    pub tok_day: &'static str,
    pub tok_day_dim: &'static str,
    pub cost: &'static str,
    pub bar_fill: &'static str,
    pub bar_empty: &'static str,
    pub dim_green: &'static str,
    pub label: &'static str,
    pub ctx: &'static str,
    pub ctx_dim: &'static str,
    pub white_brt: &'static str,
    pub arrow: &'static str,
    pub dirty: &'static str,
    pub icon_path: &'static str,
    pub tok_icon: &'static str,
    pub model: &'static str,

    // Three-step ladder
    pub safe: &'static str,
    pub warn: &'static str,
    pub alert: &'static str,
    pub yellow: &'static str,
    pub tok_arrow: &'static str,

    // Per-model identity (indexed by ModelKind)
    pub models: [ModelColors; 4],

    // Pill foreground — two-sided flip on per-cell luminance
    pub pill_fg_dark: Rgb,
    pub pill_fg_light: Rgb,

    // Gradients
    pub grad_stops: &'static [(f32, Rgb)],
    pub grey_rgb: Rgb,
    pub spark_stops: &'static [(f32, Rgb)],
    pub spec_gradients: &'static [(Rgb, Rgb, Rgb)],
    pub spec_empty_ansi: &'static str,
}

impl Theme {
    /// Look up the per-model identity colours for a given `ModelKind`.
    pub fn model_colors(&self, kind: ModelKind) -> &ModelColors {
        let idx = match kind {
            ModelKind::Opus => 0,
            ModelKind::Sonnet => 1,
            ModelKind::Haiku => 2,
            ModelKind::Other => 3,
        };
        &self.models[idx]
    }
}

/// All built-in themes, in stable registration order.
pub const ALL_THEMES: &[&Theme] = &[
    &builtin::CLAUDE_DARK,
    &builtin::CLAUDE_LIGHT,
    &builtin::CATPPUCCIN_LATTE,
    &builtin::CATPPUCCIN_MOCHA,
];

pub const DEFAULT_THEME: &Theme = &builtin::CLAUDE_DARK;

/// Find a theme by its kebab-case name. Returns `None` if no match.
pub fn by_name(name: &str) -> Option<&'static Theme> {
    for t in ALL_THEMES {
        if t.name == name {
            return Some(*t);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn by_name_finds_dark() {
        assert!(by_name("claude-dark").is_some());
        assert_eq!(by_name("claude-dark").unwrap().name, "claude-dark");
    }

    #[test]
    fn by_name_finds_all_builtins() {
        for t in ALL_THEMES {
            assert_eq!(by_name(t.name).map(|x| x.name), Some(t.name));
        }
    }

    #[test]
    fn by_name_returns_none_for_unknown() {
        assert!(by_name("nope").is_none());
    }

    #[test]
    fn default_is_claude_dark() {
        assert_eq!(DEFAULT_THEME.name, "claude-dark");
    }

    #[test]
    fn model_colors_indexable() {
        let t = &builtin::CLAUDE_DARK;
        // Opus dark theme: anchor yellow (255, 255, 0)
        assert_eq!(t.model_colors(ModelKind::Opus).anchor, (255, 255, 0));
        // Sonnet: anchor (135, 215, 135)
        assert_eq!(t.model_colors(ModelKind::Sonnet).anchor, (135, 215, 135));
    }

    #[test]
    fn ansi_escapes_are_well_formed() {
        for t in ALL_THEMES {
            for s in [
                t.border,
                t.pwd,
                t.branch,
                t.cost,
                t.label,
                t.spec_empty_ansi,
            ] {
                assert!(s.starts_with("\x1b["));
                assert!(s.ends_with('m'));
            }
        }
    }
}
