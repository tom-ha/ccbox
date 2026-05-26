//! Terminal-width detection with the tmux / COLUMNS / $CLAUDE_DIR / tty fallback.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use wait_timeout::ChildExt;

use crate::consts::DEFAULT_MAX_WIDTH;

/// Probe several sources for the current terminal width. Returns `DEFAULT_MAX_WIDTH`
/// when nothing works.
pub fn terminal_width(claude_dir: Option<&Path>) -> u16 {
    if let Ok(tmux) = std::env::var("TMUX_PANE") {
        if let Some(w) = tmux_pane_width(&tmux) {
            return w;
        }
    }
    if let Some(dir) = claude_dir {
        if let Ok(s) = std::fs::read_to_string(dir.join("terminal-width")) {
            if let Ok(w) = s.trim().parse::<u16>() {
                if w > 0 { return w; }
            }
        }
    }
    if let Ok(s) = std::env::var("COLUMNS") {
        if let Ok(w) = s.parse::<u16>() {
            if w > 0 { return w; }
        }
    }
    if let Some(w) = ioctl_winsize_cols() {
        return w;
    }
    DEFAULT_MAX_WIDTH
}

fn tmux_pane_width(pane: &str) -> Option<u16> {
    let child = Command::new("tmux")
        .args(["display-message", "-p", "-t", pane, "#{pane_width}"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut child = child;
    match child.wait_timeout(Duration::from_secs(2)).ok()? {
        Some(_) => {}
        None => {
            let _ = child.kill();
            return None;
        }
    }
    let mut buf = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        use std::io::Read;
        out.read_to_end(&mut buf).ok()?;
    }
    let s = String::from_utf8_lossy(&buf);
    s.trim().parse().ok()
}

fn ioctl_winsize_cols() -> Option<u16> {
    #[repr(C)]
    struct Winsize {
        ws_row: u16,
        ws_col: u16,
        ws_xpixel: u16,
        ws_ypixel: u16,
    }
    extern "C" {
        fn ioctl(fd: i32, request: u64, ...) -> i32;
    }
    // TIOCGWINSZ varies per OS; macOS = 0x40087468, Linux = 0x5413. We try both.
    const REQUESTS: [u64; 2] = [0x40087468, 0x5413];
    for fd in [2, 1, 0] {
        for &req in &REQUESTS {
            let mut ws = Winsize { ws_row: 0, ws_col: 0, ws_xpixel: 0, ws_ypixel: 0 };
            let r = unsafe { ioctl(fd, req, &mut ws as *mut Winsize) };
            if r == 0 && ws.ws_col > 0 {
                return Some(ws.ws_col);
            }
        }
    }
    None
}
