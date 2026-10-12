//! The OS layer next to `git.rs`: where a repository keeps its files, which of
//! them matter, the `notify` watcher and the folder scan. The design is
//! Decision 6 of plans/003-trigger-git/epic.md and research note R01.

// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use notify::RecursiveMode;
use trigger_core::Availability;

use crate::git::{run, value, TIMEOUT};

/// Names directly inside a git dir that belong to one work tree and can change
/// what the snapshot shows. Everything else (`index`, `COMMIT_EDITMSG`,
/// `FETCH_HEAD`, `logs/`, `objects/`) is noise: `git status` alone rewrites
/// `index`. `packed-refs`, `refs/` and `reftable/` are shared and handled in
/// `relevant_inside`.
const OWN_NAMES: &[&str] = &["HEAD", "MERGE_HEAD", "rebase-merge", "rebase-apply"];

pub fn availability() -> Availability {
    match run(&std::env::temp_dir(), &["--version"], TIMEOUT) {
        Ok(out) if out.ok => Availability::Available,
        _ => Availability::Unsupported {
            reason: "git was not found".into(),
        },
    }
}

/// Where a repository keeps its files. In a normal repository the two paths
/// are the same `.git` folder. In a linked worktree `git_dir` is
/// `<main>/.git/worktrees/<name>` (HEAD, MERGE_HEAD, the rebase folders) and
/// `common_dir` is `<main>/.git` (refs, packed-refs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitDirs {
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
}

fn canonical(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

impl GitDirs {
    /// Asks git for the two folders of the repository whose work tree is
    /// `repo`. Both are made canonical, because macOS reports `/private/var`
    /// for `/var` and Windows adds `\\?\`, and events are compared with them.
    pub fn resolve(repo: &Path) -> Result<GitDirs, String> {
        let git_dir = value(repo, &["rev-parse", "--absolute-git-dir"])
            .ok_or_else(|| "not a git repository".to_string())?;
        let common = value(repo, &["rev-parse", "--git-common-dir"])
            .ok_or_else(|| "not a git repository".to_string())?;
        // Git prints the common dir relative to the folder it ran in, such as
        // `.git`, unless it is outside the work tree.
        let common = PathBuf::from(common);
        let common = if common.is_relative() {
            repo.join(common)
        } else {
            common
        };
        Ok(GitDirs {
            git_dir: canonical(PathBuf::from(git_dir)),
            common_dir: canonical(common),
        })
    }

    /// The folders to watch and how deep. Whole folders, never single files,
    /// because git replaces files by renaming over them and because
    /// `MERGE_HEAD` and the rebase folders may not exist yet.
    pub fn watch_list(&self) -> Vec<(PathBuf, RecursiveMode)> {
        let mut list = vec![(self.git_dir.clone(), RecursiveMode::NonRecursive)];
        if self.common_dir != self.git_dir {
            list.push((self.common_dir.clone(), RecursiveMode::NonRecursive));
        }
        let refs = self.common_dir.join("refs");
        if refs.is_dir() {
            list.push((refs, RecursiveMode::Recursive));
        }
        // Repositories made with `--ref-format=reftable` keep refs here.
        let reftable = self.common_dir.join("reftable");
        if reftable.is_dir() {
            list.push((reftable, RecursiveMode::NonRecursive));
        }
        list
    }

    /// True when a change at `path` can change what the snapshot shows.
    /// Lock files never count: git writes `<name>.lock` and renames it onto
    /// `<name>`, and the rename is what is reported next. Remote refs never
    /// count either, because the snapshot leaves them out and a fetch must
    /// fire nothing.
    pub fn is_relevant(&self, path: &Path) -> bool {
        let locked = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".lock"));
        if locked {
            return false;
        }
        // In a linked worktree `HEAD` and the rebase folders count only in
        // its own git dir: the main repository's `HEAD` is not ours.
        let own = path
            .strip_prefix(&self.git_dir)
            .map(|rel| relevant_inside(rel, true))
            .unwrap_or(false);
        own || path
            .strip_prefix(&self.common_dir)
            .map(|rel| relevant_inside(rel, false))
            .unwrap_or(false)
    }

    /// True while git is in the middle of writing the index or HEAD. The
    /// settle step waits a little longer when this is true.
    pub fn lock_held(&self) -> bool {
        self.git_dir.join("index.lock").exists() || self.git_dir.join("HEAD.lock").exists()
    }
}

/// The decision for a path given relative to a git dir. `own` is true for the
/// work tree's own git dir, false for the common dir of a linked worktree.
fn relevant_inside(rel: &Path, own: bool) -> bool {
    let mut parts = rel
        .components()
        .map(|c| c.as_os_str().to_str().unwrap_or(""));
    match parts.next() {
        Some("refs") => parts.next() != Some("remotes"),
        Some("reftable") | Some("packed-refs") => true,
        Some(name) => own && OWN_NAMES.contains(&name),
        None => false,
    }
}

#[cfg(test)]
mod tests;
