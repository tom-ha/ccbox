//! `venv_section` — compact top-row cluster for the active Python venv.

use crate::glyphs::{GLYPH_VENV, RESET};
use crate::render::Renderer;
use crate::width::visible_width;

impl Renderer {
    /// Render the venv cluster: `<glyph> <dim "venv:"> <bright NAME>`. Returns
    /// `("", 0)` when `name` is `None` or empty, so callers can fold the
    /// result into pad math without branching.
    pub fn venv_section(&self, name: Option<&str>) -> (String, usize) {
        let name = match name {
            Some(s) if !s.is_empty() => s,
            _ => return (String::new(), 0),
        };
        let t = self.theme;
        let text = format!(
            "{}{GLYPH_VENV}{RESET} {}venv:{RESET} {}{}{RESET}",
            t.skills, t.label, t.white_brt, name,
        );
        let w = visible_width(&text);
        (text, w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn venv_section_none_returns_empty() {
        let r = Renderer::default();
        let (text, w) = r.venv_section(None);
        assert!(text.is_empty());
        assert_eq!(w, 0);
    }

    #[test]
    fn venv_section_empty_string_returns_empty() {
        let r = Renderer::default();
        let (text, w) = r.venv_section(Some(""));
        assert!(text.is_empty());
        assert_eq!(w, 0);
    }

    #[test]
    fn venv_section_renders_name() {
        let r = Renderer::default();
        let (text, w) = r.venv_section(Some("py311"));
        let plain = crate::ansi::strip_ansi(&text);
        assert!(plain.contains("py311"), "{plain}");
        assert!(plain.contains("venv:"), "{plain}");
        assert_eq!(w, visible_width(&text));
        assert!(w > 0);
    }
}
