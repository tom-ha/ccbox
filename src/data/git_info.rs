//! `GitInfo` — branch, short commit, and dirty-file counts.
//!
//! `from_cwd` reads `.git/HEAD` directly (cheap, no subprocess) and shells out
//! to `git status --porcelain=v1 -z --untracked-files=normal` with a 2-second
//! timeout for the dirty-file rollup.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use wait_timeout::ChildExt;

#[derive(Debug, Default, Clone)]
pub struct GitInfo {
    pub branch: String,
    pub commit: String,
    pub modified: u32,
    pub untracked: u32,
    pub deleted: u32,
    pub renamed: u32,
    /// `true` when `.git` is a file (worktree or submodule) rather than a
    /// real `.git` directory. Lets the renderer swap the branch glyph for a
    /// worktree-distinct one.
    pub worktree: bool,
}

impl GitInfo {
    pub fn is_repo(&self) -> bool {
        !self.branch.is_empty()
    }

    pub fn from_cwd(cwd: &str) -> Option<Self> {
        let (repo, gitdir, worktree) = Self::find_repo(cwd)?;
        let (branch, commit) = Self::read_head(&gitdir);
        let (modified, untracked, deleted, renamed) = if !branch.is_empty() {
            Self::dirty(&repo)
        } else {
            (0, 0, 0, 0)
        };
        Some(Self { branch, commit, modified, untracked, deleted, renamed, worktree })
    }

    /// Returns `(repo_root, resolved_gitdir, is_worktree)`. `is_worktree` is
    /// `true` when `.git` was a file rather than a directory.
    fn find_repo(cwd: &str) -> Option<(PathBuf, PathBuf, bool)> {
        if cwd.is_empty() { return None; }
        let mut curr = Some(PathBuf::from(cwd));
        while let Some(c) = curr {
            let g = c.join(".git");
            if g.is_dir() {
                return Some((c.clone(), g, false));
            }
            if g.is_file() {
                // Worktree (or submodule): `.git` is a file whose contents
                // are `gitdir: <path>` pointing at the real gitdir, which
                // lives under the main repo's `.git/worktrees/<name>` (for
                // worktrees) or elsewhere (for submodules).
                if let Some(resolved) = Self::resolve_gitfile(&g, &c) {
                    return Some((c.clone(), resolved, true));
                }
            }
            let parent = c.parent().map(|p| p.to_path_buf());
            if parent.as_deref() == Some(&c) || parent.is_none() {
                break;
            }
            curr = parent;
        }
        None
    }

    fn resolve_gitfile(gitfile: &Path, repo: &Path) -> Option<PathBuf> {
        let text = std::fs::read_to_string(gitfile).ok()?;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("gitdir:") {
                let raw = rest.trim();
                if raw.is_empty() { return None; }
                let p = PathBuf::from(raw);
                let resolved = if p.is_absolute() { p } else { repo.join(p) };
                return Some(resolved);
            }
        }
        None
    }

    fn read_head(gitdir: &Path) -> (String, String) {
        let head_path = gitdir.join("HEAD");
        if !head_path.is_file() { return (String::new(), String::new()); }
        let head = match std::fs::read_to_string(&head_path) {
            Ok(s) => s.trim().to_string(),
            Err(_) => return (String::new(), String::new()),
        };
        let branch = if head.starts_with("ref:") {
            head.rsplit('/').next().unwrap_or("").to_string()
        } else if !head.is_empty() {
            format!("d:{}", &head[..7.min(head.len())])
        } else {
            String::new()
        };
        let mut commit = String::new();
        if !branch.is_empty() && !branch.starts_with("d:") {
            let ref_path = gitdir.join("refs").join("heads").join(&branch);
            if ref_path.is_file() {
                if let Ok(s) = std::fs::read_to_string(&ref_path) {
                    commit = s.trim().chars().take(9).collect();
                }
            }
        }
        if commit.is_empty() {
            let orig = gitdir.join("ORIG_HEAD");
            if orig.is_file() {
                if let Ok(s) = std::fs::read_to_string(&orig) {
                    commit = s.trim().chars().take(9).collect();
                }
            }
        }
        (branch, commit)
    }

    fn dirty(repo: &Path) -> (u32, u32, u32, u32) {
        let mut modified = 0u32;
        let mut untracked = 0u32;
        let mut deleted = 0u32;
        let mut renamed = 0u32;
        let child = Command::new("git")
            .args(["-C"]).arg(repo).args(["status", "--porcelain=v1", "-z", "--untracked-files=normal"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match child { Ok(c) => c, Err(_) => return (0, 0, 0, 0) };
        match child.wait_timeout(Duration::from_secs(2)) {
            Ok(Some(_)) => {}
            _ => {
                let _ = child.kill();
                return (0, 0, 0, 0);
            }
        }
        let mut stdout_buf = Vec::new();
        if let Some(mut out) = child.stdout.take() {
            use std::io::Read;
            let _ = out.read_to_end(&mut stdout_buf);
        }
        let text = String::from_utf8_lossy(&stdout_buf);
        let entries: Vec<&str> = text.split('\0').filter(|e| !e.is_empty()).collect();
        let mut i = 0;
        while i < entries.len() {
            let e = entries[i];
            if e.len() < 2 { i += 1; continue; }
            let (x, y) = (e.as_bytes()[0] as char, e.as_bytes()[1] as char);
            if x == 'R' || y == 'R' {
                renamed += 1;
                i += 2;
                continue;
            }
            if x == '?' && y == '?' {
                untracked += 1;
            } else if x == 'A' || y == 'A' {
                untracked += 1;
            } else if x == 'D' || y == 'D' {
                deleted += 1;
            } else if x == 'M' || y == 'M' {
                modified += 1;
            }
            i += 1;
        }
        (modified, untracked, deleted, renamed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn no_git_dir_returns_none() {
        let dir = tempdir().unwrap();
        let g = GitInfo::from_cwd(dir.path().to_str().unwrap());
        assert!(g.is_none());
    }

    #[test]
    fn reads_branch_from_head_ref() {
        let dir = tempdir().unwrap();
        let gitdir = dir.path().join(".git");
        std::fs::create_dir_all(gitdir.join("refs/heads")).unwrap();
        std::fs::write(gitdir.join("HEAD"), "ref: refs/heads/feature/foo\n").unwrap();
        std::fs::write(gitdir.join("refs/heads/foo"), "deadbeefcafef00d\n").unwrap();
        let g = GitInfo::from_cwd(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(g.branch, "foo");
        assert_eq!(g.commit, "deadbeefc");
    }

    #[test]
    fn detached_head_gets_d_prefix() {
        let dir = tempdir().unwrap();
        let gitdir = dir.path().join(".git");
        std::fs::create_dir_all(&gitdir).unwrap();
        std::fs::write(gitdir.join("HEAD"), "deadbeefcafef00d\n").unwrap();
        let g = GitInfo::from_cwd(dir.path().to_str().unwrap()).unwrap();
        assert!(g.branch.starts_with("d:"), "{}", g.branch);
    }

    #[test]
    fn worktree_dot_git_file_resolved_to_real_gitdir() {
        // Simulate a worktree: main repo at `<main>` with `.git/worktrees/wt`
        // holding the worktree's HEAD; the worktree itself is `<wt>` with a
        // `.git` *file* containing `gitdir: <abs-path-to-worktree-gitdir>`.
        let root = tempdir().unwrap();
        let main = root.path().join("main");
        std::fs::create_dir_all(&main).unwrap();
        let main_gitdir = main.join(".git");
        let wt_gitdir = main_gitdir.join("worktrees").join("feature");
        std::fs::create_dir_all(&wt_gitdir).unwrap();
        std::fs::write(wt_gitdir.join("HEAD"), "ref: refs/heads/feature\n").unwrap();

        let wt = root.path().join("feature-wt");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(
            wt.join(".git"),
            format!("gitdir: {}\n", wt_gitdir.display()),
        ).unwrap();

        let g = GitInfo::from_cwd(wt.to_str().unwrap()).unwrap();
        assert_eq!(g.branch, "feature", "expected worktree branch parsed: {g:?}");
    }

    #[test]
    fn worktree_with_relative_gitdir() {
        // Some git versions write the gitdir as a relative path.
        let root = tempdir().unwrap();
        let main = root.path().join("main");
        std::fs::create_dir_all(&main).unwrap();
        let main_gitdir = main.join(".git");
        let wt_gitdir = main_gitdir.join("worktrees").join("topic");
        std::fs::create_dir_all(&wt_gitdir).unwrap();
        std::fs::write(wt_gitdir.join("HEAD"), "ref: refs/heads/topic\n").unwrap();

        let wt = root.path().join("topic-wt");
        std::fs::create_dir_all(&wt).unwrap();
        // Relative path from wt to wt_gitdir.
        let rel = pathdiff(&wt, &wt_gitdir);
        std::fs::write(
            wt.join(".git"),
            format!("gitdir: {}\n", rel.display()),
        ).unwrap();

        let g = GitInfo::from_cwd(wt.to_str().unwrap()).unwrap();
        assert_eq!(g.branch, "topic", "expected worktree branch parsed: {g:?}");
    }

    fn pathdiff(from: &Path, to: &Path) -> PathBuf {
        // Minimal relative-path helper just for the test.
        let from = from.canonicalize().unwrap_or_else(|_| from.to_path_buf());
        let to = to.canonicalize().unwrap_or_else(|_| to.to_path_buf());
        let mut from_iter = from.components();
        let mut to_iter = to.components();
        let mut common = 0;
        loop {
            match (from_iter.clone().next(), to_iter.clone().next()) {
                (Some(a), Some(b)) if a == b => {
                    from_iter.next();
                    to_iter.next();
                    common += 1;
                }
                _ => break,
            }
        }
        let _ = common;
        let mut out = PathBuf::new();
        for _ in from_iter { out.push(".."); }
        for c in to_iter { out.push(c.as_os_str()); }
        out
    }
}
