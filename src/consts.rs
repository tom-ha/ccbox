//! Layout-width thresholds and rate-limit window constants.

pub const MIN_WIDTH: u16 = 40;
pub const DEFAULT_MAX_WIDTH: u16 = 140;
pub const NARROW_WIDTH: u16 = 55;
pub const MEDIUM_WIDTH: u16 = 80;
/// Legacy soft-limit fallback used when the session JSON does not report
/// `context_window_size`. Tuned for the 200K context window (Claude Code's
/// auto-compact trigger sits around 150K of 200K, i.e. 75%).
pub const SOFT_LIMIT: u64 = 150_000;
/// Fraction of the model's reported context window at which the context-line
/// percentage hits 100%. Picked to preserve 200K behaviour bit-for-bit
/// (200_000 * 0.75 == 150_000 == legacy SOFT_LIMIT) while scaling sensibly
/// for larger windows (e.g. 1M → 750K threshold).
pub const AUTOCOMPACT_RATIO: f64 = 0.75;

pub const FIVE_HOUR_MINUTES: u32 = 300;
pub const SEVEN_DAY_MINUTES: u32 = 10_080;
pub const FIVE_HOUR_WARMUP_MINUTES: u32 = 5;
pub const SEVEN_DAY_WARMUP_MINUTES: u32 = 30;

pub const LIVE_DIM: f32 = 0.5;
