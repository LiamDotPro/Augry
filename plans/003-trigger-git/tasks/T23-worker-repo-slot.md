---
id: T23
epic: 003-trigger-git
title: Add RepoSlot: snapshot, diff and rule filter
status: done
depends_on: [T22, T21]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/lib.rs
  - src-tauri/crates/trigger-git/src/worker.rs
  - src-tauri/crates/trigger-git/src/worker/repo.rs
  - src-tauri/crates/trigger-git/src/worker/repo/tests.rs
---

## Goal

`worker/repo.rs` has `RepoSlot`: one repository with its rules, its last snapshot and a settle timer. `tick(now)` takes a snapshot once the settle time has passed, diffs it with the last one, and returns an `Event` for every rule of the kind that happened whose branch glob matches. It is tested with `FakeRepo` and no threads. The polled kinds come in T24.

## Read first

- `plans/003-trigger-git/epic.md` Decision 9 and the Shape section
- `src-tauri/crates/trigger-git/src/logic.rs` `observe`, `Change::kind`, `Change::branch`, `Change::payload`
- `src-tauri/crates/trigger-git/src/logic/settle.rs` `Settle`
- `src-tauri/crates/trigger-git/src/logic/fake.rs` `FakeRepo` (clones share state)
- `src-tauri/crates/trigger-git/src/rule.rs:40-60` `Parsed` and `branch_matches`

## Steps

1. Check that `src-tauri/crates/trigger-git/src/worker.rs` and the folder `src/worker/` do not exist.

2. In `src-tauri/crates/trigger-git/src/lib.rs`, add the line `mod worker;` directly below `mod rule;`. The block now reads `mod git;`, `mod logic;`, `mod os;`, `mod rule;`, `mod worker;`.

3. Create `src-tauri/crates/trigger-git/src/worker.rs` with exactly this content. `worker` is its own module because `lib.rs` would pass the 400 line limit with the thread in it. The two `allow` lines are removed by T27.

```rust
//! The thread that joins the OS layer to the pure logic. It owns every
//! repository slot, waits for file events and for the polled kinds' times,
//! and sends events to the sink. Glue only: the decisions are in `logic`.

// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]

mod repo;

// The allow is removed when lib.rs uses these.
#[allow(unused_imports)]
pub use repo::{Entry, RepoSlot};
```

4. Create the folder `src-tauri/crates/trigger-git/src/worker/` and in it `repo.rs` with exactly this content. `Entry` and `Watch` already hold what the polled kinds need (T24 uses it); `tick` only handles the snapshot kinds so far.

```rust
//! One repository the source watches: the rules on it, the last snapshot, the
//! settle timer. `tick` is the only way in, and time and git both come from
//! outside (`now`, the `Repo`), so it is tested with a fake and without
//! threads.

use std::time::{Instant, SystemTime};

use trigger_core::Event;

use crate::logic::polled::{BehindWatch, UncommittedWatch};
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
        self.settle.deadline()
    }

    /// Does whatever is due at `now` and returns the events it fires.
    pub fn tick(&mut self, now: Instant) -> Vec<Event> {
        let mut out = Vec::new();
        if self.settle.due(now, || (self.locked)()) {
            out.extend(self.settled());
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
```

5. Create the folder `src-tauri/crates/trigger-git/src/worker/repo/` and in it `tests.rs` with exactly this content.

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use trigger_core::testkit::rule;

use super::*;
use crate::logic::fake::FakeRepo;
use crate::logic::settle::SETTLE;

fn dir() -> String {
    std::env::temp_dir().to_string_lossy().into_owned()
}

/// A snapshot of `branch` whose HEAD is `head` (also used as the short sha),
/// reached by a commit.
fn snap(branch: &str, head: &str) -> RepoSnapshot {
    RepoSnapshot {
        head: Some(head.into()),
        branch: Some(branch.into()),
        short: Some(head.into()),
        message: Some("a message".into()),
        reflog_subject: Some("commit: a message".into()),
        ..RepoSnapshot::default()
    }
}

fn entry(id: &str, kind: &str, extra: serde_json::Value) -> Entry {
    let mut when = json!({ "repo": dir() });
    when.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let parsed = crate::rule::parse(&rule(id, kind, when)).unwrap();
    Entry::new(id.into(), kind.into(), parsed, Instant::now())
}

fn slot(fake: &FakeRepo, entries: Vec<Entry>) -> RepoSlot {
    RepoSlot::new(
        "/work/app".into(),
        Box::new(fake.clone()),
        Box::new(|| false),
        entries,
    )
}

/// Touches the slot at `t0` and ticks it once the settle time has passed.
fn settle_and_tick(slot: &mut RepoSlot, t0: Instant) -> Vec<Event> {
    slot.touch(t0);
    slot.tick(t0 + SETTLE)
}

#[test]
fn the_baseline_snapshot_fires_nothing_and_idle_ticks_do_nothing() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut slot = slot(&fake, vec![entry("p/c", "git.commit", json!({}))]);
    assert_eq!(fake.snapshot_calls(), 1);
    assert_eq!(slot.deadline(), None);
    assert!(slot
        .tick(Instant::now() + Duration::from_secs(60))
        .is_empty());
    assert_eq!(fake.snapshot_calls(), 1);
}

#[test]
fn a_commit_fires_after_the_settle_time_with_its_payload() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut slot = slot(&fake, vec![entry("p/c", "git.commit", json!({}))]);
    fake.set_snapshot(Ok(snap("main", "bbb")));
    let t0 = Instant::now();
    slot.touch(t0);
    assert_eq!(slot.deadline(), Some(t0 + SETTLE));
    assert!(slot.tick(t0 + Duration::from_millis(100)).is_empty());
    assert_eq!(fake.snapshot_calls(), 1);

    let events = slot.tick(t0 + SETTLE);
    assert_eq!(events.len(), 1);
    let e = &events[0];
    assert_eq!((e.rule.as_str(), e.kind.as_str()), ("p/c", "git.commit"));
    assert_eq!(e.payload["repo"], json!("/work/app"));
    assert_eq!(e.payload["branch"], json!("main"));
    assert_eq!(e.payload["commit"], json!("bbb"));
    assert_eq!(e.payload["message"], json!("a message"));
    assert_eq!(slot.deadline(), None);
}

#[test]
fn only_rules_of_the_kind_that_happened_fire() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut slot = slot(
        &fake,
        vec![
            entry("p/c", "git.commit", json!({})),
            entry("p/s", "git.branch-switched", json!({})),
            entry("p/d", "git.commit", json!({})),
        ],
    );
    fake.set_snapshot(Ok(snap("main", "bbb")));
    let events = settle_and_tick(&mut slot, Instant::now());
    let ids: Vec<&str> = events.iter().map(|e| e.rule.as_str()).collect();
    assert_eq!(ids, ["p/c", "p/d"]);
}

#[test]
fn the_branch_glob_filters_each_rule() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut slot = slot(
        &fake,
        vec![
            entry("p/rel", "git.commit", json!({ "branch": "release/*" })),
            entry("p/main", "git.commit", json!({ "branch": "main" })),
        ],
    );
    fake.set_snapshot(Ok(snap("main", "bbb")));
    let events = settle_and_tick(&mut slot, Instant::now());
    let ids: Vec<&str> = events.iter().map(|e| e.rule.as_str()).collect();
    assert_eq!(ids, ["p/main"]);
}

#[test]
fn a_switch_is_matched_on_the_new_branch_and_carries_from_and_to() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut slot = slot(
        &fake,
        vec![
            entry("p/old", "git.branch-switched", json!({ "branch": "main" })),
            entry(
                "p/new",
                "git.branch-switched",
                json!({ "branch": "feature/*" }),
            ),
        ],
    );
    fake.set_snapshot(Ok(snap("feature/x", "aaa")));
    let events = settle_and_tick(&mut slot, Instant::now());
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].rule, "p/new");
    assert_eq!(events[0].payload["from"], json!("main"));
    assert_eq!(events[0].payload["to"], json!("feature/x"));
}

#[test]
fn a_snapshot_that_changes_nothing_fires_nothing() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut slot = slot(&fake, vec![entry("p/c", "git.commit", json!({}))]);
    assert!(settle_and_tick(&mut slot, Instant::now()).is_empty());
    assert_eq!(fake.snapshot_calls(), 2);
}

#[test]
fn a_held_lock_delays_the_snapshot() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let locked = Arc::new(AtomicBool::new(true));
    let flag = locked.clone();
    let mut slot = RepoSlot::new(
        "/work/app".into(),
        Box::new(fake.clone()),
        Box::new(move || flag.load(Ordering::Relaxed)),
        vec![entry("p/c", "git.commit", json!({}))],
    );
    fake.set_snapshot(Ok(snap("main", "bbb")));
    let t0 = Instant::now();
    slot.touch(t0);
    assert!(slot.tick(t0 + SETTLE).is_empty());
    assert_eq!(fake.snapshot_calls(), 1);
    assert_eq!(slot.deadline(), Some(t0 + SETTLE * 2));

    locked.store(false, Ordering::Relaxed);
    let events = slot.tick(t0 + SETTLE * 2);
    assert_eq!(events.len(), 1);
    assert_eq!(fake.snapshot_calls(), 2);
}

#[test]
fn a_failed_snapshot_keeps_the_old_baseline() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut slot = slot(&fake, vec![entry("p/c", "git.commit", json!({}))]);
    fake.set_snapshot(Err("git timed out".into()));
    let t0 = Instant::now();
    assert!(settle_and_tick(&mut slot, t0).is_empty());

    fake.set_snapshot(Ok(snap("main", "bbb")));
    let events = settle_and_tick(&mut slot, t0 + Duration::from_secs(1));
    assert_eq!(events.len(), 1);
}

#[test]
fn with_no_baseline_the_first_good_snapshot_becomes_it() {
    let fake = FakeRepo::default();
    fake.set_snapshot(Err("not ready".into()));
    let mut slot = slot(&fake, vec![entry("p/c", "git.commit", json!({}))]);
    fake.set_snapshot(Ok(snap("main", "aaa")));
    let t0 = Instant::now();
    assert!(settle_and_tick(&mut slot, t0).is_empty());

    fake.set_snapshot(Ok(snap("main", "bbb")));
    let events = settle_and_tick(&mut slot, t0 + Duration::from_secs(1));
    assert_eq!(events.len(), 1);
}
```

6. Run `cd src-tauri && cargo fmt -p trigger-git`.

Note on `check-crates.mjs`: it prints warnings, not errors. A warning about `src/logic/tests.rs` being long is expected and stays. Only errors fail the check.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git worker::repo` runs 9 tests and they pass
- [ ] `cd src-tauri && cargo test -p trigger-git` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- Polling `behind-remote` or `uncommitted-older-than` (T24), the folder scan (T25) and the thread (T26).
- Calling `RepoSlot` from `lib.rs` (T27).
- Changing `observe`, `step` or `Change`.
- Adding a `Sync` bound or a lock to `RepoSlot`. It lives on one thread.
- Logging. Nothing from a repository goes into a log line.
- README changes (T27).

## If stuck

- If `lib.rs` already has `mod worker;`, or `src/worker.rs` exists, set status blocked and say so.
- If the compiler cannot find `crate::logic::settle::Settle`, T22 is not done. Set status blocked.
- If it cannot find `observe`, `Change::branch` or `Change::kind` in `crate::logic`, slice A was built differently from its cards. Set status blocked and quote the error.
- If clippy reports `unused_imports` for `pub use repo::{Entry, RepoSlot}`, the `#[allow(unused_imports)]` line in `worker.rs` was lost. Put it back.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
