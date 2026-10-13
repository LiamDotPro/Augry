---
id: T17
epic: 003-trigger-git
title: Add the behind-remote transition logic
status: done
depends_on: [T16]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-git/src/logic.rs
  - src-tauri/crates/trigger-git/src/logic/polled.rs
  - src-tauri/crates/trigger-git/src/logic/polled_tests.rs
---

## Goal

`logic/polled.rs` exists with `BehindWatch`, the pure rule for `git.behind-remote` (fires when the behind count goes from 0 to above 0), and `behind_payload`, with unit tests that need no git.

## Read first

- `plans/003-trigger-git/epic.md` Decision 10 (`behind-remote`) and Decision 12
- `src-tauri/crates/trigger-git/src/logic/repo.rs` `Repo::behind`, the source of the counts this consumes
- `src-tauri/crates/trigger-git/src/lib.rs` the `PAYLOAD_BEHIND` list (the payload must match it)
- `src-tauri/crates/trigger-git/src/logic.rs` the last lines, where modules are declared

## Steps

1. Create `src-tauri/crates/trigger-git/src/logic/polled.rs` with exactly this content. The decisions in it (first count is a baseline, a missing answer changes nothing, 2 then 5 is one event) are the card's choices for the cases Decision 10 does not spell out.

```rust
//! Pure transition logic for the three polled kinds. The glue polls a `Repo`
//! (or scans a folder) on the rule's interval and feeds each answer to one of
//! these; they say whether the answer fires. No git, no clock, no threads.
//! Decision 10 of plans/003-trigger-git/epic.md.

use trigger_core::Value;

use super::Payload;

/// `git.behind-remote`: fires when the count of commits behind the upstream
/// goes from 0 to above 0.
#[derive(Debug, Default)]
pub struct BehindWatch {
    last: Option<u32>,
}

impl BehindWatch {
    pub fn new() -> BehindWatch {
        BehindWatch::default()
    }

    /// Feeds one poll. `behind` is what `Repo::behind` returned. Returns the
    /// count when this poll fires.
    ///
    /// - The first count is a baseline and never fires, even above 0.
    /// - After a fire the count must return to 0 before it can fire again, so
    ///   2 then 5 is one event, and 0, 2, 0, 1 is two.
    /// - None (no upstream, git failed) says nothing: the last count stays.
    pub fn observe(&mut self, behind: Option<u32>) -> Option<u32> {
        let now = behind?;
        let fires = self.last == Some(0) && now > 0;
        self.last = Some(now);
        fires.then_some(now)
    }
}

/// The payload of `git.behind-remote`: exactly the fields listed in `KINDS`.
pub fn behind_payload(repo: &str, branch: Option<&str>, behind: u32) -> Payload {
    let mut out = Payload::new();
    out.insert("repo".into(), Value::from(repo));
    out.insert("branch".into(), Value::from(branch.unwrap_or_default()));
    out.insert("behind".into(), Value::from(behind));
    out
}
```

2. Create `src-tauri/crates/trigger-git/src/logic/polled_tests.rs` with exactly this content. This file also holds the tests of T18 and T19, so the helpers `listed` and `keys` are shared.

```rust
use super::polled::*;
use super::*;

/// The payload field names `KINDS` lists for a kind, sorted.
fn listed(kind: &str) -> Vec<&'static str> {
    let info = crate::KINDS.iter().find(|k| k.id == kind).unwrap();
    let mut names: Vec<&'static str> = info.payload.iter().map(|f| f.name).collect();
    names.sort_unstable();
    names
}

fn keys(payload: &Payload) -> Vec<&str> {
    payload.keys().map(String::as_str).collect()
}

#[test]
fn behind_never_fires_on_the_first_count() {
    let mut watch = BehindWatch::new();
    assert_eq!(watch.observe(Some(3)), None);
    let mut zero = BehindWatch::new();
    assert_eq!(zero.observe(Some(0)), None);
}

#[test]
fn behind_fires_when_the_count_leaves_zero() {
    let mut watch = BehindWatch::new();
    assert_eq!(watch.observe(Some(0)), None);
    assert_eq!(watch.observe(Some(0)), None);
    assert_eq!(watch.observe(Some(2)), Some(2));
}

#[test]
fn behind_fires_once_while_the_count_stays_above_zero() {
    let mut watch = BehindWatch::new();
    watch.observe(Some(0));
    assert_eq!(watch.observe(Some(2)), Some(2));
    assert_eq!(watch.observe(Some(2)), None);
    assert_eq!(watch.observe(Some(5)), None);
    assert_eq!(watch.observe(Some(1)), None);
}

#[test]
fn behind_fires_again_after_a_pull_brings_it_back_to_zero() {
    let mut watch = BehindWatch::new();
    watch.observe(Some(0));
    assert_eq!(watch.observe(Some(2)), Some(2));
    assert_eq!(watch.observe(Some(0)), None);
    assert_eq!(watch.observe(Some(1)), Some(1));
}

#[test]
fn behind_ignores_polls_with_no_answer() {
    let mut watch = BehindWatch::new();
    assert_eq!(watch.observe(None), None);
    watch.observe(Some(0));
    assert_eq!(watch.observe(None), None);
    // The 0 is still remembered, so the next real count fires.
    assert_eq!(watch.observe(Some(4)), Some(4));
    assert_eq!(watch.observe(None), None);
    assert_eq!(watch.observe(Some(4)), None);
}

#[test]
fn behind_payload_has_exactly_the_listed_fields() {
    let payload = behind_payload("/work/app", Some("main"), 3);
    assert_eq!(keys(&payload), listed("git.behind-remote"));
    assert_eq!(payload["repo"], "/work/app");
    assert_eq!(payload["branch"], "main");
    assert_eq!(payload["behind"], 3);
    assert_eq!(behind_payload("/r", None, 1)["branch"], "");
}
```

3. In `src-tauri/crates/trigger-git/src/logic.rs`, add this at the very end of the file, after a blank line.

```rust

pub mod polled;

#[cfg(test)]
mod polled_tests;
```

4. Run `cd src-tauri && cargo fmt -p trigger-git`.

Note on `check-crates.mjs`: it prints warnings, not errors, about template text. Until slice C it may print `README.md still has template text`, `src/lib.rs still has template code`, `src/os.rs still has template code` and a length warning for `src/logic/tests.rs`. Warnings are fine; only errors fail the check. Do not try to remove them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes: 82 tests in the library, 1 in `readme`, `live` shows 1 ignored
- [ ] `cd src-tauri && cargo test -p trigger-git polled_tests` runs 6 tests and they pass
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- The interval timer, the thread, and calling `Repo::behind`. That is the glue in slice C.
- The `branch` glob. The glue checks `Parsed::branch_matches` before it calls `observe`.
- The other two polled kinds (T18, T19).
- Changing `RepoSnapshot`, `Change` or `step`.
- Removing `#![allow(dead_code)]` from `logic.rs`.

## If stuck

- If `behind_payload_has_exactly_the_listed_fields` fails on the field names, `KINDS` in `lib.rs` lists different payload fields than T02 defined. Set status blocked and quote both lists.
- If the compiler says `Payload` is not found in `super`, `logic.rs` does not define it (T04 is not done). Set status blocked.
- If clippy reports `dead_code`, the line `#![allow(dead_code)]` at the top of `logic.rs` was lost. Put it back.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (82 library tests, polled_tests 6 pass, clippy and fmt clean, check-crates exit 0)
- 2026-10-12 checker: pass
