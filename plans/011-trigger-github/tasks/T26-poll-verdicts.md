---
id: T26
epic: 011-trigger-github
title: Make tick obey rate limits, 404, 401 and errors
status: done
depends_on: [T25]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-github/src/logic/poll.rs
  - src-tauri/crates/trigger-github/src/logic/poll/verdict_tests.rs
---

## Goal

`Poller::tick` handles every `Verdict` the way Decision 10 and R02 say: a rate limit stops all requests until the wait is over, transport errors and 5xx back off quietly on the 60, 120, 240 second ladder, a 404 is reported once and the repository is polled at 5x the interval, a plain 403 is reported once, a 401 (or `gh` having no login) stops the poller for good, `X-Poll-Interval` raises the interval, few requests left stretch it 4x, and a body that cannot be read is reported once without keeping its `ETag`. Thirteen tests prove each rule with the fake `gh` and a clock that is a number.

## Read first

- `plans/011-trigger-github/epic.md` Decision 10 (the rules in one paragraph)
- `plans/011-trigger-github/research/R02-rate-limits-and-gh.md` "Implications for the cards", Rate limits and backoff (the source of every number in the tests)
- `src-tauri/crates/trigger-github/src/logic/backoff.rs` `Verdict` and its doc comments (T11): what each verdict means and which already include a wait
- `src-tauri/crates/trigger-github/src/logic/poll.rs` the file you replace (T25), the `match verdict` at the end of `poll`
- `src-tauri/crates/trigger-github/src/logic/fake_gh.rs` `http`, `ok_with`, `transport` (T24)

## Steps

1. Replace the whole of `src-tauri/crates/trigger-github/src/logic/poll.rs` with exactly this.

What changed from T25: the `match verdict` now has an arm for each verdict, `tick` stops its round when the poller stopped, and four helpers sit below `impl Poller` (`next_due`, `rule_ids`, `report`).

- `Fine`: as before. A body `diff` cannot read is reported once ("GitHub sent an answer this crate cannot read", never the body) and its `ETag` is not kept, so the next request asks from scratch.
- `RateLimited`: a note with the wait, and `due = now`. `Backoff` already holds the wait for the whole crate, so no job is asked until it ends, and then the limited job goes first.
- `Transport`: quiet. `due = now`; the ladder in `Backoff` decides when.
- `NotFound`: `not_found = true` makes the interval 5x; one note until the repository answers again.
- `Denied(status)`: one note with the status number only; normal interval.
- `Unauthorized`: `stopped = true` and one note naming every rule with the command to run. A stopped poller does nothing more.
- Notes never contain a response body, a header or a token.

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
            if self.stopped {
                break;
            }
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
                match &response {
                    GhResponse::Ok { etag, body, .. } => {
                        let rules: Vec<&Armed> =
                            job.rules.iter().map(|&r| &self.rules[r]).collect();
                        let repo = self.state.repo(&job.repo);
                        match diff(job.group, body, &job.key, repo, &rules, me, &self.since) {
                            Ok(fires) => {
                                job.reported = false;
                                out.fires.extend(fires);
                                if let Some(etag) = etag {
                                    repo.set_etag(&job.path, etag);
                                }
                            }
                            // The etag is not kept, so the next poll asks again
                            // from scratch instead of getting a 304.
                            Err(_) => report(
                                job,
                                &self.rules,
                                out,
                                "GitHub sent an answer this crate cannot read",
                            ),
                        }
                    }
                    _ => job.reported = false,
                }
                job.due = next_due(&self.backoff, now, job);
            }
            Verdict::RateLimited { wait_s } => {
                let text = format!("GitHub rate limit reached, waiting {wait_s} seconds");
                out.notes.push(Note {
                    rules: rule_ids(&self.rules, &job.rules),
                    text,
                });
                job.due = now;
            }
            // Quiet: the ladder in `Backoff` decides when to try again.
            Verdict::Transport { .. } => job.due = now,
            Verdict::NotFound => {
                job.not_found = true;
                report(
                    job,
                    &self.rules,
                    out,
                    "GitHub answered 404: the repository is missing or this login cannot see it",
                );
                job.due = next_due(&self.backoff, now, job);
            }
            Verdict::Denied(status) => {
                let text =
                    format!("GitHub answered {status}: this login may not read the repository");
                report(job, &self.rules, out, &text);
                job.due = next_due(&self.backoff, now, job);
            }
            Verdict::Unauthorized => {
                self.stopped = true;
                let everyone: Vec<usize> = (0..self.rules.len()).collect();
                out.notes.push(Note {
                    rules: rule_ids(&self.rules, &everyone),
                    text: "gh is not signed in any more, polling stopped. Run gh auth login in a terminal"
                        .to_string(),
                });
            }
        }
    }
}

/// When a job is next due after an answer that was not a wait: its interval,
/// stretched by whatever `Backoff` says about the rate limit and 404s.
fn next_due(backoff: &Backoff, now: u64, job: &Job) -> u64 {
    now.saturating_add(backoff.effective_interval_s(now, job.interval_s, job.hint_s, job.not_found))
}

fn rule_ids(rules: &[Armed], which: &[usize]) -> Vec<RuleId> {
    which.iter().map(|&i| rules[i].id.clone()).collect()
}

/// Adds a note about a problem with one job, unless the last answer for it
/// already had one.
fn report(job: &mut Job, rules: &[Armed], out: &mut Outcome, text: &str) {
    if job.reported {
        return;
    }
    job.reported = true;
    out.notes.push(Note {
        rules: rule_ids(rules, &job.rules),
        text: text.to_string(),
    });
}

#[cfg(test)]
mod plan_tests;
#[cfg(test)]
mod tick_tests;
#[cfg(test)]
mod verdict_tests;
```

2. Create `src-tauri/crates/trigger-github/src/logic/poll/verdict_tests.rs` with exactly this.

```rust
//! Tests for `Poller::tick` when GitHub pushes back: rate limits, errors, a
//! lost login, a missing repository, and answers that make no sense.

use serde_json::json;

use super::*;
use crate::logic::diff::kind;
use crate::logic::fake_gh::{http, not_modified, ok, ok_with, transport, FakeGh};
use crate::logic::response::NOT_LOGGED_IN;
use crate::logic::testutil::{armed, only, ME, PULLS, RELEASES};

const REPO: &str = "cli/cli";
/// 2026-10-12T00:00:00Z.
const START: u64 = 1_791_763_200;

fn releases() -> String {
    path_for(REPO, Group::Releases, ME, None, None)
}

fn pulls() -> String {
    path_for(REPO, Group::Pulls, ME, None, None)
}

/// One release rule: one request, `releases()`.
fn release_poller() -> Poller {
    let rule = armed(
        "p/rel",
        kind::RELEASE_PUBLISHED,
        json!({ "repo": REPO, "interval_s": 60 }),
    );
    Poller::new(vec![rule], START)
}

fn seed_body() -> String {
    only(RELEASES, &[399674740, 399674700])
}

fn tick(poller: &mut Poller, now: u64, gh: &FakeGh) -> Outcome {
    poller.tick(now, gh, &|| false)
}

fn calls_at(gh: &FakeGh) -> usize {
    gh.calls().len()
}

#[test]
fn a_rate_limit_stops_every_request_until_the_wait_is_over() {
    let gh = FakeGh::new(Some(ME));
    // Every group needs an answer; only the pull request list is limited.
    let rules: Vec<Armed> = crate::KINDS
        .iter()
        .map(|k| armed(&format!("p/{}", k.id), k.id, json!({ "repo": REPO })))
        .collect();
    let paths: Vec<String> = plan(&rules, ME, &iso(START))
        .iter()
        .map(|j| j.path.clone())
        .collect();
    for path in &paths {
        gh.serve(path, ok("[]", "W/\"seed\""));
    }
    gh.serve(&paths[1], ok("{\"workflow_runs\":[]}", "W/\"runs\""));
    gh.serve(&paths[0], http(403, &[("retry-after", "90")], "{}"));
    gh.serve(&paths[0], ok("[]", "W/\"after\""));
    let mut poller = Poller::new(rules, START);

    tick(&mut poller, START, &gh);
    assert_eq!(calls_at(&gh), 6);

    let limited = tick(&mut poller, START + 120, &gh);
    assert_eq!(calls_at(&gh), 7, "the rest of the round waits");
    assert_eq!(limited.notes.len(), 1);
    assert!(limited.notes[0].text.contains("90 seconds"));
    assert_eq!(limited.notes[0].rules.len(), 4, "the rules on that request");

    for second in [121, 150, 209] {
        tick(&mut poller, START + second, &gh);
    }
    assert_eq!(calls_at(&gh), 7, "nothing is asked during the wait");

    tick(&mut poller, START + 210, &gh);
    assert_eq!(calls_at(&gh), 13, "all six requests were due and are made");
    let calls = gh.calls();
    assert_eq!(calls[7].0, paths[0], "pull requests are first again");
    assert_eq!(calls[7].1.as_deref(), Some("W/\"seed\""));
}

#[test]
fn transport_errors_back_off_60_120_240_seconds_and_say_nothing() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), ok(&seed_body(), "W/\"seed\""));
    gh.serve(
        &releases(),
        transport("gh exited with code 1 and gave no HTTP status"),
    );
    let mut poller = release_poller();
    tick(&mut poller, START, &gh);
    assert_eq!(calls_at(&gh), 1);

    let mut asked = Vec::new();
    for second in 60..=480 {
        let before = calls_at(&gh);
        let out = tick(&mut poller, START + second, &gh);
        assert!(out.notes.is_empty() && out.fires.is_empty());
        if calls_at(&gh) > before {
            asked.push(second);
        }
    }
    // Asked at 60 (wait 60), 120 (wait 120), 240 (wait 240), then 480.
    assert_eq!(asked, [60, 120, 240, 480]);
}

#[test]
fn a_good_answer_ends_the_backoff() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), ok(&seed_body(), "W/\"seed\""));
    gh.serve(&releases(), transport("offline"));
    gh.serve(&releases(), not_modified());
    let mut poller = release_poller();
    tick(&mut poller, START, &gh);
    tick(&mut poller, START + 60, &gh);
    assert_eq!(calls_at(&gh), 2);
    tick(&mut poller, START + 119, &gh);
    assert_eq!(calls_at(&gh), 2, "waiting 60 seconds");
    tick(&mut poller, START + 120, &gh);
    assert_eq!(calls_at(&gh), 3);
    // The 304 reset the ladder, so the next request is one interval later.
    tick(&mut poller, START + 179, &gh);
    assert_eq!(calls_at(&gh), 3);
    tick(&mut poller, START + 180, &gh);
    assert_eq!(calls_at(&gh), 4);
}

#[test]
fn a_401_stops_the_poller_for_good_and_names_every_rule() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), ok(&seed_body(), "W/\"seed\""));
    gh.serve(
        &releases(),
        http(401, &[], "{\"message\":\"Bad credentials\"}"),
    );
    let mut poller = release_poller();
    tick(&mut poller, START, &gh);
    assert!(!poller.is_stopped());
    let out = tick(&mut poller, START + 60, &gh);
    assert!(poller.is_stopped());
    assert_eq!(out.notes.len(), 1);
    assert_eq!(out.notes[0].rules, ["p/rel"]);
    assert!(out.notes[0].text.contains("gh auth login"));
    assert!(!out.notes[0].text.contains("Bad credentials"));
    let before = calls_at(&gh);
    for second in [61, 200, 100_000] {
        let again = tick(&mut poller, START + second, &gh);
        assert!(again.fires.is_empty() && again.notes.is_empty());
    }
    assert_eq!(calls_at(&gh), before);
}

#[test]
fn gh_reporting_no_login_stops_the_poller_too() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), transport(NOT_LOGGED_IN));
    let mut poller = release_poller();
    tick(&mut poller, START, &gh);
    assert!(poller.is_stopped());
}

#[test]
fn a_missing_repository_is_reported_once_and_asked_five_times_less_often() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), ok(&seed_body(), "W/\"seed\""));
    gh.serve(&releases(), http(404, &[], "{\"message\":\"Not Found\"}"));
    gh.serve(&releases(), ok(&seed_body(), "W/\"back\""));
    let mut poller = release_poller();
    tick(&mut poller, START, &gh);

    let first = tick(&mut poller, START + 60, &gh);
    assert_eq!(first.notes.len(), 1);
    assert_eq!(first.notes[0].rules, ["p/rel"]);
    assert!(first.notes[0].text.contains("404"));
    assert!(!first.notes[0].text.contains("Not Found"));
    assert_eq!(calls_at(&gh), 2);

    // 60 seconds times 5 is 300.
    tick(&mut poller, START + 359, &gh);
    assert_eq!(calls_at(&gh), 2);
    tick(&mut poller, START + 360, &gh);
    assert_eq!(calls_at(&gh), 3);

    // The repository is back: normal interval again.
    tick(&mut poller, START + 419, &gh);
    assert_eq!(calls_at(&gh), 3);
    tick(&mut poller, START + 420, &gh);
    assert_eq!(calls_at(&gh), 4);
}

#[test]
fn a_404_that_repeats_is_not_reported_again() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), http(404, &[], "{}"));
    let mut poller = release_poller();
    let mut notes = 0;
    for second in [0, 300, 600, 900] {
        notes += tick(&mut poller, START + second, &gh).notes.len();
    }
    assert_eq!(calls_at(&gh), 4);
    assert_eq!(notes, 1);
}

#[test]
fn a_plain_403_is_reported_once_and_does_not_slow_the_polling() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), ok(&seed_body(), "W/\"seed\""));
    gh.serve(
        &releases(),
        http(403, &[], "{\"message\":\"Resource protected by SSO\"}"),
    );
    let mut poller = release_poller();
    tick(&mut poller, START, &gh);
    let first = tick(&mut poller, START + 60, &gh);
    assert_eq!(first.notes.len(), 1);
    assert!(first.notes[0].text.contains("403"));
    assert!(!first.notes[0].text.contains("SSO"));
    let second = tick(&mut poller, START + 120, &gh);
    assert!(second.notes.is_empty());
    assert_eq!(calls_at(&gh), 3);
}

#[test]
fn x_poll_interval_raises_the_interval() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(
        &releases(),
        ok_with(&seed_body(), "W/\"seed\"", &[("x-poll-interval", "300")]),
    );
    let mut poller = release_poller();
    tick(&mut poller, START, &gh);
    tick(&mut poller, START + 299, &gh);
    assert_eq!(calls_at(&gh), 1);
    tick(&mut poller, START + 300, &gh);
    assert_eq!(calls_at(&gh), 2);
}

#[test]
fn few_requests_left_stretch_the_interval_four_times() {
    let gh = FakeGh::new(Some(ME));
    let reset = (START + 3600).to_string();
    gh.serve(
        &releases(),
        ok_with(
            &seed_body(),
            "W/\"seed\"",
            &[
                ("x-ratelimit-remaining", "50"),
                ("x-ratelimit-reset", &reset),
            ],
        ),
    );
    let mut poller = release_poller();
    tick(&mut poller, START, &gh);
    tick(&mut poller, START + 239, &gh);
    assert_eq!(calls_at(&gh), 1);
    tick(&mut poller, START + 240, &gh);
    assert_eq!(calls_at(&gh), 2);
}

#[test]
fn an_unreadable_answer_is_reported_once_without_its_text_and_no_etag_is_kept() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), ok("{\"message\":\"oops\"}", "W/\"bad\""));
    gh.serve(&releases(), ok("{\"message\":\"oops\"}", "W/\"bad\""));
    gh.serve(&releases(), ok(&seed_body(), "W/\"seed\""));
    gh.serve(&releases(), ok(RELEASES, "W/\"news\""));
    let mut poller = release_poller();
    let first = tick(&mut poller, START, &gh);
    assert_eq!(first.notes.len(), 1);
    assert!(!first.notes[0].text.contains("oops"));
    let second = tick(&mut poller, START + 60, &gh);
    assert!(second.notes.is_empty(), "reported once");
    let third = tick(&mut poller, START + 120, &gh);
    assert!(third.fires.is_empty(), "this answer only seeds");
    let fourth = tick(&mut poller, START + 180, &gh);
    assert_eq!(fourth.fires.len(), 1);
    let sent: Vec<Option<String>> = gh.calls().into_iter().map(|(_, etag)| etag).collect();
    assert_eq!(
        sent[..3],
        [None, None, None],
        "a bad answer's etag is not kept"
    );
    assert_eq!(sent[3].as_deref(), Some("W/\"seed\""));
}

#[test]
fn a_failed_login_is_retried_every_minute_and_nothing_else_is_asked() {
    let gh = FakeGh::new(None);
    let mut poller = release_poller();
    tick(&mut poller, START, &gh);
    assert_eq!(gh.logins(), 1);
    tick(&mut poller, START + 59, &gh);
    assert_eq!(gh.logins(), 1);
    tick(&mut poller, START + 60, &gh);
    assert_eq!(gh.logins(), 2);
    assert!(gh.calls().is_empty());
    assert!(poller.jobs().is_empty());

    gh.set_me(Some(ME));
    gh.serve(&releases(), ok(&seed_body(), "W/\"seed\""));
    tick(&mut poller, START + 120, &gh);
    assert_eq!(gh.logins(), 3);
    assert_eq!(calls_at(&gh), 1);
    assert_eq!(poller.jobs().len(), 1);
}

#[test]
fn a_pull_request_rule_fires_through_the_whole_path() {
    // The pull request fixtures through `tick`: a seed, then news.
    let gh = FakeGh::new(Some(ME));
    gh.serve(&pulls(), ok(&only(PULLS, &[4829000004]), "W/\"seed\""));
    gh.serve(&pulls(), ok(PULLS, "W/\"news\""));
    let rule = armed(
        "p/review",
        kind::REVIEW_REQUESTED,
        json!({ "repo": REPO, "interval_s": 60 }),
    );
    let mut poller = Poller::new(vec![rule], START);
    tick(&mut poller, START, &gh);
    let out = tick(&mut poller, START + 60, &gh);
    assert_eq!(out.fires.len(), 1);
    assert_eq!(out.fires[0].rule, "p/review");
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [x] `cd src-tauri && cargo test -p trigger-github --lib logic::poll` passes with 28 tests (13 new in this card)
- [x] `cd src-tauri && cargo test -p trigger-github` passes (the lib reports 163 tests on macOS and Linux)
- [x] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [x] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [x] `node plans/_tools/check-crates.mjs` exits 0 (a warning that `lib.rs` still has template code is expected until a later card)
- [x] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- Changing a number in the rules (60, 900, 100, 4, 5). `Backoff` owns them and R02 decided them.
- Printing the notes. The thread does that (T28); `tick` only returns them.
- Telling the app that the source became unavailable. There is no channel for it in `trigger-core`; the poller stops and says why in a note.
- Changing `Backoff`, `diff` or `plan`.
- Removing `#![allow(dead_code)]` from `logic.rs`.

## If stuck

- If `poll.rs` does not compile, compare it with step 1 character by character. Do not change another file to make it compile.
- If a verdict test fails, compare `verdict_tests.rs` with step 2. Do not change a test. If it still fails, run `cargo test -p trigger-github --lib logic::backoff`; if that fails, slice B is broken: set status blocked and say so.
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
