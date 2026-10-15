//! Pure decisions: which changes a pair of repository snapshots shows. No git
//! calls, no threads, no clocks: everything comes in as arguments, so every
//! case is a unit test. The design is Decisions 8 and 9 of
//! plans/003-trigger-git/epic.md, checked against research note R01.

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

/// Absent text becomes an empty string, so every payload field is always
/// there and `${trigger.branch}` never has to cope with a missing key.
fn text(value: &Option<String>) -> Value {
    Value::from(value.clone().unwrap_or_default())
}

impl Change {
    /// The event payload: the fields listed for this kind in `KINDS`, and no
    /// others. `repo` is the rule's repository path.
    pub fn payload(&self, repo: &str) -> Payload {
        let mut out = Payload::new();
        let mut put = |key: &str, value: Value| {
            out.insert(key.to_string(), value);
        };
        put("repo", Value::from(repo));
        match self {
            Change::RebaseStopped { branch, commit } => {
                put("branch", text(branch));
                put("commit", text(commit));
            }
            Change::MergeConflict { branch, files } => {
                put("branch", text(branch));
                put("files", Value::from(*files));
            }
            Change::BranchSwitched { from, to, commit } => {
                put("branch", Value::from(to.as_str()));
                put("from", Value::from(from.as_str()));
                put("to", Value::from(to.as_str()));
                put("commit", text(commit));
            }
            Change::DetachedHead { from, commit } => {
                put("branch", Value::from(from.as_str()));
                put("commit", text(commit));
            }
            Change::Pulled {
                branch,
                commit,
                message,
            }
            | Change::Commit {
                branch,
                commit,
                message,
            } => {
                put("branch", text(branch));
                put("commit", text(commit));
                put("message", text(message));
            }
            Change::TagCreated { branch, tag } => {
                put("branch", text(branch));
                put("tag", Value::from(tag.as_str()));
            }
            Change::StashCreated { branch } => {
                put("branch", text(branch));
            }
        }
        out
    }
}

/// The changes between two snapshots, in the order they are reported:
/// rebase stopped, merge conflict, branch switched, detached head, pulled or
/// commit, tag created, stash created. Each kind at most once.
pub fn step(prev: &RepoSnapshot, next: &RepoSnapshot) -> Vec<Change> {
    let mut out = Vec::new();

    // HEAD is detached and moves on every step of a rebase, so while one is
    // running only its start can be reported.
    if next.rebase {
        if !prev.rebase {
            out.push(Change::RebaseStopped {
                branch: prev.branch.clone(),
                commit: next.short.clone(),
            });
        }
        return out;
    }

    // A merge with no conflicting files (`merge --no-commit`) fires nothing.
    if prev.merge_head.is_none() && next.merge_head.is_some() && next.unmerged > 0 {
        out.push(Change::MergeConflict {
            branch: next.branch.clone(),
            files: next.unmerged,
        });
    }

    // A switch needs a branch on both sides. A new branch made by
    // `checkout -b` is a switch too.
    if let (Some(from), Some(to)) = (&prev.branch, &next.branch) {
        if from != to {
            out.push(Change::BranchSwitched {
                from: from.clone(),
                to: to.clone(),
                commit: next.short.clone(),
            });
        }
    }

    // Leaving a branch for a bare commit. A merge in progress is not that.
    if let (Some(from), None) = (&prev.branch, &next.branch) {
        if next.merge_head.is_none() {
            out.push(Change::DetachedHead {
                from: from.clone(),
                commit: next.short.clone(),
            });
        }
    }

    // HEAD moved with the branch unchanged: the reflog subject says why. A
    // reset or the end of a rebase has another subject and fires nothing.
    if prev.branch == next.branch && next.head.is_some() && prev.head != next.head {
        let subject = next.reflog_subject.as_deref().unwrap_or("");
        if subject.starts_with("pull") || subject.starts_with("merge") {
            out.push(Change::Pulled {
                branch: next.branch.clone(),
                commit: next.short.clone(),
                message: next.message.clone(),
            });
        } else if subject.starts_with("commit") || subject.starts_with("cherry-pick") {
            out.push(Change::Commit {
                branch: next.branch.clone(),
                commit: next.short.clone(),
                message: next.message.clone(),
            });
        }
    }

    // Only the first new tag by name: the hub's cool down would drop a second
    // event for the same rule anyway.
    let new_tag = next
        .refs
        .keys()
        .filter(|name| name.starts_with("refs/tags/") && !prev.refs.contains_key(*name))
        .map(|name| name.trim_start_matches("refs/tags/"))
        .next();
    if let Some(tag) = new_tag {
        out.push(Change::TagCreated {
            branch: next.branch.clone(),
            tag: tag.to_string(),
        });
    }

    // A stash that appeared, or one pushed on top of another. Removing the
    // last stash (pop or drop) removes the ref and fires nothing.
    if let Some(stash) = next.refs.get("refs/stash") {
        if prev.refs.get("refs/stash") != Some(stash) {
            out.push(Change::StashCreated {
                branch: next.branch.clone(),
            });
        }
    }

    out
}

/// Like `step`, for the glue that may not have a previous snapshot yet. The
/// first snapshot is a baseline: it never fires anything.
pub fn observe(prev: Option<&RepoSnapshot>, next: &RepoSnapshot) -> Vec<Change> {
    match prev {
        Some(prev) => step(prev, next),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests;

pub mod repo;

#[cfg(test)]
pub mod fake;

pub mod polled;

#[cfg(test)]
mod polled_tests;

pub mod settle;
