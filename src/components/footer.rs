//! `Footer` — bottom border. `ups` is set by the composition runner when the
//! preceding content row carried `downs` markers.

use crate::glyphs::{BOLD, RESET};
use crate::layout::{RowKind, RowSpec};
use crate::release::Version;
use crate::theme::Theme;

use super::component::{Component, ComponentOutput, SeparatorPolicy};
use super::context::ComponentContext;

pub fn update_chips(theme: &Theme, latest: Version) -> Vec<String> {
    let (ok, dim) = (theme.safe, theme.label);
    let head = format!("{BOLD}{ok}⬆ ccbox {latest}{RESET}");
    let cmd = format!("{ok}ccbox update{RESET}");
    vec![
        format!(" {head}{dim} available · run {RESET}{cmd} "),
        format!(" {head}{dim} · {RESET}{cmd} "),
        format!(" {head} "),
    ]
}

pub struct Footer;
pub static FOOTER: Footer = Footer;

impl Component for Footer {
    fn id(&self) -> &'static str {
        "footer"
    }

    fn is_visible(&self, _ctx: &ComponentContext) -> bool {
        true
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let mut row = RowSpec::new(RowKind::BottomBorder);
        if let Some(latest) = ctx
            .data
            .update_check(ctx)
            .and_then(|c| c.newer_than_installed())
        {
            row.right_chips = update_chips(ctx.renderer.theme, latest);
        }
        ComponentOutput {
            rows: vec![row],
            leading_separator: SeparatorPolicy::None,
            top_right_chip: String::new(),
            leading_left_chip: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::strip_ansi;
    use crate::config::Env;
    use crate::data::update_check::run_check_with;
    use crate::input::session::SessionInfo;
    use crate::render::{BgShift, Renderer};
    use crate::theme::builtin::{CATPPUCCIN_LATTE, CATPPUCCIN_MOCHA, CLAUDE_DARK, CLAUDE_LIGHT};
    use crate::width::visible_width;

    fn env_with_latest(latest: Option<&str>, enabled: bool) -> (tempfile::TempDir, Env) {
        let d = tempfile::tempdir().unwrap();
        let latest = latest.map(str::to_string);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        run_check_with(d.path(), now - 60.0, true, || Ok(latest));
        let env = Env {
            claude_dir: d.path().to_path_buf(),
            update_check: enabled,
            ..Default::default()
        };
        (d, env)
    }

    fn bottom_row(env: &Env, width: i32, theme: &'static Theme) -> String {
        let r = Renderer::new(theme, BgShift::Warm);
        let out = crate::layout::render(&SessionInfo::default(), env, width, &r);
        out.lines().last().unwrap().to_string()
    }

    fn newer() -> String {
        let v = Version::installed();
        format!("{}.0.0", v.major + 1)
    }

    #[test]
    fn chip_shows_for_a_newer_release_at_every_layout_and_theme() {
        let latest = newer();
        let (_d, env) = env_with_latest(Some(&latest), true);
        for theme in [
            &CLAUDE_DARK,
            &CLAUDE_LIGHT,
            &CATPPUCCIN_LATTE,
            &CATPPUCCIN_MOCHA,
        ] {
            for width in [40, 44, 54, 55, 64, 79, 80, 134, 200] {
                let row = bottom_row(&env, width, theme);
                assert_eq!(
                    visible_width(&row),
                    width as usize,
                    "{} width={width}",
                    theme.name
                );
                let plain = strip_ansi(&row);
                assert!(
                    plain.contains(&format!("⬆ ccbox {latest}")),
                    "{} width={width}: {plain}",
                    theme.name
                );
                if width >= 60 {
                    assert!(
                        plain.contains("available · run ccbox update"),
                        "width={width}: {plain}"
                    );
                }
            }
        }
    }

    #[test]
    fn no_chip_when_equal_older_absent_or_disabled() {
        let installed = Version::installed().to_string();
        for (latest, enabled) in [
            (Some(installed.as_str()), true),
            (Some("0.0.1"), true),
            (None, true),
            (Some(newer().as_str()), false),
        ] {
            let (_d, env) = env_with_latest(latest, enabled);
            let raw = bottom_row(&env, 134, &CLAUDE_DARK);
            let row = strip_ansi(&raw);
            assert!(
                !row.contains('⬆'),
                "latest={latest:?} enabled={enabled}: {row}"
            );
            assert!(
                row.chars().skip(1).take(132).all(|c| c == '─' || c == '┴'),
                "{row}"
            );
        }
    }

    #[test]
    fn a_fresh_cache_is_the_only_input() {
        let (d, env) = env_with_latest(Some(&newer()), true);
        let _ = bottom_row(&env, 134, &CLAUDE_DARK);
        let names: Vec<String> = std::fs::read_dir(d.path().join("ccbox-cache"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            ["update-check.json"],
            "fresh cache, so no spawn marker"
        );
    }
}
