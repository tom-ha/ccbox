//! Layout-width thresholds and rate-limit window constants.

pub const MIN_WIDTH: u16 = 40;
pub const DEFAULT_MAX_WIDTH: u16 = 140;
pub const NARROW_WIDTH: u16 = 55;
pub const MEDIUM_WIDTH: u16 = 80;
/// Approximate point at which Claude Code auto-compacts.
pub const AUTOCOMPACT_RATIO: f64 = 0.75;

pub const FIVE_HOUR_MINUTES: u32 = 300;
pub const SEVEN_DAY_MINUTES: u32 = 10_080;
pub const FIVE_HOUR_WARMUP_MINUTES: u32 = 5;
pub const SEVEN_DAY_WARMUP_MINUTES: u32 = 30;

pub const LIVE_DIM: f32 = 0.5;
