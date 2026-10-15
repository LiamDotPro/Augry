//! One repository the source watches: the rules on it, the last snapshot, the
//! settle timer. `tick` is the only way in, and time and git both come from
//! outside (`now`, the `Repo`), so it is tested with a fake and without
//! threads.

use std::time::{Duration, Instant, SystemTime};

use trigger_core::Event;

use crate::logic::polled::{behind_payload, uncommitted_payload, BehindWatch, UncommittedWatch};
use crate::logic::repo::Repo;
use crate::logic::settle::Settle;
use crate::logic::{observe, Payload, RepoSnapshot};
use crate::rule::Parsed;

/// What a polled rule remembers between polls. The other kinds are decided
/// from snapshots and remember nothing here.
pub enum Watch {
    Snapshot,
    Behind(BehindWatch),
    Uncommitted(UncommittedWatch),
}

impl Watch {
    /// Forgets what was seen. Used while the branch glob does not match, so
    /// the first poll after the branch matches again is a fresh baseline.
    fn reset(&mut self) {
        match self {
            Watch::Snapshot => {}
            Watch::Behind(w) => *w = BehindWatch::new(),
            Watch::Uncommitted(w) => *w = UncommittedWatch::new(),
        }
    }
}

/// One rule on a repository.
pub struct Entry {
    pub id: String,
    pub kind: String,
    pub parsed: Parsed,
    pub watch: Watch,
    /// When a polled rule is next due. Unused by the snapshot kinds.
    pub next_poll: Instant,
}

impl Entry {
    /// `now` is when the source starts: a polled rule is due at once, so its
    /// first answer (a baseline for `behind-remote`) arrives straight away.
    pub fn new(id: String, kind: String, parsed: Parsed, now: Instant) -> Entry {
        let watch = match kind.as_str() {
            "git.behind-remote" => Watch::Behind(BehindWatch::new()),
            "git.uncommitted-older-than" => Watch::Uncommitted(UncommittedWatch::new()),
            _ => Watch::Snapshot,
        };
        Entry {
            id,
            kind,
            parsed,
            watch,
            next_poll: now,
        }
    }
}

pub(crate) fn event(rule: &str, kind: &str, payload: Payload) -> Event {
    Event {
        rule: rule.to_string(),
        kind: kind.to_string(),
        at: SystemTime::now(),
        payload,
    }
}

pub struct RepoSlot {
    /// The repository path as the rules gave it, used in payloads.
    path: String,
    repo: Box<dyn Repo>,
    /// True while git holds `index.lock` or `HEAD.lock`.
    locked: Box<dyn Fn() -> bool + Send>,
    entries: Vec<Entry>,
    prev: Option<RepoSnapshot>,
    settle: Settle,
}

impl RepoSlot {
    /// Takes the baseline snapshot now, so anything that happens after `new`
    /// returns is seen as a change. A failed snapshot leaves no baseline and
    /// the first good one becomes it.
    pub fn new(
        path: String,
        repo: Box<dyn Repo>,
        locked: Box<dyn Fn() -> bool + Send>,
        entries: Vec<Entry>,
    ) -> RepoSlot {
        let prev = repo.snapshot().ok();
        RepoSlot {
            path,
            repo,
            locked,
            entries,
            prev,
            settle: Settle::new(),
        }
    }

    /// A watched file changed.
    pub fn touch(&mut self, now: Instant) {
        self.settle.touch(now);
    }

    /// When `tick` next has something to do. None when nothing is waiting.
    pub fn deadline(&self) -> Option<Instant> {
        let polls = self
            .entries
            .iter()
            .filter(|e| !matches!(e.watch, Watch::Snapshot))
            .map(|e| e.next_poll);
        self.settle.deadline().into_iter().chain(polls).min()
    }

    /// Does whatever is due at `now` and returns the events it fires.
    pub fn tick(&mut self, now: Instant) -> Vec<Event> {
        let mut out = Vec::new();
        if self.settle.due(now, || (self.locked)()) {
            out.extend(self.settled());
        }
        out.extend(self.poll(now));
        out
    }

    /// Runs every polled rule that is due. The branch is the one in the last
    /// snapshot, so a branch switch seen a moment ago already counts. A rule
    /// whose glob does not match the branch is reset and not asked.
    fn poll(&mut self, now: Instant) -> Vec<Event> {
        let branch = self.prev.as_ref().and_then(|s| s.branch.clone());
        let mut out = Vec::new();
        for entry in &mut self.entries {
            if matches!(entry.watch, Watch::Snapshot) || entry.next_poll > now {
                continue;
            }
            entry.next_poll = now + Duration::from_secs(entry.parsed.when.interval_s);
            if !entry.parsed.branch_matches(branch.as_deref()) {
                entry.watch.reset();
                continue;
            }
            let payload = match &mut entry.watch {
                Watch::Snapshot => None,
                Watch::Behind(watch) => watch
                    .observe(self.repo.behind(entry.parsed.when.fetch))
                    .map(|n| behind_payload(&self.path, branch.as_deref(), n)),
                Watch::Uncommitted(watch) => watch
                    .observe(
                        self.repo.oldest_change_secs(),
                        entry.parsed.when.older_than_min,
                    )
                    .map(|m| uncommitted_payload(&self.path, branch.as_deref(), m)),
            };
            if let Some(payload) = payload {
                out.push(event(&entry.id, &entry.kind, payload));
            }
        }
        out
    }

    /// Takes a snapshot, diffs it with the last one, and turns each change
    /// into an event for every rule of that kind whose branch glob matches.
    fn settled(&mut self) -> Vec<Event> {
        let Ok(next) = self.repo.snapshot() else {
            return Vec::new();
        };
        let changes = observe(self.prev.as_ref(), &next);
        self.prev = Some(next);
        let mut out = Vec::new();
        for change in &changes {
            for entry in &self.entries {
                if entry.kind == change.kind() && entry.parsed.branch_matches(change.branch()) {
                    out.push(event(&entry.id, change.kind(), change.payload(&self.path)));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests;
