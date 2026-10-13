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
