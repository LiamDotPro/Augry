---
id: T24
epic: 011-trigger-github
title: Plan the poll jobs and add a fake gh for tests
status: done
depends_on: [T23]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-github/src/logic/poll.rs
  - src-tauri/crates/trigger-github/src/logic/poll/plan_tests.rs
  - src-tauri/crates/trigger-github/src/logic/fake_gh.rs
  - src-tauri/crates/trigger-github/src/logic.rs
---

## Goal

The crate has `logic/poll.rs` with `Job`, `plan` and a `Poller` that can be built but does not poll yet. `plan` turns the armed rules into the list of requests to repeat: one `Job` per (repository, group), plus one per literal branch for CI runs, in the order the poll loop must use (pull requests before the comment lists). Eight tests prove the grouping. `logic/fake_gh.rs` is a stand in for `gh` for the next cards' tests. It is compiled for tests only.

## Read first

- `plans/011-trigger-github/epic.md` Decision 9 and the "Shape" section (what a job shares between rules)
- `src-tauri/crates/trigger-github/src/logic/diff.rs` `Group`, `groups_for`, `path_for`, `feed_key` (T14). Read their doc comments: `since` only matters for the two comment groups, a literal branch only for `Runs`, `Group` is ordered so `Pulls` is first
- `src-tauri/crates/trigger-github/src/rule.rs` `Parsed`, `literal_branch` (T06)
- `src-tauri/crates/trigger-github/src/logic/cli.rs` the `Gh` trait the fake implements (T20)
- `src-tauri/crates/trigger-github/src/logic.rs` the file that gets three new lines

## Steps

1. Create `src-tauri/crates/trigger-github/src/logic/poll.rs` with exactly this.

How a job is made:

- A rule needs the groups `groups_for(kind)` returns. For each, the job is found by (repo, group, branch). The branch part is the rule's literal branch for `Group::Runs` only, so `branch: "trunk"` on a CI rule asks GitHub for trunk runs, while a glob or no branch shares the unfiltered request. On every other group the branch is ignored.
- The job keeps the indexes of the rules that read it. Its `interval_s` is the shortest of theirs. `due` starts at 0, so every job is polled on the first tick.
- `key` is `feed_key` and `path` is `path_for` with `since`. The jobs come out sorted by repository, then group, then branch.
- `Poller::new` stores the rules and `since = iso(start_s)` and plans no jobs yet: the paths contain `me`, which is only known after `gh api user` (next card).

```rust
//! The brain of the poll loop. Pure: the time comes in as whole seconds since
//! the Unix epoch and `gh` comes in as the `Gh` trait, so a test drives whole
//! afternoons of polling with a fake `gh` and a number for the clock.
//!
//! One `Job` is one request that is repeated: a repository, a group (see
//! `diff::Group`) and, for CI runs, a literal branch. Every rule that needs
//! that request is fed from its one answer. Jobs run one after another, never
//! at the same time, and one `Backoff` covers all of them.

use std::collections::BTreeMap;

use super::backoff::Backoff;
use super::diff::{feed_key, groups_for, path_for, Group};
use super::text::iso;
use super::{Armed, State};

/// Seconds to wait before asking `gh` who the user is again, after it failed.
pub const LOGIN_RETRY_S: u64 = 60;

/// One request that is made again and again.
#[derive(Debug)]
pub struct Job {
    pub repo: String,
    pub group: Group,
    /// Which of the poller's rules read this answer, as indexes.
    pub rules: Vec<usize>,
    /// The shortest `interval_s` of those rules.
    pub interval_s: u64,
    /// The name of the remembered feed, from `diff::feed_key`.
    pub key: String,
    /// The full request path, with `since` for the comment groups.
    pub path: String,
    /// Not polled before this time. Zero means at once.
    pub due: u64,
    /// The last `X-Poll-Interval` GitHub sent for this request.
    pub hint_s: Option<u64>,
    /// The repository answered 404: poll it less often.
    pub not_found: bool,
    /// The user has been told about the last problem with this request.
    pub reported: bool,
}

/// Turns the rules into jobs, in polling order: by repository, then by group
/// (`Group` is ordered so that `Pulls` comes before the comment groups, which
/// need its pull request numbers), then by branch. `me` is the signed in login
/// and `since` the ISO 8601 start time, both only used to build paths.
pub fn plan(rules: &[Armed], me: &str, since: &str) -> Vec<Job> {
    let mut jobs: BTreeMap<(String, Group, Option<String>), Job> = BTreeMap::new();
    for (index, rule) in rules.iter().enumerate() {
        let repo = &rule.parsed.when.repo;
        for &group in groups_for(&rule.kind) {
            // A literal branch is part of the request for CI runs only, because
            // GitHub filters on it: `branch: "trunk"` gets its own feed of trunk runs.
            let branch = match group {
                Group::Runs => rule.parsed.literal_branch().map(str::to_string),
                _ => None,
            };
            let job = jobs
                .entry((repo.clone(), group, branch.clone()))
                .or_insert_with(|| Job {
                    repo: repo.clone(),
                    group,
                    rules: Vec::new(),
                    interval_s: u64::MAX,
                    key: feed_key(repo, group, me, branch.as_deref()),
                    path: path_for(repo, group, me, Some(since), branch.as_deref()),
                    due: 0,
                    hint_s: None,
                    not_found: false,
                    reported: false,
                });
            job.rules.push(index);
            job.interval_s = job.interval_s.min(rule.parsed.when.interval_s);
        }
    }
    jobs.into_values().collect()
}

pub struct Poller {
    rules: Vec<Armed>,
    /// `text::iso` of the start time. The same value goes into every comment
    /// request and into `diff`, so a comment from before the start never fires.
    since: String,
    /// The signed in login. Read from `gh` on the first tick that can.
    me: Option<String>,
    login_retry_at: u64,
    jobs: Vec<Job>,
    state: State,
    backoff: Backoff,
    /// True once `gh` said it has no login. A stopped poller does nothing.
    stopped: bool,
}

impl Poller {
    /// `start_s` is the start time in whole seconds since the Unix epoch.
    pub fn new(rules: Vec<Armed>, start_s: u64) -> Poller {
        Poller {
            rules,
            since: iso(start_s),
            me: None,
            login_retry_at: 0,
            jobs: Vec::new(),
            state: State::default(),
            backoff: Backoff::new(),
            stopped: false,
        }
    }

    /// The jobs, empty until the login is known.
    #[cfg(test)]
    pub fn jobs(&self) -> &[Job] {
        &self.jobs
    }

    /// True when `gh` lost its login and polling ended for good.
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }
}

#[cfg(test)]
mod plan_tests;
```

2. Create `src-tauri/crates/trigger-github/src/logic/poll/plan_tests.rs` with exactly this.

```rust
//! Tests for `plan`: which requests the rules need, and in which order.

use serde_json::json;

use super::*;
use crate::logic::diff::kind;
use crate::logic::testutil::{armed, ME};

const SINCE: &str = "2026-10-12T00:00:00Z";

fn rule(id: &str, kind: &str, extra: serde_json::Value) -> Armed {
    let mut when = json!({ "repo": "cli/cli" });
    for (k, v) in extra.as_object().unwrap() {
        when[k] = v.clone();
    }
    armed(id, kind, when)
}

fn every_kind(repo: &str) -> Vec<Armed> {
    crate::KINDS
        .iter()
        .map(|k| armed(&format!("{repo}/{}", k.id), k.id, json!({ "repo": repo })))
        .collect()
}

fn groups(jobs: &[Job]) -> Vec<Group> {
    jobs.iter().map(|j| j.group).collect()
}

#[test]
fn one_rule_makes_one_job_that_is_due_at_once() {
    let rules = vec![rule("p/rel", kind::RELEASE_PUBLISHED, json!({}))];
    let jobs = plan(&rules, ME, SINCE);
    assert_eq!(jobs.len(), 1);
    let job = &jobs[0];
    assert_eq!(job.repo, "cli/cli");
    assert_eq!(job.group, Group::Releases);
    assert_eq!(job.path, "repos/cli/cli/releases?per_page=10");
    assert_eq!(job.rules, [0]);
    assert_eq!(job.interval_s, 120);
    assert_eq!(job.due, 0);
    assert!(!job.not_found && !job.reported && job.hint_s.is_none());
}

#[test]
fn every_kind_makes_six_jobs_with_pull_requests_first() {
    let rules = every_kind("cli/cli");
    let jobs = plan(&rules, ME, SINCE);
    assert_eq!(
        groups(&jobs),
        [
            Group::Pulls,
            Group::Runs,
            Group::Releases,
            Group::AssignedIssues,
            Group::IssueComments,
            Group::PullComments
        ]
    );
    // review-requested, pr-commented, pr-merged and dependency-pr-opened read the pull request list.
    assert_eq!(jobs[0].rules.len(), 4);
    assert_eq!(jobs[4].rules.len(), 1);
    assert_eq!(jobs[5].rules.len(), 1);
}

#[test]
fn rules_that_share_a_request_share_one_job_and_the_shortest_interval_wins() {
    let rules = vec![
        rule("p/a", kind::RELEASE_PUBLISHED, json!({ "interval_s": 300 })),
        rule("p/b", kind::RELEASE_PUBLISHED, json!({ "interval_s": 90 })),
        rule("p/c", kind::RELEASE_PUBLISHED, json!({})),
    ];
    let jobs = plan(&rules, ME, SINCE);
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].rules, [0, 1, 2]);
    assert_eq!(jobs[0].interval_s, 90);
}

#[test]
fn each_repository_has_its_own_jobs_in_repository_order() {
    let mut rules = every_kind("b/two");
    rules.extend(every_kind("a/one"));
    let jobs = plan(&rules, ME, SINCE);
    assert_eq!(jobs.len(), 12);
    assert!(jobs[..6].iter().all(|j| j.repo == "a/one"));
    assert!(jobs[6..].iter().all(|j| j.repo == "b/two"));
    assert_eq!(jobs[0].group, Group::Pulls);
    assert_eq!(jobs[6].group, Group::Pulls);
}

#[test]
fn a_literal_branch_gets_its_own_runs_job_and_a_glob_does_not() {
    let rules = vec![
        rule("p/trunk", kind::CI_FAILED, json!({ "branch": "trunk" })),
        rule("p/trunk-ok", kind::CI_PASSED, json!({ "branch": "trunk" })),
        rule("p/glob", kind::CI_FAILED, json!({ "branch": "release/*" })),
        rule("p/any", kind::CI_FAILED, json!({})),
    ];
    let jobs = plan(&rules, ME, SINCE);
    assert_eq!(jobs.len(), 2);
    // The job without a branch comes first and serves the glob and the open rule.
    assert_eq!(jobs[0].rules, [2, 3]);
    assert_eq!(
        jobs[0].path,
        "repos/cli/cli/actions/runs?status=completed&per_page=100"
    );
    assert_eq!(jobs[1].rules, [0, 1]);
    assert_eq!(
        jobs[1].path,
        "repos/cli/cli/actions/runs?status=completed&per_page=100&branch=trunk"
    );
    assert_ne!(jobs[0].key, jobs[1].key);
}

#[test]
fn a_branch_on_a_pull_request_rule_does_not_split_the_pull_request_job() {
    let rules = vec![
        rule("p/a", kind::PR_MERGED, json!({ "branch": "trunk" })),
        rule("p/b", kind::PR_MERGED, json!({ "branch": "release/*" })),
    ];
    let jobs = plan(&rules, ME, SINCE);
    assert_eq!(jobs.len(), 1);
    assert!(!jobs[0].path.contains("branch="));
    assert_eq!(jobs[0].rules, [0, 1]);
}

#[test]
fn paths_carry_me_and_since_where_they_belong() {
    let rules = every_kind("cli/cli");
    let jobs = plan(&rules, ME, SINCE);
    let path = |group| jobs.iter().find(|j| j.group == group).unwrap().path.clone();
    assert!(path(Group::AssignedIssues).contains("assignee=LiamDotPro"));
    assert!(path(Group::IssueComments).ends_with("&since=2026-10-12T00%3A00%3A00Z"));
    assert!(path(Group::PullComments).ends_with("&since=2026-10-12T00%3A00%3A00Z"));
    assert!(!path(Group::Pulls).contains("since"));
    assert!(!path(Group::Releases).contains("since"));
    // The remembered feed never carries `since`.
    let key = |group| jobs.iter().find(|j| j.group == group).unwrap().key.clone();
    assert!(!key(Group::IssueComments).contains("since"));
}

#[test]
fn a_poller_knows_no_jobs_until_it_knows_who_you_are() {
    let poller = Poller::new(every_kind("cli/cli"), 1_791_763_200);
    assert!(poller.jobs().is_empty());
    assert!(!poller.is_stopped());
}
```

3. Create `src-tauri/crates/trigger-github/src/logic/fake_gh.rs` with exactly this.

`FakeGh` serves queued answers by request path (the last answer for a path repeats; a path with none gets a 404), records every request with the `If-None-Match` it carried, and answers `login()` with what the test set. Clones share their state, so a test keeps one and gives the other to the poller or the thread. The small functions at the bottom build `GhResponse` values.

```rust
//! A stand in for `gh` that serves saved answers and remembers what it was
//! asked. Compiled for tests only.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

use super::cli::Gh;
use super::response::GhResponse;

#[derive(Default)]
struct Inner {
    me: Option<String>,
    logins: usize,
    answers: BTreeMap<String, VecDeque<GhResponse>>,
    calls: Vec<(String, Option<String>)>,
}

/// Clones share their state, so a test keeps one and gives the poller another.
#[derive(Clone, Default)]
pub struct FakeGh {
    inner: Arc<Mutex<Inner>>,
}

impl FakeGh {
    /// `me` is what `login()` answers; None means `gh` cannot say.
    pub fn new(me: Option<&str>) -> FakeGh {
        let gh = FakeGh::default();
        gh.set_me(me);
        gh
    }

    pub fn set_me(&self, me: Option<&str>) {
        self.lock().me = me.map(str::to_string);
    }

    /// Queues an answer for a request path. Answers for one path are served in
    /// the order queued, and the last one is served again and again. A path
    /// nothing was queued for gets a 404.
    pub fn serve(&self, path: &str, response: GhResponse) {
        self.lock()
            .answers
            .entry(path.to_string())
            .or_default()
            .push_back(response);
    }

    /// Every request so far: the path and the `If-None-Match` it carried.
    pub fn calls(&self) -> Vec<(String, Option<String>)> {
        self.lock().calls.clone()
    }

    /// Just the paths of `calls`.
    pub fn paths(&self) -> Vec<String> {
        self.lock().calls.iter().map(|(p, _)| p.clone()).collect()
    }

    /// How many times `login()` was asked.
    pub fn logins(&self) -> usize {
        self.lock().logins
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Gh for FakeGh {
    fn get(&self, path: &str, etag: Option<&str>) -> GhResponse {
        let mut inner = self.lock();
        inner
            .calls
            .push((path.to_string(), etag.map(str::to_string)));
        match inner.answers.get_mut(path) {
            Some(queue) if queue.len() > 1 => queue.pop_front().unwrap(),
            Some(queue) => queue.front().cloned().unwrap(),
            None => http(404, &[], "{\"message\":\"Not Found\"}"),
        }
    }

    fn login(&self) -> Option<String> {
        let mut inner = self.lock();
        inner.logins += 1;
        inner.me.clone()
    }
}

/// A 200 with an `ETag`.
pub fn ok(body: &str, etag: &str) -> GhResponse {
    ok_with(body, etag, &[])
}

/// A 200 with an `ETag` and more headers (names in lower case, as the parser
/// gives them).
pub fn ok_with(body: &str, etag: &str, headers: &[(&str, &str)]) -> GhResponse {
    let mut all = vec![("etag".to_string(), etag.to_string())];
    all.extend(headers.iter().map(|(n, v)| (n.to_string(), v.to_string())));
    GhResponse::Ok {
        status: 200,
        etag: Some(etag.to_string()),
        headers: all,
        body: body.to_string(),
    }
}

pub fn not_modified() -> GhResponse {
    GhResponse::NotModified { headers: vec![] }
}

pub fn http(status: u16, headers: &[(&str, &str)], body: &str) -> GhResponse {
    GhResponse::Http {
        status,
        headers: headers
            .iter()
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect(),
        body: body.to_string(),
    }
}

pub fn transport(message: &str) -> GhResponse {
    GhResponse::Transport(message.to_string())
}
```

4. In `src-tauri/crates/trigger-github/src/logic.rs`, make this edit.

Find this text, which appears once:

```rust
pub mod payload;
```

Replace it with:

```rust
pub mod payload;
pub mod poll;
```

5. In `src-tauri/crates/trigger-github/src/logic.rs`, make this edit.

Find this text, which appears once:

```rust
#[cfg(test)]
mod tests;
```

Replace it with:

```rust
#[cfg(test)]
pub(crate) mod fake_gh;
#[cfg(test)]
mod tests;
```

6. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib logic::poll` passes with 8 tests
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (the lib reports 143 tests on macOS and Linux)
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (a warning that `lib.rs` still has template code is expected until a later card)
- [ ] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- `tick`, asking `gh`, the backoff and the notes (the next cards).
- Tests for `fake_gh.rs`. The tick tests in the next cards use it.
- Changing `diff.rs` or `rule.rs`.
- Removing `#![allow(dead_code)]` from `logic.rs`. The fake and parts of the poller are unused until later cards.

## If stuck

- If a find text in steps 4 or 5 is not in `logic.rs`, slice B or an earlier card of this slice left it different. Set status blocked and say which edit.
- If a plan test fails, compare `poll.rs` and `plan_tests.rs` with steps 1 and 2 character by character. Do not change a test.
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
