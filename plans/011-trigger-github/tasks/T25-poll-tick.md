---
id: T25
epic: 011-trigger-github
title: Poll what is due with Poller::tick
status: done
depends_on: [T24]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-github/src/logic/poll.rs
  - src-tauri/crates/trigger-github/src/logic/poll/tick_tests.rs
---

## Goal

`Poller::tick(now, gh, cancelled)` exists and works when everything goes well: it reads the login once, plans the jobs, asks `gh` for whatever is due (pull requests first, serially, with the stored `ETag`), feeds each answer to `diff`, stores the new `ETag`, schedules the next request one interval later and returns what fired. Seven tests drive whole rounds with the fake `gh` and the slice B fixtures: a silent seed round, a round with 12 firings, a round of 304s, the interval, and the `since` rule. Answers that are not a success are handled by the next card; until then they only schedule an immediate retry.

## Read first

- `plans/011-trigger-github/epic.md` Decisions 4, 9 and 10 (one request per rule per interval at most, ETags per path, the first poll only seeds)
- `src-tauri/crates/trigger-github/src/logic/poll.rs` the file you replace (T24): `Job`, `plan`, `Poller`
- `src-tauri/crates/trigger-github/src/logic/diff.rs` `diff` and its doc comment (T19): the `key`, `since` and seeding rules
- `src-tauri/crates/trigger-github/src/logic/backoff.rs` `Backoff::record`, `may_poll`, `effective_interval_s` (T11)
- `src-tauri/crates/trigger-github/src/logic/fake_gh.rs` `FakeGh`, `ok`, `not_modified` (T24)

## Steps

1. Replace the whole of `src-tauri/crates/trigger-github/src/logic/poll.rs` with exactly this.

What is new, in order of use:

- `Note` and `Outcome`: what a tick returns. `fires` become events; `notes` are log lines (rule ids and plain words only). Notes are added by the next card.
- `tick` returns at once when stopped or when `Backoff` says to wait. It learns the login on first use (`learn_who_you_are`: a failure is retried after 60 seconds), then walks the jobs in order. Before each request it asks `cancelled()` and `Backoff::may_poll`, so a stop request or a rate limit that begins halfway through a round ends the round.
- `poll` asks `gh` with the `ETag` kept for that path, hands the answer to `Backoff::record` and, for `Verdict::Fine`, to `diff` with the job's rules, the signed in login and `since`. Only after `diff` succeeds is the new `ETag` kept. The job is due again one effective interval later (`X-Poll-Interval` raises it). A 304 changes nothing else.
- Any other verdict only sets `due = now` for now (the match arm `_`). The next card replaces that arm.

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

use trigger_core::RuleId;

use super::backoff::{Backoff, Verdict};
use super::cli::Gh;
use super::diff::{diff, feed_key, groups_for, path_for, Fire, Group};
use super::response::GhResponse;
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

/// A line for the log. Rule ids and plain words only: never a response body,
/// a header or anything the user has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub rules: Vec<RuleId>,
    pub text: String,
}

/// What one `tick` produced.
#[derive(Debug, Default)]
pub struct Outcome {
    pub fires: Vec<Fire>,
    pub notes: Vec<Note>,
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

    /// Does whatever is due at time `now` and returns what fired. Called about
    /// once a second. `cancelled` is asked before every request, so a stop
    /// request is noticed between requests.
    ///
    /// Requests run one after another in the order of `plan`. When GitHub says
    /// to wait, nothing is asked until the wait is over.
    pub fn tick(&mut self, now: u64, gh: &dyn Gh, cancelled: &dyn Fn() -> bool) -> Outcome {
        let mut out = Outcome::default();
        if self.stopped || !self.backoff.may_poll(now) {
            return out;
        }
        if self.me.is_none() && !self.learn_who_you_are(now, gh) {
            return out;
        }
        for index in 0..self.jobs.len() {
            if cancelled() || !self.backoff.may_poll(now) {
                break;
            }
            if self.jobs[index].due > now {
                continue;
            }
            self.poll(index, now, gh, &mut out);
        }
        out
    }

    /// Reads the login once and plans the jobs. When `gh` cannot say, tries
    /// again after `LOGIN_RETRY_S`.
    fn learn_who_you_are(&mut self, now: u64, gh: &dyn Gh) -> bool {
        if now < self.login_retry_at {
            return false;
        }
        match gh.login() {
            Some(me) => {
                self.jobs = plan(&self.rules, &me, &self.since);
                self.me = Some(me);
                true
            }
            None => {
                self.login_retry_at = now.saturating_add(LOGIN_RETRY_S);
                false
            }
        }
    }

    /// Makes one request, takes the answer into the backoff and the memory,
    /// and schedules the next one.
    fn poll(&mut self, index: usize, now: u64, gh: &dyn Gh, out: &mut Outcome) {
        let Some(me) = self.me.as_deref() else {
            return;
        };
        let job = &mut self.jobs[index];
        let etag = self
            .state
            .repo(&job.repo)
            .etag(&job.path)
            .map(str::to_string);
        let response = gh.get(&job.path, etag.as_deref());
        let verdict = self.backoff.record(now, &response);
        if let Some(hint) = response.poll_interval_s() {
            job.hint_s = Some(hint);
        }
        match verdict {
            Verdict::Fine => {
                job.not_found = false;
                job.reported = false;
                if let GhResponse::Ok { etag, body, .. } = &response {
                    let rules: Vec<&Armed> = job.rules.iter().map(|&r| &self.rules[r]).collect();
                    let repo = self.state.repo(&job.repo);
                    if let Ok(fires) =
                        diff(job.group, body, &job.key, repo, &rules, me, &self.since)
                    {
                        out.fires.extend(fires);
                        if let Some(etag) = etag {
                            repo.set_etag(&job.path, etag);
                        }
                    }
                }
                job.due = now.saturating_add(self.backoff.effective_interval_s(
                    now,
                    job.interval_s,
                    job.hint_s,
                    job.not_found,
                ));
            }
            _ => job.due = now,
        }
    }
}

#[cfg(test)]
mod plan_tests;
#[cfg(test)]
mod tick_tests;
```

2. Create `src-tauri/crates/trigger-github/src/logic/poll/tick_tests.rs` with exactly this.

The clock is a plain number: `START` is 2026-10-12T00:00:00Z, the day the fixtures were written around, so the comment fixtures are "after the start" and the 2026-10-09 review comment is "before it".

```rust
//! Tests for `Poller::tick` when everything goes well: what is asked, when,
//! with which `ETag`, and what fires. Failures are in `verdict_tests.rs`.

use serde_json::json;

use super::*;
use crate::logic::diff::kind;
use crate::logic::fake_gh::{not_modified, ok, FakeGh};
use crate::logic::testutil::{
    armed, only, ISSUES, ISSUE_COMMENTS, ME, PULLS, PULL_COMMENTS, RELEASES, RUNS,
};

const REPO: &str = "cli/cli";
/// 2026-10-12T00:00:00Z, the start time the fixtures were written around.
const START: u64 = 1_791_763_200;
const GROUPS: [Group; 6] = [
    Group::Pulls,
    Group::Runs,
    Group::Releases,
    Group::AssignedIssues,
    Group::IssueComments,
    Group::PullComments,
];

fn every_kind() -> Vec<Armed> {
    crate::KINDS
        .iter()
        .map(|k| armed(&format!("p/{}", k.id), k.id, json!({ "repo": REPO })))
        .collect()
}

/// The request path of a group, as `plan` builds it for this test.
fn path(group: Group) -> String {
    path_for(REPO, group, ME, Some(&iso(START)), None)
}

fn tick(poller: &mut Poller, now: u64, gh: &FakeGh) -> Outcome {
    poller.tick(now, gh, &|| false)
}

/// The fixtures, reduced to a few items for the first poll.
fn seed_bodies() -> [String; 6] {
    [
        only(PULLS, &[4829000004, 4829000005, 4829000006]),
        only(RUNS, &[38303985555, 38303985556]),
        only(RELEASES, &[399674740, 399674700]),
        only(ISSUES, &[5741818100]),
        only(ISSUE_COMMENTS, &[6131952697, 6131952628]),
        only(PULL_COMMENTS, &[4233990557]),
    ]
}

fn full_bodies() -> [&'static str; 6] {
    [PULLS, RUNS, RELEASES, ISSUES, ISSUE_COMMENTS, PULL_COMMENTS]
}

/// Queues, for every group: the seed answer, the full answer, then a 304.
fn serve_three_rounds(gh: &FakeGh) {
    for ((group, seed), full) in GROUPS.into_iter().zip(seed_bodies()).zip(full_bodies()) {
        gh.serve(&path(group), ok(&seed, "W/\"seed\""));
        gh.serve(&path(group), ok(full, "W/\"news\""));
        gh.serve(&path(group), not_modified());
    }
}

fn count(out: &Outcome, kind: &str) -> usize {
    out.fires.iter().filter(|f| f.kind == kind).count()
}

#[test]
fn the_first_round_seeds_the_second_fires_and_the_third_is_a_304() {
    let gh = FakeGh::new(Some(ME));
    serve_three_rounds(&gh);
    let mut poller = Poller::new(every_kind(), START);

    let first = tick(&mut poller, START, &gh);
    assert!(first.fires.is_empty(), "{:?}", first.fires);
    assert!(first.notes.is_empty());
    let expected: Vec<String> = GROUPS.into_iter().map(path).collect();
    assert_eq!(gh.paths(), expected, "pull requests first, then the rest");
    assert!(gh.calls().iter().all(|(_, etag)| etag.is_none()));

    let second = tick(&mut poller, START + 120, &gh);
    assert_eq!(count(&second, kind::REVIEW_REQUESTED), 1);
    assert_eq!(count(&second, kind::DEPENDENCY_PR_OPENED), 2);
    assert_eq!(count(&second, kind::PR_MERGED), 1);
    assert_eq!(count(&second, kind::CI_FAILED), 3);
    assert_eq!(count(&second, kind::CI_PASSED), 1);
    assert_eq!(count(&second, kind::RELEASE_PUBLISHED), 1);
    assert_eq!(count(&second, kind::ISSUE_ASSIGNED), 1);
    assert_eq!(count(&second, kind::PR_COMMENTED), 2);
    assert_eq!(second.fires.len(), 12);
    let calls = gh.calls();
    assert_eq!(calls.len(), 12);
    assert!(calls[6..]
        .iter()
        .all(|(_, etag)| etag.as_deref() == Some("W/\"seed\"")));

    let third = tick(&mut poller, START + 240, &gh);
    assert!(third.fires.is_empty());
    let calls = gh.calls();
    assert_eq!(calls.len(), 18);
    assert!(calls[12..]
        .iter()
        .all(|(_, etag)| etag.as_deref() == Some("W/\"news\"")));
}

#[test]
fn the_login_is_asked_once() {
    let gh = FakeGh::new(Some(ME));
    serve_three_rounds(&gh);
    let mut poller = Poller::new(every_kind(), START);
    for round in 0..3 {
        tick(&mut poller, START + round * 120, &gh);
    }
    assert_eq!(gh.logins(), 1);
}

#[test]
fn nothing_is_asked_before_the_interval_is_up() {
    let gh = FakeGh::new(Some(ME));
    serve_three_rounds(&gh);
    let mut poller = Poller::new(every_kind(), START);
    tick(&mut poller, START, &gh);
    assert_eq!(gh.calls().len(), 6);
    for second in [1, 60, 119] {
        let out = tick(&mut poller, START + second, &gh);
        assert!(out.fires.is_empty());
    }
    assert_eq!(gh.calls().len(), 6);
    tick(&mut poller, START + 120, &gh);
    assert_eq!(gh.calls().len(), 12);
}

#[test]
fn a_rule_with_a_longer_interval_is_asked_less_often() {
    let gh = FakeGh::new(Some(ME));
    let release = path_for(REPO, Group::Releases, ME, None, None);
    let runs = path_for(REPO, Group::Runs, ME, None, None);
    gh.serve(&release, ok(&only(RELEASES, &[399674740]), "W/\"r\""));
    gh.serve(&runs, ok(&only(RUNS, &[38303985555]), "W/\"c\""));
    let rules = vec![
        armed(
            "p/rel",
            kind::RELEASE_PUBLISHED,
            json!({ "repo": REPO, "interval_s": 60 }),
        ),
        armed(
            "p/ci",
            kind::CI_FAILED,
            json!({ "repo": REPO, "interval_s": 300 }),
        ),
    ];
    let mut poller = Poller::new(rules, START);
    for second in [0, 60, 120, 180, 240, 300] {
        tick(&mut poller, START + second, &gh);
    }
    let asked = |p: &str| gh.paths().iter().filter(|x| *x == p).count();
    assert_eq!(asked(&release), 6);
    assert_eq!(asked(&runs), 2);
}

#[test]
fn only_the_requests_a_rule_needs_are_made() {
    let gh = FakeGh::new(Some(ME));
    let rules = vec![armed(
        "p/rel",
        kind::RELEASE_PUBLISHED,
        json!({ "repo": REPO }),
    )];
    let mut poller = Poller::new(rules, START);
    tick(&mut poller, START, &gh);
    assert_eq!(gh.paths(), ["repos/cli/cli/releases?per_page=10"]);
}

#[test]
fn a_cancelled_tick_asks_nothing() {
    let gh = FakeGh::new(Some(ME));
    let mut poller = Poller::new(every_kind(), START);
    let out = poller.tick(START, &gh, &|| true);
    assert!(out.fires.is_empty());
    assert!(gh.calls().is_empty());
}

#[test]
fn a_comment_from_before_the_start_never_fires() {
    let gh = FakeGh::new(Some(ME));
    let rules = vec![armed("p/c", kind::PR_COMMENTED, json!({ "repo": REPO }))];
    // The seed has nothing, so everything in the later answers looks new. The
    // review comment on #14568 was written on 2026-10-09, before the start.
    gh.serve(&path(Group::Pulls), ok("[]", "W/\"p\""));
    gh.serve(&path(Group::IssueComments), ok("[]", "W/\"i\""));
    gh.serve(&path(Group::PullComments), ok("[]", "W/\"r\""));
    gh.serve(&path(Group::Pulls), ok(PULLS, "W/\"p2\""));
    gh.serve(&path(Group::IssueComments), ok(ISSUE_COMMENTS, "W/\"i2\""));
    gh.serve(&path(Group::PullComments), ok(PULL_COMMENTS, "W/\"r2\""));
    let mut poller = Poller::new(rules, START);
    tick(&mut poller, START, &gh);
    let out = tick(&mut poller, START + 120, &gh);
    let ids: Vec<&serde_json::Value> = out.fires.iter().map(|f| &f.payload["id"]).collect();
    assert!(!ids.contains(&&json!(4233990557u64)), "{ids:?}");
    assert!(!out.fires.is_empty());
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib logic::poll` passes with 15 tests (7 new in this card)
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (the lib reports 150 tests on macOS and Linux)
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (a warning that `lib.rs` still has template code is expected until a later card)
- [ ] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- Rate limits, 404, 401, 403, transport errors, unreadable bodies and notes. Next card.
- The thread, the clock and `start` (T28).
- Changing `plan`, `diff` or `Backoff`.
- Removing `#![allow(dead_code)]` from `logic.rs`.

## If stuck

- If `poll.rs` does not compile, compare it with step 1 character by character. Do not change `plan` or any other file to make it compile.
- If a tick test fails, compare `tick_tests.rs` with step 2. Do not change a test or a fixture. If it still fails, run `cargo test -p trigger-github --lib logic::tests` and `logic::diff`; if those fail too, slice B is broken: set status blocked and say so.
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
