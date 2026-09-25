//! Property test: every rendered statusline row's visible width equals the
//! requested width, across an arbitrary session shape × width × density ×
//! tasks-view.
//!
//! NOTE on coverage gap: this test currently exercises **wide** (`>= 80`) and
//! **narrow** (`< NARROW_WIDTH`) widths only. The **medium** zone
//! (`NARROW_WIDTH..80`) is a known pre-existing offender: the tokens-cost row
//! has a fixed-width cluster that overflows the box when the internal width
//! is below ~68. Fixing that overflow is intentionally out of scope for the
//! `refine-composable-statusline` change; the property test will be extended
//! to the medium zone once that bug is addressed.

use proptest::prelude::*;

use ccbox::{
    ansi::strip_ansi,
    config::{Density, Env, TasksView},
    consts::{MIN_WIDTH, NARROW_WIDTH},
    input::session::{Model, RateBucket, RateLimits, SessionInfo},
    render,
    width::visible_width,
};

fn make_session(model_name: &str, cwd: &str, transcript_path: &str) -> SessionInfo {
    SessionInfo {
        session_id: "test".to_string(),
        cwd: cwd.to_string(),
        transcript_path: transcript_path.to_string(),
        model: Model {
            id: model_name.to_string(),
            display_name: model_name.to_string(),
        },
        ..Default::default()
    }
}

fn env_for(density: Density, tasks_view: TasksView) -> Env {
    Env {
        // Both paths point at locations that don't exist, so no on-disk
        // statusline-tokens.log / subscription marker / git cache writes
        // happen during the property test.
        claude_dir: std::path::PathBuf::from("/tmp/ccbox-propertytest-claude-dir-nonexistent"),
        home: std::path::PathBuf::from("/tmp/ccbox-propertytest-home-nonexistent"),
        density,
        tasks_view,
        git_cache_ttl_ms: 0,
        ..Default::default()
    }
}

prop_compose! {
    fn arb_density()(d in 0u8..3) -> Density {
        match d {
            0 => Density::Minimal,
            1 => Density::Standard,
            _ => Density::Verbose,
        }
    }
}

prop_compose! {
    fn arb_tasks_view()(t in 0u8..2) -> TasksView {
        match t { 0 => TasksView::Inline, _ => TasksView::Board }
    }
}

prop_compose! {
    fn arb_model()(s in "[A-Za-z][A-Za-z0-9 .-]{0,30}") -> String { s }
}

prop_compose! {
    fn arb_cwd()(s in "(/[A-Za-z0-9._-]{1,12}){0,5}") -> String { s }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 96, ..ProptestConfig::default() })]

    #[test]
    fn wide_zone_every_row_has_requested_visible_width(
        width in 80u16..=200,
        density in arb_density(),
        tasks_view in arb_tasks_view(),
        model in arb_model(),
        cwd in arb_cwd(),
    ) {
        let session = make_session(&model, &cwd, "");
        let env = env_for(density, tasks_view);
        let out = render(&session, &env, width);
        prop_assert!(!out.is_empty(), "render returned empty at width {}", width);
        for (idx, line) in out.lines().enumerate() {
            let vw = visible_width(strip_ansi(line).as_ref());
            let want = width as usize;
            let plain = strip_ansi(line).into_owned();
            prop_assert_eq!(
                vw, want,
                "line {} has visible width {}, expected {}; line was {:?}",
                idx, vw, want, plain
            );
        }
    }

    #[test]
    fn narrow_zone_every_row_has_requested_visible_width(
        width in MIN_WIDTH..NARROW_WIDTH,
        density in arb_density(),
        tasks_view in arb_tasks_view(),
        model in arb_model(),
        cwd in arb_cwd(),
        limits in proptest::option::of((0.0f64..=120.0, 0.0f64..=120.0, 60i64..600_000)),
    ) {
        let mut session = make_session(&model, &cwd, "");
        let claude = tempfile::TempDir::new().unwrap();
        let mut env = env_for(density, tasks_view);
        if let Some((five, seven, resets_in)) = limits {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
            session.rate_limits = RateLimits {
                five_hour: RateBucket { used_percentage: five, resets_at: now + resets_in % 18_000 },
                seven_day: RateBucket { used_percentage: seven, resets_at: now + resets_in },
            };
            env.claude_dir = claude.path().to_path_buf();
        }
        let out = render(&session, &env, width);
        prop_assert!(!out.is_empty(), "render returned empty at width {}", width);
        for (idx, line) in out.lines().enumerate() {
            let vw = visible_width(strip_ansi(line).as_ref());
            let want = width as usize;
            let plain = strip_ansi(line).into_owned();
            prop_assert_eq!(
                vw, want,
                "line {} has visible width {}, expected {}; line was {:?}",
                idx, vw, want, plain
            );
        }
        if limits.is_some() {
            let plain = strip_ansi(&out).into_owned();
            prop_assert!(plain.contains("session") && plain.contains("week"), "{}", plain);
        }
    }
}
