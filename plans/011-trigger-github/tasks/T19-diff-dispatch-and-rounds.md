---
id: T19
epic: 011-trigger-github
title: Add diff() with seeding and test whole rounds
status: todo
depends_on: [T18]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-github/src/logic/diff.rs
  - src-tauri/crates/trigger-github/src/logic/tests.rs
  - src-tauri/crates/trigger-github/src/logic.rs
---

## Goal

`logic::diff::diff` exists: it sends one answer to its group, and the first answer for a request only seeds (fires nothing). `logic/tests.rs` runs whole rounds over all six groups with the fixtures: a silent seed round, a round with 12 firings, a silent repeat. Slice B is then complete: slice C only has to run `gh` and call these functions.

## Read first

- `plans/011-trigger-github/epic.md` Decision 9 ("The first poll of a repo only seeds state")
- `src-tauri/crates/trigger-github/src/logic/diff.rs` the file to edit (T14, T15)
- `src-tauri/crates/trigger-github/src/logic/pulls.rs`, `runs.rs`, `simple.rs`, `comments.rs` the functions `diff` calls (T16 to T18)
- `src-tauri/crates/trigger-github/src/logic/testutil.rs` the helpers the tests use (T15)

## Steps

1. In `src-tauri/crates/trigger-github/src/logic/diff.rs`, make this edit.

Find this text, which appears once:

```rust
use super::json::Value;
```

Replace it with:

```rust
use super::json::{BodyError, Value};
```

2. In `src-tauri/crates/trigger-github/src/logic/diff.rs`, make this edit.

Find this text, which appears once:

```rust
use super::Armed;
```

Replace it with:

```rust
use super::Armed;
use super::{comments, pulls, runs, simple};
```

3. In `src-tauri/crates/trigger-github/src/logic/diff.rs`, make this edit.

Find this text, which appears once:

```rust
#[cfg(test)]
mod tests;
```

Replace it with:

```rust
/// Takes one answer of one request into account and returns what fires.
///
/// - `key` is `feed_key` of the request: it says which remembered feed this
///   answer belongs to.
/// - `rules` are the rules of this repository (any kind; each group picks its
///   own).
/// - `me` is the signed in login and `since` the ISO 8601 time the comment
///   requests were made with (`text::iso` of when the crate started).
///
/// The first answer for a `key` only records what exists and fires nothing, so
/// arming a rule never replays history. Poll `Group::Pulls` before the comment
/// groups in the same round, so the pull request numbers are current.
/// A body that is not the expected JSON is an error and changes nothing.
pub fn diff(
    group: Group,
    body: &str,
    key: &str,
    state: &mut RepoState,
    rules: &[&Armed],
    me: &str,
    since: &str,
) -> Result<Vec<Fire>, BodyError> {
    let RepoState { feeds, prs, .. } = state;
    let feed = feeds.entry(key.to_string()).or_default();
    let found = match group {
        Group::Pulls => pulls::fires(body, feed, prs, rules, me)?,
        Group::Runs => runs::fires(body, feed, rules)?,
        Group::Releases => simple::releases(body, feed, rules)?,
        Group::AssignedIssues => simple::issues(body, feed, rules)?,
        Group::IssueComments => comments::issue_fires(body, feed, prs, rules, me, since)?,
        Group::PullComments => comments::pull_fires(body, feed, rules, me, since)?,
    };
    let first = !feed.seeded;
    feed.seeded = true;
    Ok(if first { Vec::new() } else { found })
}

#[cfg(test)]
mod tests;
```

4. Create `src-tauri/crates/trigger-github/src/logic/tests.rs` with exactly this.

The round helper polls the six groups in the order a poll loop must use: pull requests first, because the comment lists need the pull request numbers. `SINCE` stands for the time the crate started; slice C passes `text::iso` of the start time to both `path_for` and `diff`.

```rust
//! The whole round: every group, polled in order, through `diff` and `State`,
//! with the fixtures. The per group rules are tested in their own files.

use serde_json::json;

use super::diff::{diff, feed_key, kind, path_for, Fire, Group};
use super::testutil::{
    armed, only, ISSUES, ISSUE_COMMENTS, ME, PULLS, PULL_COMMENTS, RELEASES, RUNS,
};
use super::{Armed, State};

const REPO: &str = "cli/cli";
const SINCE: &str = "2026-10-12T00:00:00Z";

fn every_kind() -> Vec<Armed> {
    crate::KINDS
        .iter()
        .map(|k| armed(&format!("p/{}", k.id), k.id, json!({ "repo": REPO })))
        .collect()
}

/// One round over all six groups, in polling order.
fn round(state: &mut State, rules: &[Armed], bodies: [&str; 6]) -> Vec<Fire> {
    let refs: Vec<&Armed> = rules.iter().collect();
    let groups = [
        Group::Pulls,
        Group::Runs,
        Group::Releases,
        Group::AssignedIssues,
        Group::IssueComments,
        Group::PullComments,
    ];
    let mut out = Vec::new();
    for (group, body) in groups.into_iter().zip(bodies) {
        let key = feed_key(REPO, group, ME, None);
        let repo = state.repo(REPO);
        out.extend(diff(group, body, &key, repo, &refs, ME, SINCE).unwrap());
    }
    out
}

fn count(fires: &[Fire], kind: &str) -> usize {
    fires.iter().filter(|f| f.kind == kind).count()
}

#[test]
fn a_seed_round_is_silent_then_news_fires_once_per_kind() {
    let rules = every_kind();
    let mut state = State::default();
    let seed = round(
        &mut state,
        &rules,
        [
            &only(PULLS, &[4829000004, 4829000005, 4829000006]),
            &only(RUNS, &[38303985555, 38303985556]),
            &only(RELEASES, &[399674740, 399674700]),
            &only(ISSUES, &[5741818100]),
            &only(ISSUE_COMMENTS, &[6131952697, 6131952628]),
            &only(PULL_COMMENTS, &[4233990557]),
        ],
    );
    assert!(seed.is_empty(), "{seed:?}");

    let news = round(
        &mut state,
        &rules,
        [PULLS, RUNS, RELEASES, ISSUES, ISSUE_COMMENTS, PULL_COMMENTS],
    );
    assert_eq!(count(&news, kind::REVIEW_REQUESTED), 1);
    assert_eq!(count(&news, kind::DEPENDENCY_PR_OPENED), 2);
    assert_eq!(count(&news, kind::PR_MERGED), 1);
    assert_eq!(count(&news, kind::CI_FAILED), 3);
    assert_eq!(count(&news, kind::CI_PASSED), 1);
    assert_eq!(count(&news, kind::RELEASE_PUBLISHED), 1);
    assert_eq!(count(&news, kind::ISSUE_ASSIGNED), 1);
    // One on an issue comment list (a pull request found just above) and one review comment.
    assert_eq!(count(&news, kind::PR_COMMENTED), 2);
    assert_eq!(news.len(), 12);

    let quiet = round(
        &mut state,
        &rules,
        [PULLS, RUNS, RELEASES, ISSUES, ISSUE_COMMENTS, PULL_COMMENTS],
    );
    assert!(quiet.is_empty(), "{quiet:?}");
}

#[test]
fn every_fire_carries_the_rule_and_exactly_the_documented_payload() {
    let rules = every_kind();
    let mut state = State::default();
    let seed = ["[]", "{\"workflow_runs\":[]}", "[]", "[]", "[]", "[]"];
    assert!(round(&mut state, &rules, seed).is_empty());
    let news = round(
        &mut state,
        &rules,
        [PULLS, RUNS, RELEASES, ISSUES, ISSUE_COMMENTS, PULL_COMMENTS],
    );
    assert!(!news.is_empty());
    for fire in &news {
        assert_eq!(fire.rule, format!("p/{}", fire.kind));
        let info = crate::KINDS.iter().find(|k| k.id == fire.kind).unwrap();
        let mut documented: Vec<&str> = info.payload.iter().map(|f| f.name).collect();
        documented.sort_unstable();
        let keys: Vec<&str> = fire.payload.keys().map(String::as_str).collect();
        assert_eq!(keys, documented, "{}", fire.kind);
    }
}

#[test]
fn a_bad_body_is_an_error_and_the_next_good_one_is_still_a_seed() {
    let rules = every_kind();
    let refs: Vec<&Armed> = rules.iter().collect();
    let mut state = State::default();
    let key = feed_key(REPO, Group::Releases, ME, None);
    let repo = state.repo(REPO);
    let bad = diff(
        Group::Releases,
        "{\"message\":\"Bad\"}",
        &key,
        repo,
        &refs,
        ME,
        SINCE,
    );
    assert!(bad.is_err());
    let seed = diff(Group::Releases, RELEASES, &key, repo, &refs, ME, SINCE).unwrap();
    assert!(seed.is_empty());
}

#[test]
fn repositories_and_branch_filtered_feeds_are_seeded_separately() {
    let rules = every_kind();
    let refs: Vec<&Armed> = rules.iter().collect();
    let mut state = State::default();
    let any = feed_key(REPO, Group::Runs, ME, None);
    let trunk = feed_key(REPO, Group::Runs, ME, Some("trunk"));
    assert_ne!(any, trunk);
    let first = diff(Group::Runs, RUNS, &any, state.repo(REPO), &refs, ME, SINCE).unwrap();
    assert!(first.is_empty());
    let other_feed = diff(
        Group::Runs,
        RUNS,
        &trunk,
        state.repo(REPO),
        &refs,
        ME,
        SINCE,
    )
    .unwrap();
    assert!(other_feed.is_empty());
    let other_repo = diff(Group::Runs, RUNS, &any, state.repo("a/b"), &refs, ME, SINCE).unwrap();
    assert!(other_repo.is_empty());
    assert_eq!(state.repos.len(), 2);
}

#[test]
fn etags_live_in_the_repo_state_by_full_path() {
    let mut state = State::default();
    let path = path_for(REPO, Group::Pulls, ME, None, None);
    state.repo(REPO).set_etag(&path, "W/\"093a24\"");
    assert_eq!(state.repo(REPO).etag(&path), Some("W/\"093a24\""));
    assert_eq!(state.repo("a/b").etag(&path), None);
}
```

5. In `src-tauri/crates/trigger-github/src/logic.rs`, make this edit.

Find this text, which appears once:

```rust
#[cfg(test)]
mod testutil;
```

Replace it with:

```rust
#[cfg(test)]
mod tests;
#[cfg(test)]
mod testutil;
```

6. Run `cd src-tauri && cargo fmt -p trigger-github`. It may reorder the `use` lines in `diff.rs`; that is expected.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib` passes with 106 tests (5 new in this card)
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (the `source`, `readme` and `live` tests included)
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (warnings that `lib.rs` and `os.rs` still have template code are expected until slice C)
- [ ] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- The poll loop, timers, `gh` and `os.rs` (slice C).
- Turning `Fire` into `trigger_core::Event`. Slice C does that, with the clock.
- Changing anything in the group files or the fixtures.
- Removing `#![allow(dead_code)]` from `logic.rs`.

## If stuck

- If one of the six edits in step 1 fails because the text is not found, `diff.rs` is not as T14 and T15 left it. Set status blocked and say which edit.
- If `a_seed_round_is_silent_then_news_fires_once_per_kind` fails with a different count, do not change the test. Run the group tests (`cargo test -p trigger-github --lib logic::pulls`, `logic::runs`, `logic::simple`, `logic::comments`) and report which one fails in the Log; then set status blocked.
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again. It may reorder the `use` lines in `diff.rs`; that is fine.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
