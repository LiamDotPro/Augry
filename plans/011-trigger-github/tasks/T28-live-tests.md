---
id: T28
epic: 011-trigger-github
title: Add the live tests against the real GitHub
status: todo
depends_on: [T27]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-github/tests/live.rs
---

## Goal

`tests/live.rs` has two `#[ignore]` tests that run the real source against GitHub through the user's own `gh` login, read only, against the public repository `cli/cli`. They print a message and pass without testing anything when `gh` is missing or not signed in. One checks that `availability()` agrees with whether `gh` is installed; the other arms one rule of every kind, waits ten seconds, and checks that the first poll started without an error and fired nothing (a first poll only seeds).

## Read first

- `plans/011-trigger-github/epic.md` Decisions 7 and 9 ("The first poll of a repo only seeds state")
- `src-tauri/crates/trigger-github/tests/live.rs` the template test you replace
- `src-tauri/crates/trigger-fs/tests/live.rs` lines 1 to 12 (how the sibling crate documents `--ignored`; read only)
- `src-tauri/crates/trigger-core/src/testkit.rs` `RecordingSink::wait_for`, `drain`, `rule`

## Steps

1. Replace the whole of `src-tauri/crates/trigger-github/tests/live.rs` with exactly this.

Why it is shaped like this:

- Nothing on GitHub can be made to happen on demand, and a first poll fires nothing, so the tests cannot wait for a real event. They check the parts that can be checked: `start` succeeds (or says why it cannot), and ten seconds later the sink is still empty.
- `start` returning `SourceError::Unavailable` is the skip: it covers `gh` missing and `gh` not signed in. `SourceError::Os` is a failure: the question could not be answered.
- Six requests plus the login are made to GitHub, once. They are all `GET`s and the repository is public.

```rust
//! Tests against the real GitHub, through the user's own `gh` login. Not run by
//! `cargo test`; run them on a machine where `gh auth login` was done with:
//!
//!     cargo test -p trigger-github -- --ignored --nocapture
//!
//! They only read, and only a public repository (`cli/cli`). When `gh` is not
//! installed or not signed in they print why and pass without testing
//! anything, so a CI machine without a login stays green. The first poll of a
//! rule only records what exists and fires nothing, so there is no way to make
//! GitHub fire an event on demand; these tests check that arming rules against
//! the real API starts and stays quiet.

use std::time::Duration;

use serde_json::json;
use trigger_core::testkit::{rule, RecordingSink};
use trigger_core::{Availability, Rule, Source, SourceError};
use trigger_github::{GithubSource, KINDS};

/// One rule per kind, so one first poll makes all six requests.
fn every_kind_on_cli() -> Vec<Rule> {
    KINDS
        .iter()
        .map(|k| {
            rule(
                &format!("live/{}", k.id),
                k.id,
                json!({ "repo": "cli/cli", "interval_s": 60 }),
            )
        })
        .collect()
}

#[test]
#[ignore]
fn availability_says_whether_gh_is_installed() {
    match GithubSource.availability() {
        Availability::Available => println!("gh found"),
        Availability::Unsupported { reason } => {
            assert!(reason.contains("https://cli.github.com"), "{reason}");
            println!("skipped: {reason}");
        }
        other => panic!("unexpected availability {other:?}"),
    }
}

#[test]
#[ignore]
fn the_first_poll_of_the_real_api_seeds_and_fires_nothing() {
    let sink = RecordingSink::new();
    let handle = match GithubSource.start(every_kind_on_cli(), sink.tx.clone()) {
        Ok(handle) => handle,
        Err(SourceError::Unavailable(why)) => {
            println!("skipped: {why}");
            return;
        }
        Err(SourceError::Os(why)) => panic!("start failed: {why}"),
    };
    // Six requests, one after another. Ten seconds is generous; the next poll
    // is a minute away, so anything that arrives now came from the first one.
    assert!(sink
        .wait_for("github.release-published", Duration::from_secs(10))
        .is_none());
    assert!(sink.drain().is_empty(), "a first poll must not fire");
    drop(handle);
}
```

2. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github` passes and the `live` test binary reports `0 passed; 0 failed; 2 ignored`
- [ ] `cd src-tauri && cargo test -p trigger-github --test live -- --ignored --nocapture` passes with 2 tests. On a machine with `gh` signed in it takes about ten seconds and prints `gh found`. On a machine without `gh`, or without a login, it prints `skipped: ...` for the tests concerned and still passes
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 and prints no line with `trigger-github`
- [ ] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- A test that makes GitHub fire an event. It would have to write to GitHub.
- Running the live tests in CI. That is a workflow change for slice D or later.
- Asserting anything about a particular issue, pull request or release in `cli/cli`. They change every day.
- Other repositories than `cli/cli`.

## If stuck

- If the second live test fails with `start failed: gh auth status ...`, the question could not be answered (timeout or bad exit code). Run `gh auth status` in a terminal. If it works there, set status blocked with the failure text; do not change the test.
- If it fails on the `is_none` or `drain` assertion, an event fired on the first poll. That is a bug in `logic`, not in the test: set status blocked and say which kind fired (`sink.drain()` shows it; add a temporary `println!` and remove it after).
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
