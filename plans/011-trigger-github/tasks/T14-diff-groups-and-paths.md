---
id: T14
epic: 011-trigger-github
title: Add Group, request paths and the seen set loop
status: todo
depends_on: [T13]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-github/src/logic/diff.rs
  - src-tauri/crates/trigger-github/src/logic/testutil.rs
  - src-tauri/crates/trigger-github/src/logic.rs
---

## Goal

The crate has `logic/diff.rs` with the vocabulary of polling: the `Group` enum, which groups a kind needs (`groups_for`), the request path of each group (`path_for`, `feed_key`), what is remembered between polls (`Feed`, `RepoState` with seen keys and etags, and `State::repo` in `logic.rs`), the `Fire` result, and `each_new`, the one loop every group uses to fire a rule for each unseen item. The groups themselves come in later cards. A minimal test helper `armed` exists for them.

## Read first

- `plans/011-trigger-github/epic.md` Decision 9 and the Shape section (the `Group` enum, "four shared endpoint groups")
- `plans/011-trigger-github/research/R01-endpoints.md` the four shared GETs at the top of the Answer, the CI `branch=` note and the pr-commented `since` note
- `src-tauri/crates/trigger-github/src/logic.rs` the file you replace (`Armed`, `State`)
- `src-tauri/crates/trigger-github/src/logic/payload.rs` `Payload` (created by T13)

## Steps

1. Create `src-tauri/crates/trigger-github/src/logic/diff.rs` with exactly this.

How it fits together:

- A request path is one `(repo, group)` pair, built by `path_for`. `branch` is a literal branch (from `Parsed::literal_branch`) and only matters for `Group::Runs`; `since` only matters for the two comment groups. Both are left out when `None`.
- `feed_key` is `path_for` without `since`. It names the remembered `Feed` for a request, so two rules with different literal branches on CI have separate feeds, and a changing `since` never splits one.
- A `Feed` has `seeded` (false until the first answer is taken in) and `seen`, the keys already seen per kind. `RepoState` has the feeds, the numbers of the pull requests found (`prs`, for `github.pr-commented`) and the `ETag` per full path.
- `each_new` fires every rule of one kind for every item whose key is not yet in the seen set, then adds the key. An item with no key (`None`) is not a candidate for that kind and is not remembered.
- `Spec` has `&dyn Fn` fields because each group needs closures that capture `me`, `since` or the pull request numbers.

```rust
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
```

2. Create `src-tauri/crates/trigger-github/src/logic/testutil.rs` with exactly this. Only `armed` for now. T15 replaces this file with a longer one.

```rust
//! Helpers shared by the tests in this folder. Compiled for tests only.

use serde_json::Value;
use trigger_core::testkit::rule;

use super::Armed;

/// A rule as the poll loop holds it, parsed by the real `rule::parse`.
pub fn armed(id: &str, kind: &str, when: Value) -> Armed {
    let r = rule(id, kind, when);
    Armed {
        id: r.id.clone(),
        kind: r.kind.clone(),
        parsed: crate::rule::parse(&r).unwrap(),
    }
}
```

3. Replace the whole of `src-tauri/crates/trigger-github/src/logic.rs` with exactly this. This replaces the whole file. New since T13: `pub mod diff;`, the `testutil` module declaration, `State` now holds `repos` and has `repo()`.

```rust
//! Pure decisions: given what `gh` answered, which rules fire and with what
//! payload. No child processes, no threads, no clocks: everything it needs
//! comes in as arguments, so every case is a unit test. Slice B fills it in.

// Removed in slice C, when the poll loop reads everything in this file.
#![allow(dead_code)]

pub mod backoff;
pub mod diff;
pub mod json;
pub mod payload;
pub mod response;
pub mod text;

#[cfg(test)]
mod testutil;

use std::collections::BTreeMap;

use trigger_core::RuleId;

use crate::rule::Parsed;

/// A rule this crate is polling for.
#[derive(Debug, Clone)]
pub struct Armed {
    pub id: RuleId,
    pub kind: String,
    pub parsed: Parsed,
}

/// Everything remembered between polls: by repository (`owner/name`), the
/// seen ids and etags in `diff::RepoState`.
#[derive(Debug, Default)]
pub struct State {
    pub repos: BTreeMap<String, diff::RepoState>,
}

impl State {
    /// The memory of one repository, made empty on first use.
    pub fn repo(&mut self, repo: &str) -> &mut diff::RepoState {
        self.repos.entry(repo.to_string()).or_default()
    }
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib` passes with 52 tests (none new in this card)
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (the `source`, `readme` and `live` tests included)
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (warnings that `lib.rs` and `os.rs` still have template code are expected until slice C)
- [ ] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- The `diff` function itself and the group files (`pulls.rs`, `runs.rs`, `simple.rs`, `comments.rs`). Later cards.
- Tests for this file. T15 adds them.
- Removing `#![allow(dead_code)]` from `logic.rs`.

## If stuck

- If the build fails, compare each file with its step character by character.
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
