//! `pwd_tilde` — collapse the home prefix in a CWD to `~`.
//!
//! Returns the CWD verbatim except that a leading `$HOME` is replaced with
//! `~`. The new ccbox UI shows full paths and never abbreviates intermediate
//! segments; head-ellipsis at render time handles overflow.

use std::path::Path;

use super::session::SessionInfo;

pub fn pwd_tilde(cwd: &str, home: &Path) -> String {
    let home_str = home.to_string_lossy();
    if !home_str.is_empty() && cwd.starts_with(home_str.as_ref()) {
        format!("~{}", &cwd[home_str.len()..])
    } else {
        cwd.to_string()
    }
}

impl SessionInfo {
    /// Path with `$HOME` collapsed to `~`, no segment abbreviation.
    pub fn short_pwd(&self, home: &Path) -> String {
        pwd_tilde(&self.cwd, home)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn home() -> PathBuf { PathBuf::from("/Users/alice") }

    #[test]
    fn substitutes_home_with_tilde() {
        assert_eq!(pwd_tilde("/Users/alice/projects/foo", &home()), "~/projects/foo");
    }

    #[test]
    fn intermediate_segments_not_abbreviated() {
        assert_eq!(pwd_tilde("/projects/foo/bar", &home()), "/projects/foo/bar");
    }

    #[test]
    fn full_segment_names_kept() {
        assert_eq!(
            pwd_tilde("/Users/alice/work/some-long-project-name", &home()),
            "~/work/some-long-project-name",
        );
    }

    #[test]
    fn empty_cwd_yields_empty_string() {
        assert_eq!(pwd_tilde("", &home()), "");
    }

    #[test]
    fn cwd_not_under_home_left_alone() {
        assert_eq!(pwd_tilde("/var/log/system", &home()), "/var/log/system");
    }

    #[test]
    fn session_info_short_pwd_uses_helper() {
        let s = SessionInfo { cwd: "/Users/alice/projects/foo".into(), ..Default::default() };
        assert_eq!(s.short_pwd(&home()), "~/projects/foo");
    }
}
