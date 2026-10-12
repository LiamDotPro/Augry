---
id: T18
epic: 011-trigger-github
title: Fire pr-commented from the two comment lists
status: done
depends_on: [T17]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-github/src/logic/comments.rs
  - src-tauri/crates/trigger-github/src/logic.rs
---

## Goal

The crate has `logic/comments.rs`: `issue_fires` and `pull_fires` for `github.pr-commented`. A comment fires when it is someone else's, was created at or after `since`, has an unseen id, and (for the issue comment list) its number, taken from the end of `issue_url`, is a known pull request number. 7 tests use the fixtures.

## Read first

- `plans/011-trigger-github/research/R01-endpoints.md` the pr-commented row of the table and the "pr-commented" bullet under Rules (the `issue_url` check, `created_at >= baseline`, `user.login != me`)
- `src-tauri/crates/trigger-github/src/logic/pulls.rs` how `prs` is filled (created by T16)
- `src-tauri/crates/trigger-github/src/logic/text.rs` `last_number`, `same_login` (created by T12)
- `src-tauri/crates/trigger-github/src/logic/testutil.rs` the helpers the tests use (created by T15)

## Steps

1. Create `src-tauri/crates/trigger-github/src/logic/comments.rs` with exactly this.

Two lists feed one kind. `issues/comments` holds comments on issues and pull requests alike, so its number must be in the set of pull request numbers (`prs`) that `pulls::fires` filled from the pull request list. `pulls/comments` holds review comments, always on a pull request, so it needs no lookup. In both, `since` is compared as text with `created_at` (not `updated_at`, which an edit changes), and the signed in user's own comments are skipped. Each list has its own `Feed`, so the two id spaces never meet.

```rust
//! The two comment lists behind `github.pr-commented`. Pure.
//!
//! `issues/comments` holds comments on issues and on pull requests alike and
//! says nothing about which, so the number at the end of `issue_url` is looked
//! up in the pull request numbers found by the pull request list (R01).
//! `pulls/comments` holds review comments, which are always on a pull request.

use std::collections::BTreeSet;

use super::diff::{each_new, kind, Feed, Fire, Spec};
use super::json::{id_key, list, text, BodyError, Value};
use super::payload;
use super::text::{last_number, same_login};
use super::Armed;

fn anyone(_: &Armed, _: &Value) -> bool {
    true
}

/// A comment counts when it is someone else's and was written at or after
/// `since`. Editing an old comment bumps `updated_at` and so brings it back
/// into the list, but it does not change `created_at`.
fn is_new_from_someone_else(v: &Value, me: &str, since: &str) -> bool {
    text(v, "/created_at") >= since && !same_login(text(v, "/user/login"), me)
}

/// Reads one answer of `issues/comments`. `prs` is the set of pull request
/// numbers of the repository, from `pulls::fires`.
pub fn issue_fires(
    body: &str,
    feed: &mut Feed,
    prs: &BTreeSet<u64>,
    rules: &[&Armed],
    me: &str,
    since: &str,
) -> Result<Vec<Fire>, BodyError> {
    let items = list(body, None)?;
    let on_a_pull_request =
        |v: &Value| last_number(text(v, "/issue_url")).is_some_and(|number| prs.contains(&number));
    let key = |v: &Value| {
        (is_new_from_someone_else(v, me, since) && on_a_pull_request(v))
            .then(|| id_key(v))
            .flatten()
    };
    comments(feed, &key, rules, &items)
}

/// Reads one answer of `pulls/comments`.
pub fn pull_fires(
    body: &str,
    feed: &mut Feed,
    rules: &[&Armed],
    me: &str,
    since: &str,
) -> Result<Vec<Fire>, BodyError> {
    let items = list(body, None)?;
    let key = |v: &Value| {
        is_new_from_someone_else(v, me, since)
            .then(|| id_key(v))
            .flatten()
    };
    comments(feed, &key, rules, &items)
}

fn comments(
    feed: &mut Feed,
    key: &dyn Fn(&Value) -> Option<String>,
    rules: &[&Armed],
    items: &[Value],
) -> Result<Vec<Fire>, BodyError> {
    let spec = Spec {
        kind: kind::PR_COMMENTED,
        key,
        accept: &anyone,
        build: payload::comment,
    };
    let mut out = Vec::new();
    each_new(feed, &spec, rules, items, &mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::json::list as parse_list;
    use crate::logic::testutil::{armed, ISSUE_COMMENTS, ME, PULL_COMMENTS};
    use serde_json::json;

    const SINCE: &str = "2026-10-12T00:00:00Z";

    // Pull request numbers in tests/fixtures/pulls.json that the comments use.
    fn known_prs() -> BTreeSet<u64> {
        BTreeSet::from([14649, 14620, 14648])
    }

    fn rules() -> Vec<Armed> {
        vec![armed(
            "p/c",
            kind::PR_COMMENTED,
            json!({ "repo": "cli/cli" }),
        )]
    }

    fn ids(fires: &[Fire]) -> Vec<u64> {
        fires
            .iter()
            .map(|f| f.payload["id"].as_u64().unwrap())
            .collect()
    }

    fn with_first(body: &str, item: Value) -> String {
        let mut items = parse_list(body, None).unwrap();
        items.insert(0, item);
        Value::Array(items).to_string()
    }

    fn issue_poll(feed: &mut Feed, prs: &BTreeSet<u64>, body: &str) -> Vec<Fire> {
        let rules = rules();
        let refs: Vec<&Armed> = rules.iter().collect();
        issue_fires(body, feed, prs, &refs, ME, SINCE).unwrap()
    }

    #[test]
    fn only_someone_elses_new_comment_on_a_known_pull_request_fires() {
        let mut feed = Feed::default();
        let got = issue_poll(&mut feed, &known_prs(), ISSUE_COMMENTS);
        // Skipped: #14660 is an issue, mine, an old comment edited today, #13923 unknown.
        assert_eq!(ids(&got), [6131952700]);
        let p = &got[0].payload;
        assert_eq!(got[0].kind, kind::PR_COMMENTED);
        assert_eq!(p["author"], json!("sergiou87"));
        assert_eq!(p["created_at"], json!("2026-10-12T20:30:00Z"));
        assert_eq!(
            p["url"],
            json!("https://github.com/cli/cli/issues/14649#issuecomment-6131952700")
        );
        assert!(issue_poll(&mut feed, &known_prs(), ISSUE_COMMENTS).is_empty());
    }

    #[test]
    fn a_number_in_the_pull_request_set_is_what_makes_it_a_pull_request() {
        let mut feed = Feed::default();
        let mut prs = known_prs();
        prs.insert(14660);
        let got = issue_poll(&mut feed, &prs, ISSUE_COMMENTS);
        assert_eq!(ids(&got), [6131952700, 6131952699]);
        let mut empty = Feed::default();
        assert!(issue_poll(&mut empty, &BTreeSet::new(), ISSUE_COMMENTS).is_empty());
    }

    #[test]
    fn a_comment_by_me_is_skipped_whatever_the_case() {
        let rules = rules();
        let refs: Vec<&Armed> = rules.iter().collect();
        let got = issue_fires(
            ISSUE_COMMENTS,
            &mut Feed::default(),
            &known_prs(),
            &refs,
            "sergiou87",
            SINCE,
        )
        .unwrap();
        assert_eq!(ids(&got), [6131952698]);
        let upper = issue_fires(
            ISSUE_COMMENTS,
            &mut Feed::default(),
            &known_prs(),
            &refs,
            "SERGIOU87",
            SINCE,
        )
        .unwrap();
        assert_eq!(ids(&upper), [6131952698]);
    }

    #[test]
    fn a_later_comment_fires_and_an_old_one_edited_does_not() {
        let mut feed = Feed::default();
        issue_poll(&mut feed, &known_prs(), ISSUE_COMMENTS);
        let fresh = json!({
            "id": 6131952800u64,
            "issue_url": "https://api.github.com/repos/cli/cli/issues/14648",
            "html_url": "https://github.com/cli/cli/issues/14648#issuecomment-6131952800",
            "created_at": "2026-10-12T21:00:00Z", "updated_at": "2026-10-12T21:00:00Z",
            "user": { "login": "A4-Tacks" }, "body": "Thanks for merging."
        });
        let got = issue_poll(&mut feed, &known_prs(), &with_first(ISSUE_COMMENTS, fresh));
        assert_eq!(ids(&got), [6131952800]);
    }

    #[test]
    fn a_long_body_is_cut_to_200_characters() {
        let long = json!({
            "id": 6131952801u64,
            "issue_url": "https://api.github.com/repos/cli/cli/issues/14649",
            "html_url": "u", "created_at": "2026-10-12T22:00:00Z",
            "user": { "login": "octocat" }, "body": "é".repeat(300)
        });
        let got = issue_poll(&mut Feed::default(), &known_prs(), &with_first("[]", long));
        assert_eq!(
            got[0].payload["body"].as_str().unwrap().chars().count(),
            200
        );
    }

    #[test]
    fn review_comments_need_no_pull_request_lookup() {
        let rules = rules();
        let refs: Vec<&Armed> = rules.iter().collect();
        let mut feed = Feed::default();
        let got = pull_fires(PULL_COMMENTS, &mut feed, &refs, ME, SINCE).unwrap();
        // Skipped: 4233990601 is mine, 4233990557 was written before SINCE.
        assert_eq!(ids(&got), [4233990600]);
        assert_eq!(got[0].payload["author"], json!("BagToad"));
        assert_eq!(
            got[0].payload["body"],
            json!("Could this use the shared helper instead?")
        );
        assert!(pull_fires(PULL_COMMENTS, &mut feed, &refs, ME, SINCE)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn other_kinds_and_bad_bodies_are_left_alone() {
        let ci = [armed("p/ci", kind::CI_FAILED, json!({ "repo": "cli/cli" }))];
        let refs: Vec<&Armed> = ci.iter().collect();
        let mut feed = Feed::default();
        let got = pull_fires(PULL_COMMENTS, &mut feed, &refs, ME, SINCE).unwrap();
        assert!(got.is_empty());
        assert!(feed.seen.is_empty());
        let rules = rules();
        let refs: Vec<&Armed> = rules.iter().collect();
        assert!(pull_fires("{}", &mut feed, &refs, ME, SINCE).is_err());
        assert!(issue_fires("x", &mut feed, &known_prs(), &refs, ME, SINCE).is_err());
    }
}
```

2. Edit `logic.rs` so it declares the module. Replace the line `pub mod backoff;` with the two lines below. Use exactly this edit in `src-tauri/crates/trigger-github/src/logic.rs`.

Find this text, which appears once:

```rust
pub mod backoff;
```

Replace it with:

```rust
pub mod backoff;
pub mod comments;
```

3. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib` passes with 101 tests (7 new in this card)
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (the `source`, `readme` and `live` tests included)
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (warnings that `lib.rs` and `os.rs` still have template code are expected until slice C)
- [ ] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- Fetching anything for a comment on a pull request that is not in `prs`. It is skipped.
- Changing `payload.rs`, `diff.rs`, `pulls.rs` or the fixtures.
- Removing `#![allow(dead_code)]` from `logic.rs`.

## If stuck

- If a test fails, compare the file with step 1 character by character. Do not change a test or a fixture.
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (101 lib tests, full crate tests, clippy, fmt check, check-crates all exit 0)
- 2026-10-12 checker: pass
