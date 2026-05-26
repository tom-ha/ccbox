//! `ccbox` — render a Claude Code statusline from a session JSON blob.

pub mod ansi;
pub mod config;
pub mod consts;
pub mod cost;
pub mod data;
pub mod glyphs;
pub mod input;
pub mod layout;
pub mod render;
pub mod terminal;
pub mod theme;
pub mod width;

pub use config::Env;
pub use input::session::SessionInfo;
pub use render::{BgShift, Renderer};
pub use theme::Theme;

/// Top-level entry: render a session into a multi-line statusline string.
///
/// Empty string if `width < MIN_WIDTH`. No trailing newline.
pub fn render(session: &SessionInfo, env: &Env, width: u16) -> String {
    let r = Renderer::new(theme::DEFAULT_THEME, BgShift::Warm);
    layout::render(session, env, width as i32, &r)
}

/// Render against a caller-supplied theme + bg-shift.
pub fn render_with(session: &SessionInfo, env: &Env, width: u16, theme: &'static Theme, bg_shift: BgShift) -> String {
    let r = Renderer::new(theme, bg_shift);
    layout::render(session, env, width as i32, &r)
}
