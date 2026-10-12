//! Pure decisions: which changes a pair of repository snapshots shows. No git
//! calls, no threads, no clocks: everything comes in as arguments, so every
//! case is a unit test. The design is Decisions 8 and 9 of
//! plans/003-trigger-git/epic.md, checked against research note R01.

// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]

use std::collections::BTreeMap;

use trigger_core::Value;

pub type Payload = BTreeMap<String, Value>;

/// What the repository looked like at one moment, reduced to what the diff
/// needs. `refs` holds `refs/heads/*`, `refs/tags/*` and `refs/stash` only,
/// never remote refs, so a fetch changes nothing in it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoSnapshot {
    /// Full sha of HEAD, None on an unborn branch.
    pub head: Option<String>,
    /// Current branch, None when HEAD is detached.
    pub branch: Option<String>,
    /// Sha in MERGE_HEAD, set only while a merge is in progress.
    pub merge_head: Option<String>,
    /// True while `rebase-merge/` or `rebase-apply/` exists.
    pub rebase: bool,
    /// Files with unresolved conflicts. Only read when `merge_head` is set.
    pub unmerged: u32,
    /// Subject of the newest HEAD reflog entry, such as `commit: fix`.
    pub reflog_subject: Option<String>,
    /// Short sha of HEAD.
    pub short: Option<String>,
    /// First line of the HEAD commit message.
    pub message: Option<String>,
    /// Ref name to sha.
    pub refs: BTreeMap<String, String>,
}

/// One thing that happened between two snapshots. Each variant is one kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    RebaseStopped {
        branch: Option<String>,
        commit: Option<String>,
    },
    MergeConflict {
        branch: Option<String>,
        files: u32,
    },
    BranchSwitched {
        from: String,
        to: String,
        commit: Option<String>,
    },
    DetachedHead {
        from: String,
        commit: Option<String>,
    },
    Pulled {
        branch: Option<String>,
        commit: Option<String>,
        message: Option<String>,
    },
    Commit {
        branch: Option<String>,
        commit: Option<String>,
        message: Option<String>,
    },
    TagCreated {
        branch: Option<String>,
        tag: String,
    },
    StashCreated {
        branch: Option<String>,
    },
}

impl Change {
    /// The kind id this change fires.
    pub fn kind(&self) -> &'static str {
        match self {
            Change::RebaseStopped { .. } => "git.rebase-stopped",
            Change::MergeConflict { .. } => "git.merge-conflict",
            Change::BranchSwitched { .. } => "git.branch-switched",
            Change::DetachedHead { .. } => "git.detached-head",
            Change::Pulled { .. } => "git.pulled",
            Change::Commit { .. } => "git.commit",
            Change::TagCreated { .. } => "git.tag-created",
            Change::StashCreated { .. } => "git.stash-created",
        }
    }

    /// The branch a rule's `branch` glob is matched against: the new branch
    /// for a switch, the branch left for a detach, the current branch for the
    /// rest.
    pub fn branch(&self) -> Option<&str> {
        match self {
            Change::BranchSwitched { to, .. } => Some(to),
            Change::DetachedHead { from, .. } => Some(from),
            Change::RebaseStopped { branch, .. }
            | Change::MergeConflict { branch, .. }
            | Change::Pulled { branch, .. }
            | Change::Commit { branch, .. }
            | Change::TagCreated { branch, .. }
            | Change::StashCreated { branch } => branch.as_deref(),
        }
    }
}

#[cfg(test)]
mod tests;
