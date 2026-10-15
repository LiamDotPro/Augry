//! Pure transition logic for the three polled kinds. The glue polls a `Repo`
//! (or scans a folder) on the rule's interval and feeds each answer to one of
//! these; they say whether the answer fires. No git, no clock, no threads.
//! Decision 10 of plans/003-trigger-git/epic.md.

use std::collections::BTreeSet;
use std::path::Path;

use trigger_core::Value;

use super::Payload;

/// `git.behind-remote`: fires when the count of commits behind the upstream
/// goes from 0 to above 0.
#[derive(Debug, Default)]
pub struct BehindWatch {
    last: Option<u32>,
}

impl BehindWatch {
    pub fn new() -> BehindWatch {
        BehindWatch::default()
    }

    /// Feeds one poll. `behind` is what `Repo::behind` returned. Returns the
    /// count when this poll fires.
    ///
    /// - The first count is a baseline and never fires, even above 0.
    /// - After a fire the count must return to 0 before it can fire again, so
    ///   2 then 5 is one event, and 0, 2, 0, 1 is two.
    /// - None (no upstream, git failed) says nothing: the last count stays.
    pub fn observe(&mut self, behind: Option<u32>) -> Option<u32> {
        let now = behind?;
        let fires = self.last == Some(0) && now > 0;
        self.last = Some(now);
        fires.then_some(now)
    }
}

/// The payload of `git.behind-remote`: exactly the fields listed in `KINDS`.
pub fn behind_payload(repo: &str, branch: Option<&str>, behind: u32) -> Payload {
    let mut out = Payload::new();
    out.insert("repo".into(), Value::from(repo));
    out.insert("branch".into(), Value::from(branch.unwrap_or_default()));
    out.insert("behind".into(), Value::from(behind));
    out
}

/// `git.uncommitted-older-than`: fires once when the oldest modified tracked
/// file has waited `older_than_min` minutes, and arms again when the tree is
/// clean.
#[derive(Debug, Default)]
pub struct UncommittedWatch {
    fired: bool,
}

impl UncommittedWatch {
    pub fn new() -> UncommittedWatch {
        UncommittedWatch::default()
    }

    /// Feeds one poll. `age_secs` is what `Repo::oldest_change_secs` returned.
    /// Returns the whole minutes the oldest change has waited when this poll
    /// fires.
    ///
    /// - None means a clean tree (or a failed read): the watch arms again.
    /// - At or past the limit it fires once, then stays quiet until a clean
    ///   tree. Committing some of the files does not arm it again.
    /// - There is no baseline: work that is already old when the rule starts
    ///   fires on the first poll.
    pub fn observe(&mut self, age_secs: Option<u64>, older_than_min: u64) -> Option<u64> {
        let Some(age) = age_secs else {
            self.fired = false;
            return None;
        };
        if self.fired || age < older_than_min.saturating_mul(60) {
            return None;
        }
        self.fired = true;
        Some(age / 60)
    }
}

/// The payload of `git.uncommitted-older-than`: exactly the fields listed in
/// `KINDS`.
pub fn uncommitted_payload(repo: &str, branch: Option<&str>, minutes: u64) -> Payload {
    let mut out = Payload::new();
    out.insert("repo".into(), Value::from(repo));
    out.insert("branch".into(), Value::from(branch.unwrap_or_default()));
    out.insert("minutes".into(), Value::from(minutes));
    out
}

/// `git.repo-cloned`: fires for each new child of a folder that holds a
/// repository. The first scan is the baseline.
#[derive(Debug, Default)]
pub struct CloneScan {
    known: Option<BTreeSet<String>>,
}

impl CloneScan {
    pub fn new() -> CloneScan {
        CloneScan::default()
    }

    /// Feeds one scan. `repos` is the names of the children of the folder
    /// that contain `.git` (the OS layer lists them). Returns the names that
    /// were not in the previous scan, in sorted order.
    ///
    /// - The first scan returns nothing, whatever it holds.
    /// - A repository that disappears is forgotten, so cloning it again
    ///   fires again.
    pub fn observe(&mut self, repos: BTreeSet<String>) -> Vec<String> {
        let new = match &self.known {
            Some(known) => repos.difference(known).cloned().collect(),
            None => Vec::new(),
        };
        self.known = Some(repos);
        new
    }
}

/// The payload of `git.repo-cloned`: exactly the fields listed in `KINDS`.
pub fn cloned_payload(folder: &Path, name: &str) -> Payload {
    let mut out = Payload::new();
    let path = folder.join(name);
    out.insert("path".into(), Value::from(path.to_string_lossy().as_ref()));
    out
}
