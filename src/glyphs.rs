//! Nerd Font codepoints, box-drawing glyphs, pill chars, and bar chars.
//!
//! Exposed as `&'static str` so callers can concatenate with `push_str` and
//! `format!` without converting `char` -> `String`.

// --- Bar chars (used by gradient bars / context meter) -----------------------

pub mod bar {
    pub const FILLED: &str = "█";
    pub const HEAVY: &str = "▆";
    /// Nerd Font half-pill-right glyph (U+E0B4) used as the leading-edge cap
    /// in gradient bars.
    pub const MID: &str = "\u{e0b4}";
    pub const EMPTY: &str = "░";
}

// --- Style escapes -----------------------------------------------------------

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const ITALIC: &str = "\x1b[3m";

// --- 256-color foregrounds used throughout the renderer ---------------------

pub const CLR_GREY_DIM: &str = "\x1b[38;5;244m";
pub const CLR_GREY_DARK: &str = "\x1b[38;5;238m";
pub const CLR_BORDER_OFF: &str = "\x1b[38;5;242m";
pub const CLR_SKY_BLUE: &str = "\x1b[38;5;75m";
pub const CLR_GREEN_OK: &str = "\x1b[38;5;114m";
pub const CLR_GREEN_DIM: &str = "\x1b[38;5;77m";
pub const CLR_GREEN_BRT: &str = "\x1b[38;5;46m";
pub const CLR_PURPLE: &str = "\x1b[38;5;183m";
pub const CLR_GOLD: &str = "\x1b[38;5;222m";
pub const CLR_YELLOW: &str = "\x1b[38;5;226m";
pub const CLR_YELLOW_BRT: &str = "\x1b[38;5;11m";
pub const CLR_CYAN: &str = "\x1b[38;5;116m";
pub const CLR_CYAN_DIM: &str = "\x1b[38;5;244m";
pub const CLR_CYAN_DAY: &str = "\x1b[38;5;109m";
pub const CLR_CYAN_DAY_DIM: &str = "\x1b[38;5;240m";
pub const CLR_CYAN_ICON: &str = "\x1b[38;5;117m";
pub const CLR_PINK: &str = "\x1b[38;5;210m";
pub const CLR_PEACH: &str = "\x1b[38;5;216m";
pub const CLR_WHITE_BRT: &str = "\x1b[38;5;15m";
pub const CLR_WARN: &str = "\x1b[38;5;214m";
pub const CLR_ALERT: &str = "\x1b[38;5;167m";

// --- Nerd Font / Unicode glyphs ---------------------------------------------

pub const ICON_COST: &str = "\u{efc8}"; // nf-md currency-usd
pub const ICON_TOK_RATE: &str = "\u{f0830}"; // nf-md coin
pub const GLYPH_MODEL: &str = "\u{f08b9}"; // nf-md-monitor-dashboard
pub const GLYPH_THINKING: &str = "\u{f1a53}"; // nf-md-brain
pub const GLYPH_BURN_FAST: &str = "\u{ef76}"; // nf-cod-zap
pub const GLYPH_BURN_SLOW: &str = "\u{f490}"; // nf-oct-flame
pub const GLYPH_FOLDER: &str = "\u{ef85}"; // nf-custom folder
pub const GLYPH_GIT_BRANCH: &str = "\u{e0a0}"; // nf-pl-branch
pub const GLYPH_GIT_WORKTREE: &str = "\u{f1bb}"; // nf-fa-tree
pub const GLYPH_SUBAGENT: &str = "\u{f135}"; // nf-fa-tasks
pub const GLYPH_SUBAGENT_ROW: &str = "\u{25b6}"; // ▶
pub const GLYPH_TASKS: &str = "\u{f0755}"; // nf-md format-list-checks
pub const GLYPH_SKILLS: &str = "\u{f07df}"; // nf-md skills
pub const GLYPH_PLUGINS: &str = "\u{f1e6}"; // nf-fa-plug
pub const GLYPH_HELPER: &str = "\u{f4cd}"; // nf-mdi-star_circle
pub const GLYPH_TRASH: &str = "\u{f0a7a}"; // nf-md-trash_can
pub const GLYPH_RENAMED: &str = "\u{f1031}"; // nf-md-file_move
pub const GLYPH_CONTINUATION: &str = "\u{2514}"; // └
pub const GLYPH_RESPONDING: &str = "\u{f0189}"; // nf-md-message
pub const GLYPH_HOURGLASS: &str = "\u{f253}"; // nf-fa-hourglass_half
pub const GLYPH_PIE: &str = "\u{f200}"; // nf-fa-pie_chart
pub const GLYPH_VENV: &str = "\u{f0320}"; // nf-md-language_python
pub const GLYPH_CLOCK: &str = "\u{f017}"; // nf-fa-clock_o

// --- Sparkline slope chars (U+1FB3C..U+1FB6B "Symbols for Legacy Computing") -

pub const SPARK_RISE_SMALL: &str = "\u{1fb48}";
pub const SPARK_FALL_SMALL: &str = "\u{1fb3d}";
pub const SPARK_RISE_MED: &str = "\u{1fb4a}";
pub const SPARK_FALL_MED: &str = "\u{1fb3f}";
pub const SPARK_RISE_TALL: &str = "\u{1fb45}";
pub const SPARK_FALL_TALL: &str = "\u{1fb50}";
pub const SPARK_RISE_TOP: &str = "\u{1fb4b}";
pub const SPARK_FALL_TOP: &str = "\u{1fb40}";

// --- Pill quadrant chars ----------------------------------------------------

pub const PILL_TL: &str = "\u{2597}";
pub const PILL_TOP: &str = "\u{2584}";
pub const PILL_TR: &str = "\u{2596}";
pub const PILL_LEFT: &str = "\u{2590}";
pub const PILL_RIGHT: &str = "\u{258c}";
pub const PILL_BL: &str = "\u{259d}";
pub const PILL_BOT: &str = "\u{2580}";
pub const PILL_BR: &str = "\u{2598}";
