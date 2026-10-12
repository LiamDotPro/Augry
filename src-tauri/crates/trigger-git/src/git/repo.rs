//! `GitRepo`: a repository on disk, answered by running git in its work tree.
//! The pure parsing of git's output is in small functions at the bottom so it
//! has tests that need no git.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::{run, value, FETCH_TIMEOUT, TIMEOUT};
use crate::logic::repo::Repo;
use crate::logic::RepoSnapshot;

/// A repository on disk, read by running git in its work tree.
#[derive(Debug, Clone)]
pub struct GitRepo {
    path: PathBuf,
}

impl GitRepo {
    /// `path` is the work tree root, the rule's `repo`.
    pub fn new(path: impl Into<PathBuf>) -> GitRepo {
        GitRepo { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn value(&self, args: &[&str]) -> Option<String> {
        value(&self.path, args)
    }

    /// Commits `HEAD` is behind its upstream, counted from the remote refs on
    /// disk. With `fetch` the remote is asked first; a fetch that fails
    /// (offline, no remote) is ignored and the count uses what is on disk.
    /// `--no-tags` keeps the fetch from creating local tags, which would show
    /// up as `git.tag-created`. None without an upstream or on a detached HEAD.
    fn read_behind(&self, fetch: bool) -> Option<u32> {
        if fetch {
            let _ = run(
                &self.path,
                &["fetch", "--quiet", "--no-tags"],
                FETCH_TIMEOUT,
            );
        }
        self.value(&["rev-list", "--count", "HEAD..@{u}"])?
            .parse()
            .ok()
    }

    /// Age in seconds of the oldest modified tracked file, from the modified
    /// times of the files `status` lists. Untracked files are left out, and so
    /// is a deleted file, which has no modified time to read.
    fn read_oldest_change_secs(&self) -> Option<u64> {
        let text = self.value(&["status", "--porcelain=v2", "-z", "--untracked-files=no"])?;
        let now = SystemTime::now();
        changed_paths(&text)
            .iter()
            .filter_map(|path| {
                let modified = std::fs::metadata(self.path.join(path))
                    .ok()?
                    .modified()
                    .ok()?;
                Some(age_secs(now, modified))
            })
            .max()
    }

    /// The sha in MERGE_HEAD (None unless a merge is in progress) and whether
    /// a rebase is in progress. `--git-path` finds the files for a plain
    /// repository and for a linked worktree alike, so no path is built by hand.
    fn operation_state(&self) -> (Option<String>, bool) {
        let paths = self.value(&[
            "rev-parse",
            "--git-path",
            "MERGE_HEAD",
            "--git-path",
            "rebase-merge",
            "--git-path",
            "rebase-apply",
        ]);
        let Some(paths) = paths else {
            return (None, false);
        };
        // The paths are relative to the work tree, or absolute; `join` copes
        // with both.
        let mut lines = paths.lines().map(|line| self.path.join(line));
        let merge_head = lines
            .next()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .map(|text| text.trim().to_string())
            .filter(|sha| !sha.is_empty());
        let rebase = lines.any(|dir| dir.exists());
        (merge_head, rebase)
    }

    /// Files with unresolved conflicts, from `status`.
    fn unmerged(&self) -> u32 {
        self.value(&["status", "--porcelain=v2", "-z", "--untracked-files=no"])
            .map(|text| count_unmerged(&text))
            .unwrap_or(0)
    }

    /// Reads the repository into a `RepoSnapshot`. Each field comes from one
    /// read only git call, and a call that fails leaves its field empty, so an
    /// unborn branch (no commits yet) is a snapshot with `head: None`.
    fn read_snapshot(&self) -> Result<RepoSnapshot, String> {
        let probe =
            run(&self.path, &["rev-parse", "--git-dir"], TIMEOUT).map_err(|e| e.to_string())?;
        if !probe.ok {
            return Err(format!("not a git repository: {}", self.path.display()));
        }
        let head = self.value(&["rev-parse", "--verify", "-q", "HEAD"]);
        let branch = self.value(&["symbolic-ref", "-q", "--short", "HEAD"]);
        let (short, message) = self
            .value(&["log", "-1", "--format=%h%x00%s", "HEAD"])
            .map(|text| parse_log(&text))
            .unwrap_or_default();
        let reflog_subject = self.value(&["reflog", "-1", "--format=%gs", "HEAD"]);
        // Local branches, tags and the stash only. Remote refs stay out so a
        // fetch changes nothing in the snapshot (Decision 8).
        let refs = self
            .value(&[
                "for-each-ref",
                "--count=2000",
                "--format=%(refname)%00%(objectname)",
                "refs/heads",
                "refs/tags",
                "refs/stash",
            ])
            .map(|text| parse_refs(&text))
            .unwrap_or_default();
        let (merge_head, rebase) = self.operation_state();
        // Counting conflicts costs a `status`, so only a merge in progress
        // pays for it.
        let unmerged = if merge_head.is_some() {
            self.unmerged()
        } else {
            0
        };
        Ok(RepoSnapshot {
            head,
            branch,
            merge_head,
            rebase,
            unmerged,
            reflog_subject,
            short,
            message,
            refs,
        })
    }
}

impl Repo for GitRepo {
    fn snapshot(&self) -> Result<RepoSnapshot, String> {
        self.read_snapshot()
    }

    fn behind(&self, fetch: bool) -> Option<u32> {
        self.read_behind(fetch)
    }

    fn oldest_change_secs(&self) -> Option<u64> {
        self.read_oldest_change_secs()
    }
}

/// Counts the `u` records of `status --porcelain=v2 -z`. A rename record (`2`)
/// is followed by one more record holding the old path, which is skipped so a
/// path that happens to start with `u ` is not counted.
pub(crate) fn count_unmerged(text: &str) -> u32 {
    let mut records = text.split('\0');
    let mut count = 0;
    while let Some(record) = records.next() {
        if record.starts_with("2 ") {
            records.next();
        } else if record.starts_with("u ") {
            count += 1;
        }
    }
    count
}

/// The paths in `status --porcelain=v2 -z --untracked-files=no`. Each record
/// starts with its type and then has a fixed number of space separated fields
/// before the path: `1` ordinary (8 before the path), `2` renamed or copied
/// (9, and the old path follows as the next record, which is skipped), `u`
/// unmerged (10). The path itself may contain spaces.
pub(crate) fn changed_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut records = text.split('\0');
    while let Some(record) = records.next() {
        let parts = match record.get(..2) {
            Some("1 ") => 9,
            Some("2 ") => {
                records.next();
                10
            }
            Some("u ") => 11,
            _ => continue,
        };
        if let Some(path) = record.splitn(parts, ' ').nth(parts - 1) {
            paths.push(path.to_string());
        }
    }
    paths
}

/// Whole seconds from `then` to `now`; 0 when `then` is in the future.
pub(crate) fn age_secs(now: SystemTime, then: SystemTime) -> u64 {
    now.duration_since(then).map(|d| d.as_secs()).unwrap_or(0)
}

/// Splits the output of `log -1 --format=%h%x00%s` into the short sha and the
/// first line of the message. Either is None when empty.
pub(crate) fn parse_log(text: &str) -> (Option<String>, Option<String>) {
    let (short, message) = text.split_once('\0').unwrap_or((text, ""));
    let some = |s: &str| (!s.is_empty()).then(|| s.to_string());
    (some(short), some(message))
}

/// Parses lines of `refname NUL sha` into a map. Lines without the NUL are
/// skipped.
pub(crate) fn parse_refs(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once('\0'))
        .map(|(name, sha)| (name.to_string(), sha.to_string()))
        .collect()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod poll_tests;
