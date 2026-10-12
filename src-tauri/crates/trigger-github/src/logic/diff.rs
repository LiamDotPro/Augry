//! Which rules fire when a poll brings news. Pure.
//!
//! The kinds share four requests per repository (R01): pull requests, workflow
//! runs, releases and issues assigned to you, plus the two comment lists for
//! `github.pr-commented`. A request is made once however many rules read it.
//! This file has the vocabulary: the groups, the request paths, what is
//! remembered between polls, and `each_new`, the one loop every group uses.
//! The groups themselves are in `pulls.rs`, `runs.rs`, `simple.rs` and
//! `comments.rs`, and `diff` at the bottom of this file dispatches to them.

use std::collections::{BTreeMap, BTreeSet};

use trigger_core::RuleId;

use super::json::Value;
use super::payload::Payload;
use super::text::encode;
use super::Armed;

/// The kind ids, so the logic never repeats a string.
pub mod kind {
    pub const ISSUE_ASSIGNED: &str = "github.issue-assigned";
    pub const REVIEW_REQUESTED: &str = "github.review-requested";
    pub const CI_FAILED: &str = "github.ci-failed";
    pub const CI_PASSED: &str = "github.ci-passed";
    pub const PR_COMMENTED: &str = "github.pr-commented";
    pub const PR_MERGED: &str = "github.pr-merged";
    pub const RELEASE_PUBLISHED: &str = "github.release-published";
    pub const DEPENDENCY_PR_OPENED: &str = "github.dependency-pr-opened";
}

/// One request per repository. The order is the order to poll in: `Pulls`
/// comes first because the comment groups need the pull request numbers it
/// finds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Pulls,
    Runs,
    Releases,
    AssignedIssues,
    IssueComments,
    PullComments,
}

/// The groups a rule of this kind needs, in polling order. Empty for a kind
/// that is not ours.
pub fn groups_for(kind: &str) -> &'static [Group] {
    match kind {
        kind::ISSUE_ASSIGNED => &[Group::AssignedIssues],
        kind::REVIEW_REQUESTED | kind::PR_MERGED | kind::DEPENDENCY_PR_OPENED => &[Group::Pulls],
        kind::CI_FAILED | kind::CI_PASSED => &[Group::Runs],
        kind::RELEASE_PUBLISHED => &[Group::Releases],
        kind::PR_COMMENTED => &[Group::Pulls, Group::IssueComments, Group::PullComments],
        _ => &[],
    }
}

/// The request path for `gh api`. `me` is the signed in login. `since` (an
/// ISO 8601 time) only matters for the two comment groups and `branch` (a
/// literal branch name, from `Parsed::literal_branch`) only for `Runs`; both
/// are left out when `None`. Never use `--paginate`: page one is what we want.
pub fn path_for(
    repo: &str,
    group: Group,
    me: &str,
    since: Option<&str>,
    branch: Option<&str>,
) -> String {
    let mut path = match group {
        Group::Pulls => {
            format!("repos/{repo}/pulls?state=all&sort=updated&direction=desc&per_page=100")
        }
        Group::Runs => format!("repos/{repo}/actions/runs?status=completed&per_page=100"),
        Group::Releases => format!("repos/{repo}/releases?per_page=10"),
        Group::AssignedIssues => format!(
            "repos/{repo}/issues?state=open&assignee={}&sort=updated&direction=desc&per_page=100",
            encode(me)
        ),
        Group::IssueComments => {
            format!("repos/{repo}/issues/comments?sort=updated&direction=desc&per_page=100")
        }
        Group::PullComments => {
            format!("repos/{repo}/pulls/comments?sort=updated&direction=desc&per_page=100")
        }
    };
    if let (Group::Runs, Some(branch)) = (group, branch) {
        path.push_str(&format!("&branch={}", encode(branch)));
    }
    if let (Group::IssueComments | Group::PullComments, Some(since)) = (group, since) {
        path.push_str(&format!("&since={}", encode(since)));
    }
    path
}

/// The name a feed is remembered under: the path without `since`, because
/// `since` is the same for the whole run anyway and must not split the feed.
pub fn feed_key(repo: &str, group: Group, me: &str, branch: Option<&str>) -> String {
    path_for(repo, group, me, None, branch)
}

/// One rule firing. The poll loop turns it into a `trigger_core::Event`.
#[derive(Debug, Clone, PartialEq)]
pub struct Fire {
    pub rule: RuleId,
    pub kind: String,
    pub payload: Payload,
}

/// What is remembered about one request path.
#[derive(Debug, Default)]
pub struct Feed {
    /// False until the first answer has been taken in. That answer only
    /// records what exists; it fires nothing.
    pub seeded: bool,
    /// Per kind, the keys already seen: the item `id`, or `id:run_attempt`
    /// for workflow runs.
    pub seen: BTreeMap<String, BTreeSet<String>>,
}

/// What is remembered about one repository.
#[derive(Debug, Default)]
pub struct RepoState {
    /// By `feed_key`.
    pub feeds: BTreeMap<String, Feed>,
    /// Numbers of the pull requests the last answers listed. A comment on an
    /// issue is only ours when its number is in here.
    pub prs: BTreeSet<u64>,
    /// The last `ETag` per full request path, verbatim.
    pub etags: BTreeMap<String, String>,
}

impl RepoState {
    pub fn etag(&self, path: &str) -> Option<&str> {
        self.etags.get(path).map(String::as_str)
    }

    pub fn set_etag(&mut self, path: &str, etag: &str) {
        self.etags.insert(path.to_string(), etag.to_string());
    }
}

/// How one kind reads a list of items.
pub struct Spec<'a> {
    pub kind: &'a str,
    /// The key of an item for this kind, or `None` when the item is not a
    /// candidate for it at all.
    pub key: &'a dyn Fn(&Value) -> Option<String>,
    /// Whether this rule wants the item (its branch glob, its authors).
    pub accept: &'a dyn Fn(&Armed, &Value) -> bool,
    pub build: fn(&Value) -> Payload,
}

/// Fires every rule of `spec.kind` for every item that has a key we have not
/// seen, then remembers the key. Items come newest first and fire in that
/// order. Does nothing when no rule has the kind.
pub fn each_new(
    feed: &mut Feed,
    spec: &Spec,
    rules: &[&Armed],
    items: &[Value],
    out: &mut Vec<Fire>,
) {
    let mine: Vec<&Armed> = rules
        .iter()
        .copied()
        .filter(|r| r.kind == spec.kind)
        .collect();
    if mine.is_empty() {
        return;
    }
    let seen = feed.seen.entry(spec.kind.to_string()).or_default();
    for item in items {
        let Some(key) = (spec.key)(item) else {
            continue;
        };
        if !seen.insert(key) {
            continue;
        }
        for rule in &mine {
            if (spec.accept)(rule, item) {
                out.push(Fire {
                    rule: rule.id.clone(),
                    kind: rule.kind.clone(),
                    payload: (spec.build)(item),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests;
