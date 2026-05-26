//! `plugins_skills` — single-line summary of loaded skills + enabled plugins.

use crate::glyphs::{BOLD, GLYPH_PLUGINS, GLYPH_SKILLS, RESET};
use crate::render::palette::rainbow_at;
use crate::render::Renderer;

impl Renderer {
    pub fn plugins_skills(&self, skills_count: u32, skills_names: &str, plugin_names: &str, step: usize) -> String {
        let c_skills = rainbow_at(step, 3);
        let c_plugins = rainbow_at(step, 6);
        let mut extras: Vec<String> = Vec::new();
        if skills_count > 0 {
            extras.push(format!(
                "{c_skills}{BOLD}{GLYPH_SKILLS}  {RESET}{}{skills_names}{RESET}",
                self.theme.skills,
            ));
        }
        if !plugin_names.is_empty() {
            extras.push(format!(
                "{c_plugins}{BOLD}{GLYPH_PLUGINS}  {RESET}{}{plugin_names}{RESET}",
                self.theme.skills,
            ));
        }
        let sep = format!(" {}|{RESET} ", self.theme.label);
        extras.join(&sep)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_returns_empty() {
        let r = Renderer::default();
        assert_eq!(r.plugins_skills(0, "", "", 0), "");
    }

    #[test]
    fn only_skills_no_separator() {
        let r = Renderer::default();
        let s = r.plugins_skills(2, "code-review, init", "", 0);
        let plain = crate::ansi::strip_ansi(&s);
        assert!(plain.contains("code-review"));
        assert!(!plain.contains('|'));
    }

    #[test]
    fn only_plugins_no_separator() {
        let r = Renderer::default();
        let s = r.plugins_skills(0, "", "fav, other", 0);
        let plain = crate::ansi::strip_ansi(&s);
        assert!(plain.contains("fav, other"));
        assert!(!plain.contains('|'));
    }

    #[test]
    fn both_skills_and_plugins_separated() {
        let r = Renderer::default();
        let s = r.plugins_skills(1, "code-review", "fav", 0);
        let plain = crate::ansi::strip_ansi(&s);
        assert!(plain.contains("code-review"));
        assert!(plain.contains("fav"));
        assert!(plain.contains('|'));
    }
}
