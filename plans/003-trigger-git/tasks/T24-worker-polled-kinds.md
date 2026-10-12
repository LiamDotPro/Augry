---
id: T24
epic: 003-trigger-git
title: Poll behind-remote and uncommitted-older-than in RepoSlot
status: todo
depends_on: [T23]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/worker/repo.rs
  - src-tauri/crates/trigger-git/src/worker/repo/tests.rs
---

## Goal

`RepoSlot` also runs the two polled kinds that use a repository: `git.behind-remote` and `git.uncommitted-older-than`. A rule is asked when its `interval_s` has passed, using `BehindWatch` and `UncommittedWatch`, with the branch from the latest snapshot. `deadline()` now includes the poll times.

## Read first

- `plans/003-trigger-git/epic.md` Decision 10
- `src-tauri/crates/trigger-git/src/worker/repo.rs` the file you edit
- `src-tauri/crates/trigger-git/src/logic/polled.rs` `BehindWatch::observe`, `UncommittedWatch::observe`, `behind_payload`, `uncommitted_payload`
- `src-tauri/crates/trigger-git/src/logic/fake.rs` the counters `fetch_calls` and `plain_behind_calls`
- `src-tauri/crates/trigger-git/src/worker/repo/tests.rs` the tests you append to

## Steps

1. In `src-tauri/crates/trigger-git/src/worker/repo.rs` make these 5 edits, each a find and replace of exact text. Each find text appears once.

**Edit 1.** Find:

```rust
use std::time::{Instant, SystemTime};
```

Replace with:

```rust
use std::time::{Duration, Instant, SystemTime};
```

**Edit 2.** Find:

```rust
use crate::logic::polled::{BehindWatch, UncommittedWatch};
```

Replace with:

```rust
use crate::logic::polled::{behind_payload, uncommitted_payload, BehindWatch, UncommittedWatch};
```

**Edit 3.** Find:

```rust
/// One rule on a repository.
pub struct Entry {
```

Replace with:

```rust
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
```

**Edit 4.** Find:

```rust
    pub fn deadline(&self) -> Option<Instant> {
        self.settle.deadline()
    }
```

Replace with:

```rust
    pub fn deadline(&self) -> Option<Instant> {
        let polls = self
            .entries
            .iter()
            .filter(|e| !matches!(e.watch, Watch::Snapshot))
            .map(|e| e.next_poll);
        self.settle.deadline().into_iter().chain(polls).min()
    }
```

**Edit 5.** Find:

```rust
            out.extend(self.settled());
        }
        out
    }
```

Replace with:

```rust
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
                    .observe(self.repo.oldest_change_secs(), entry.parsed.when.older_than_min)
                    .map(|m| uncommitted_payload(&self.path, branch.as_deref(), m)),
            };
            if let Some(payload) = payload {
                out.push(event(&entry.id, &entry.kind, payload));
            }
        }
        out
    }
```

2. Append this to the end of `src-tauri/crates/trigger-git/src/worker/repo/tests.rs` (it starts with a blank line).

```rust

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

#[test]
fn behind_remote_fires_when_the_count_leaves_zero() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut slot = slot(
        &fake,
        vec![entry("p/b", "git.behind-remote", json!({ "interval_s": 30 }))],
    );
    let t0 = Instant::now();
    fake.set_behind(Some(0));
    assert!(slot.tick(t0).is_empty());
    assert_eq!(fake.plain_behind_calls(), 1);

    // Not due again before the interval, so git is not asked.
    fake.set_behind(Some(2));
    assert!(slot.tick(t0 + secs(29)).is_empty());
    assert_eq!(fake.plain_behind_calls(), 1);

    let events = slot.tick(t0 + secs(30));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "git.behind-remote");
    assert_eq!(events[0].payload["repo"], json!("/work/app"));
    assert_eq!(events[0].payload["branch"], json!("main"));
    assert_eq!(events[0].payload["behind"], json!(2));
}

#[test]
fn fetch_true_asks_for_a_fetch_and_false_does_not() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut with = slot(
        &fake,
        vec![entry("p/b", "git.behind-remote", json!({ "fetch": true }))],
    );
    with.tick(Instant::now());
    assert_eq!((fake.fetch_calls(), fake.plain_behind_calls()), (1, 0));

    let other = FakeRepo::new(snap("main", "aaa"));
    let mut without = slot(&other, vec![entry("p/b", "git.behind-remote", json!({}))]);
    without.tick(Instant::now());
    assert_eq!((other.fetch_calls(), other.plain_behind_calls()), (0, 1));
}

#[test]
fn uncommitted_work_fires_once_when_old_enough_and_again_after_a_clean_tree() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut slot = slot(
        &fake,
        vec![entry(
            "p/u",
            "git.uncommitted-older-than",
            json!({ "interval_s": 30, "older_than_min": 60 }),
        )],
    );
    let t0 = Instant::now();
    fake.set_age(Some(30 * 60));
    assert!(slot.tick(t0).is_empty());

    fake.set_age(Some(2 * 60 * 60));
    let events = slot.tick(t0 + secs(30));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "git.uncommitted-older-than");
    assert_eq!(events[0].payload["minutes"], json!(120));
    assert_eq!(events[0].payload["branch"], json!("main"));

    assert!(slot.tick(t0 + secs(60)).is_empty());
    fake.set_age(None);
    assert!(slot.tick(t0 + secs(90)).is_empty());
    fake.set_age(Some(2 * 60 * 60));
    assert_eq!(slot.tick(t0 + secs(120)).len(), 1);
}

#[test]
fn a_polled_rule_whose_glob_does_not_match_is_not_asked() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut slot = slot(
        &fake,
        vec![entry(
            "p/b",
            "git.behind-remote",
            json!({ "branch": "release/*" }),
        )],
    );
    fake.set_behind(Some(5));
    assert!(slot.tick(Instant::now()).is_empty());
    assert_eq!(fake.plain_behind_calls() + fake.fetch_calls(), 0);
}

#[test]
fn a_poll_uses_the_branch_of_the_latest_snapshot() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut slot = slot(
        &fake,
        vec![entry(
            "p/b",
            "git.behind-remote",
            json!({ "branch": "release/*", "interval_s": 30 }),
        )],
    );
    let t0 = Instant::now();
    fake.set_snapshot(Ok(snap("release/1", "aaa")));
    slot.touch(t0);
    // The settled snapshot moves the slot to release/1; the poll that follows
    // in the same tick is the baseline.
    fake.set_behind(Some(0));
    assert!(slot.tick(t0 + SETTLE).is_empty());
    assert_eq!(fake.plain_behind_calls(), 1);

    fake.set_behind(Some(4));
    let events = slot.tick(t0 + SETTLE + secs(30));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].payload["branch"], json!("release/1"));
}

#[test]
fn the_deadline_includes_the_polled_rules() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut snapshot_only = slot(&fake, vec![entry("p/c", "git.commit", json!({}))]);
    assert_eq!(snapshot_only.deadline(), None);
    let _ = snapshot_only.tick(Instant::now());
    assert_eq!(snapshot_only.deadline(), None);

    let mut polled = slot(
        &fake,
        vec![entry("p/b", "git.behind-remote", json!({ "interval_s": 30 }))],
    );
    let t0 = Instant::now();
    assert!(polled.deadline().unwrap() <= t0);
    polled.tick(t0);
    assert_eq!(polled.deadline(), Some(t0 + secs(30)));
    polled.touch(t0);
    assert_eq!(polled.deadline(), Some(t0 + SETTLE));
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-git`.

Note on `check-crates.mjs`: it prints warnings, not errors. A warning about `src/logic/tests.rs` being long is expected and stays. Only errors fail the check.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git worker::repo` runs 15 tests and they pass
- [ ] `cd src-tauri && cargo test -p trigger-git` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- Changing `BehindWatch`, `UncommittedWatch` or the payload functions.
- Calling `snapshot()` from `poll`. The branch comes from the last settled snapshot, on purpose.
- Skipping the immediate first poll. A polled rule is due as soon as the slot exists (that first answer is the baseline).
- The folder scan (T25) and the thread (T26).
- README changes (T27).

## If stuck

- If an edit's find text is not in `repo.rs`, T23 was built differently from its card. Set status blocked and say which edit.
- If a test in `worker::repo` fails, do not change the expected values: set status blocked and quote the failure.
- If clippy reports `dead_code`, the `#![allow(dead_code)]` at the top of `worker.rs` was lost. Put it back.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
