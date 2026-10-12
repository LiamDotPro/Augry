//! `GitRepo`: a repository on disk, answered by running git in its work tree.
//! The pure parsing of git's output is in small functions at the bottom so it
//! has tests that need no git.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{run, value, TIMEOUT};
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
        Ok(RepoSnapshot {
            head,
            branch,
            reflog_subject,
            short,
            message,
            refs,
            ..RepoSnapshot::default()
        })
    }
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
