//! `path_section` and `branch_chip`.
//!
//! The path is rendered verbatim when it fits the budget; otherwise the head
//! is replaced with `…` so the path *tail* (the working directory leaf)
//! survives. Branch is a compact chip showing branch name plus a dirty marker.

use crate::data::git_info::GitInfo;
use crate::glyphs::{GLYPH_FOLDER, GLYPH_GIT_BRANCH, GLYPH_GIT_WORKTREE, RESET};
use crate::render::Renderer;
use crate::width::visible_width;

impl Renderer {
    /// Render the path cluster with a folder glyph: `<glyph>  <path>`.
    ///
    /// When `visible_width(path) > budget`, the path is replaced with
    /// `…<tail>` so the result fits in `budget` columns. The function never
    /// abbreviates intermediate segments.
    pub fn path_section(&self, short_pwd: &str, budget: i32) -> String {
        let t = self.theme;
        let prefix = format!("{}{GLYPH_FOLDER}  {RESET}", t.icon_path);
        // Prefix glyph + 2 spaces = 3 visible cells (the folder glyph is wide).
        let prefix_w: i32 = visible_width(&prefix) as i32;
        let path_budget = (budget - prefix_w).max(1);
        let fitted = fit_path_head_ellipsis(short_pwd, path_budget);
        format!("{prefix}{}{fitted}{RESET}", t.pwd)
    }

    /// Render the git branch chip: `<glyph> <branch>` plus optional markers.
    ///
    /// Markers (right of the branch name, in order):
    /// - `±` (dirty colour) when the working tree is dirty.
    /// - `↑N` (commit-colour) when ahead of upstream.
    /// - `↓N` (warn colour) when behind upstream.
    ///
    /// Uses a file-tree glyph when the repo is a worktree (or submodule)
    /// instead of the branch glyph. Returns `("", 0)` when there is no
    /// branch.
    pub fn branch_chip(&self, git: &GitInfo) -> (String, usize) {
        if !git.is_repo() {
            return (String::new(), 0);
        }
        let t = self.theme;
        let dirty = git.modified > 0 || git.untracked > 0 || git.deleted > 0 || git.renamed > 0;
        let glyph = if git.worktree {
            GLYPH_GIT_WORKTREE
        } else {
            GLYPH_GIT_BRANCH
        };

        let mut text = format!(
            "{}{glyph}{RESET} {}{}{RESET}",
            t.branch, t.branch, git.branch,
        );
        if dirty {
            text.push_str(&format!(" {}±{RESET}", t.dirty));
        }
        if git.ahead > 0 {
            text.push_str(&format!(" {}↑{}{RESET}", t.commit, git.ahead));
        }
        if git.behind > 0 {
            text.push_str(&format!(" {}↓{}{RESET}", t.warn, git.behind));
        }
        let w = visible_width(&text);
        (text, w)
    }
}

/// Fit a path to `budget` visible columns. Returns the input verbatim when it
/// fits; otherwise prepends `…` and slices from the right so the tail is
/// preserved. The function never abbreviates intermediate segments.
pub fn fit_path_head_ellipsis(short_pwd: &str, budget: i32) -> String {
    let budget = budget.max(1) as usize;
    let chars: Vec<char> = short_pwd.chars().collect();
    let widths: Vec<usize> = chars
        .iter()
        .map(|c| if crate::width::is_wide(*c) { 2 } else { 1 })
        .collect();
    let total_w: usize = widths.iter().sum();
    if total_w <= budget {
        return short_pwd.to_string();
    }
    if budget == 1 {
        return "…".to_string();
    }
    // Reserve 1 column for the leading ellipsis; take from the right until
    // adding another char would exceed `budget - 1` columns.
    let mut taken = 0usize;
    let mut start = chars.len();
    for i in (0..chars.len()).rev() {
        if taken + widths[i] > budget - 1 {
            break;
        }
        taken += widths[i];
        start = i;
    }
    let mut out = String::from("…");
    for c in chars.iter().skip(start) {
        out.push(*c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(branch: &str) -> GitInfo {
        GitInfo {
            branch: branch.into(),
            ..Default::default()
        }
    }

    #[test]
    fn fit_path_fits_returns_verbatim() {
        assert_eq!(fit_path_head_ellipsis("~/proj", 80), "~/proj");
        assert_eq!(fit_path_head_ellipsis("~/proj", 6), "~/proj");
    }

    #[test]
    fn fit_path_head_ellipsis_preserves_tail() {
        let p = "/Users/alice/git/public/yet-another-statusline-in-rust";
        let r = fit_path_head_ellipsis(p, 30);
        assert!(r.starts_with('…'), "{r}");
        assert_eq!(visible_width(&r), 30);
        // Tail of the input should appear in the output.
        let tail: String = p
            .chars()
            .rev()
            .take(20)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        assert!(r.ends_with(&tail), "expected suffix preserved: {r}");
    }

    #[test]
    fn fit_path_no_segment_abbreviation() {
        let p = "/foo/bar/baz/qux";
        let r = fit_path_head_ellipsis(p, 20);
        // No "f/b/b/q" style abbreviation should be present.
        assert!(!r.contains("f/b"));
    }

    #[test]
    fn fit_path_tiny_budget_yields_ellipsis() {
        assert_eq!(fit_path_head_ellipsis("hello", 1), "…");
    }

    #[test]
    fn path_section_renders_folder_glyph_and_path() {
        let r = Renderer::default();
        let s = r.path_section("~/proj", 60);
        let plain = crate::ansi::strip_ansi(&s);
        assert!(plain.contains("~/proj"), "{plain}");
        assert!(plain.contains(GLYPH_FOLDER), "{plain}");
    }

    #[test]
    fn branch_chip_clean_renders_glyph_and_branch() {
        let r = Renderer::default();
        let (text, w) = r.branch_chip(&git("main"));
        let plain = crate::ansi::strip_ansi(&text);
        assert!(
            plain.contains(GLYPH_GIT_BRANCH),
            "missing branch glyph: {plain}"
        );
        assert!(plain.contains("main"), "missing branch name: {plain}");
        assert!(!plain.contains("±"));
        assert_eq!(w, visible_width(&text));
    }

    #[test]
    fn branch_chip_dirty_appends_dot() {
        let r = Renderer::default();
        let mut g = git("main");
        g.modified = 1;
        let (text, _w) = r.branch_chip(&g);
        let plain = crate::ansi::strip_ansi(&text);
        assert!(plain.contains(GLYPH_GIT_BRANCH));
        assert!(plain.contains("main ±"), "{plain}");
    }

    #[test]
    fn branch_chip_dirty_with_untracked_only() {
        let r = Renderer::default();
        let mut g = git("main");
        g.untracked = 2;
        let (text, _w) = r.branch_chip(&g);
        let plain = crate::ansi::strip_ansi(&text);
        assert!(plain.contains("±"), "{plain}");
    }

    #[test]
    fn branch_chip_no_git_returns_empty() {
        let r = Renderer::default();
        let (text, w) = r.branch_chip(&GitInfo::default());
        assert!(text.is_empty());
        assert_eq!(w, 0);
    }

    #[test]
    fn branch_chip_worktree_swaps_glyph() {
        let r = Renderer::default();
        let mut g = git("feature");
        g.worktree = true;
        let (text, _w) = r.branch_chip(&g);
        let plain = crate::ansi::strip_ansi(&text);
        assert!(
            plain.contains(GLYPH_GIT_WORKTREE),
            "expected worktree glyph: {plain}"
        );
        assert!(
            !plain.contains(GLYPH_GIT_BRANCH),
            "should not have branch glyph: {plain}"
        );
        assert!(plain.contains("feature"));
    }

    #[test]
    fn branch_chip_non_worktree_uses_branch_glyph() {
        let r = Renderer::default();
        let (text, _w) = r.branch_chip(&git("main"));
        let plain = crate::ansi::strip_ansi(&text);
        assert!(plain.contains(GLYPH_GIT_BRANCH), "{plain}");
        assert!(!plain.contains(GLYPH_GIT_WORKTREE), "{plain}");
    }

    #[test]
    fn branch_chip_ahead_only() {
        let r = Renderer::default();
        let mut g = git("main");
        g.ahead = 2;
        let (text, _w) = r.branch_chip(&g);
        let plain = crate::ansi::strip_ansi(&text);
        assert!(plain.contains("↑2"), "{plain}");
        assert!(!plain.contains("↓"), "{plain}");
        assert!(!plain.contains("±"), "{plain}");
    }

    #[test]
    fn branch_chip_behind_only() {
        let r = Renderer::default();
        let mut g = git("main");
        g.behind = 3;
        let (text, _w) = r.branch_chip(&g);
        let plain = crate::ansi::strip_ansi(&text);
        assert!(plain.contains("↓3"), "{plain}");
        assert!(!plain.contains("↑"), "{plain}");
    }

    #[test]
    fn branch_chip_ahead_and_behind() {
        let r = Renderer::default();
        let mut g = git("main");
        g.ahead = 2;
        g.behind = 1;
        let (text, _w) = r.branch_chip(&g);
        let plain = crate::ansi::strip_ansi(&text);
        assert!(plain.contains("↑2"), "{plain}");
        assert!(plain.contains("↓1"), "{plain}");
    }

    #[test]
    fn branch_chip_dirty_plus_diverged() {
        let r = Renderer::default();
        let mut g = git("main");
        g.modified = 1;
        g.ahead = 2;
        g.behind = 1;
        let (text, _w) = r.branch_chip(&g);
        let plain = crate::ansi::strip_ansi(&text);
        assert!(plain.contains("±"), "{plain}");
        assert!(plain.contains("↑2"), "{plain}");
        assert!(plain.contains("↓1"), "{plain}");
    }

    #[test]
    fn branch_chip_in_sync_has_no_arrows() {
        let r = Renderer::default();
        let (text, _w) = r.branch_chip(&git("main"));
        let plain = crate::ansi::strip_ansi(&text);
        assert!(!plain.contains("↑"));
        assert!(!plain.contains("↓"));
    }
}
