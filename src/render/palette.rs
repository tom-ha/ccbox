//! Palette helpers: rainbow palette, model key, level thresholds, BG luminance.

/// Rainbow palette of xterm-256 indices used for the title/star effect.
pub const RAINBOW_PALETTE: [u8; 30] = [
    196, 202, 208, 214, 220, 226, 190, 154, 118, 82, 46, 47, 48, 49, 50, 51, 45, 39, 33, 27, 21,
    57, 93, 129, 165, 201, 200, 199, 198, 197,
];

/// Background-luminance threshold above which the pill foreground flips to
/// the dark variant for legibility.
pub const BG_LUM_THRESHOLD: u16 = 110;

/// Categorise a model name into one of four kinds. `to_key()` recovers the
/// lowercase string form (`"opus" | "sonnet" | "haiku" | "other"`) when needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    Opus,
    Sonnet,
    Haiku,
    Other,
}

impl ModelKind {
    pub fn to_key(self) -> &'static str {
        match self {
            ModelKind::Opus => "opus",
            ModelKind::Sonnet => "sonnet",
            ModelKind::Haiku => "haiku",
            ModelKind::Other => "other",
        }
    }
}

pub fn model_key(name: &str) -> ModelKind {
    let m = name.to_ascii_lowercase();
    if m.contains("opus") {
        ModelKind::Opus
    } else if m.contains("sonnet") {
        ModelKind::Sonnet
    } else if m.contains("haiku") {
        ModelKind::Haiku
    } else {
        ModelKind::Other
    }
}

/// Compute the rainbow escape at `(step + offset) % palette_len`.
pub fn rainbow_at(step: usize, offset: usize) -> String {
    let idx = (step + offset) % RAINBOW_PALETTE.len();
    let color = RAINBOW_PALETTE[idx];
    format!("\x1b[38;5;{color}m")
}

/// Effort-level thresholds (used by the model-effort pill).
#[derive(Debug, Clone, Copy)]
pub struct LevelPct;

impl LevelPct {
    pub const LOW: u16 = 30;
    pub const MEDIUM: u16 = 55;
    pub const HIGH: u16 = 80;
    pub const XHIGH: u16 = 100;
    pub const MAX: u16 = 140;

    /// Look up a level by its lowercase key. Returns `0` for unknown levels.
    pub fn lookup(level: &str) -> u16 {
        match level.to_ascii_lowercase().as_str() {
            "low" => Self::LOW,
            "medium" => Self::MEDIUM,
            "high" => Self::HIGH,
            "xhigh" => Self::XHIGH,
            "max" => Self::MAX,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rainbow_at_returns_palette_entry() {
        let step = 5;
        let offset = 3;
        let idx = (step + offset) % RAINBOW_PALETTE.len();
        let expected = format!("\x1b[38;5;{}m", RAINBOW_PALETTE[idx]);
        assert_eq!(rainbow_at(step, offset), expected);
    }

    #[test]
    fn rainbow_at_zero_offset() {
        let expected = format!("\x1b[38;5;{}m", RAINBOW_PALETTE[0]);
        assert_eq!(rainbow_at(0, 0), expected);
    }

    #[test]
    fn rainbow_at_wraps_around() {
        let palette_len = RAINBOW_PALETTE.len();
        let step = palette_len - 1;
        let offset = 2;
        let idx = (step + offset) % palette_len;
        let expected = format!("\x1b[38;5;{}m", RAINBOW_PALETTE[idx]);
        assert_eq!(rainbow_at(step, offset), expected);
    }

    #[test]
    fn model_key_recognises_opus() {
        assert_eq!(model_key("claude-opus-4-7"), ModelKind::Opus);
    }
    #[test]
    fn model_key_recognises_sonnet() {
        assert_eq!(model_key("claude-sonnet-4-6"), ModelKind::Sonnet);
    }
    #[test]
    fn model_key_recognises_haiku() {
        assert_eq!(model_key("claude-haiku-4-5"), ModelKind::Haiku);
    }
    #[test]
    fn model_key_other_for_unknown() {
        assert_eq!(model_key("gpt-4o"), ModelKind::Other);
    }
    #[test]
    fn model_key_case_insensitive() {
        assert_eq!(model_key("CLAUDE-OPUS"), ModelKind::Opus);
    }

    #[test]
    fn level_pct_lookup() {
        assert_eq!(LevelPct::lookup("low"), 30);
        assert_eq!(LevelPct::lookup("MEDIUM"), 55);
        assert_eq!(LevelPct::lookup("high"), 80);
        assert_eq!(LevelPct::lookup("xhigh"), 100);
        assert_eq!(LevelPct::lookup("max"), 140);
        assert_eq!(LevelPct::lookup("unknown"), 0);
    }
}
