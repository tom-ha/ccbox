//! `ccbox-demo` — visual smoke test.
//!
//! Loads the canonical session-info fixture and renders it at a fixed
//! progression of widths (narrow / medium / wide). No animation loop —
//! the goal is a deterministic visual snapshot.

use std::path::PathBuf;
use std::process::ExitCode;

use ccbox::{render, theme, Env, SessionInfo};

fn main() -> ExitCode {
    let fixture_path = std::env::args().nth(1).unwrap_or_else(|| {
        // Default to the test fixture when run from the workspace root.
        "tests/fixtures/session-info-example.json".to_string()
    });
    let raw = match std::fs::read_to_string(&fixture_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ccbox-demo: cannot read {fixture_path}: {e}");
            return ExitCode::from(1);
        }
    };
    let session: SessionInfo = serde_json::from_str(&raw).unwrap_or_default();

    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
    let env = Env {
        claude_dir: home.join(".claude"),
        home,
        ..Default::default()
    };

    let widths = [44u16, 65, 100, 130];
    for w in widths {
        println!("\n== width {w} ==");
        let out = render(&session, &env, w);
        println!("{out}");
    }
    println!();

    // Also a quick swatch across all built-in themes at width 100.
    println!("\n== theme swatch (width 100) ==");
    for t in theme::ALL_THEMES {
        println!("\n-- {} --", t.name);
        let out = ccbox::render_with(
            &session, &env, 100, t, ccbox::BgShift::Warm,
        );
        println!("{out}");
    }

    ExitCode::SUCCESS
}
