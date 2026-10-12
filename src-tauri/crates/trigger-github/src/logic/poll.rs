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
