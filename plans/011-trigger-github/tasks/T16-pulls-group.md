---
id: T16
epic: 011-trigger-github
title: Fire review-requested, merged and dependency PRs
status: todo
depends_on: [T15]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-github/src/logic/pulls.rs
  - src-tauri/crates/trigger-github/src/logic.rs
---

## Goal

The crate has `logic/pulls.rs`: `fires` reads one answer of the pull request list and returns the firings for `github.review-requested`, `github.dependency-pr-opened` and `github.pr-merged`, remembers every pull request number in `RepoState.prs`, and forgets a review request once it stops, so asking again fires again. 12 tests use the fixture.

## Read first

- `plans/011-trigger-github/research/R01-endpoints.md` the "Fire when" table rows for review-requested, dependency-pr-opened and pr-merged
- `src-tauri/crates/trigger-github/src/logic/diff.rs` `each_new`, `Spec`, `Feed`, `kind` (created by T14)
- `src-tauri/crates/trigger-github/src/logic/testutil.rs` the helpers the tests use (created by T15)
- `src-tauri/crates/trigger-github/src/rule.rs` `Parsed::matches_branch`, `When.author` (slice A)

## Steps

1. Create `src-tauri/crates/trigger-github/src/logic/pulls.rs` with exactly this.

The three kinds, as the code has them:

- review-requested: the pull request is `open` and `requested_reviewers` has the signed in login (case ignored). It fires when its `id` is not in the seen set. A pull request in the answer that no longer asks for you is removed from the seen set, so a new request fires again; one that merely falls off the first page is kept.
- dependency-pr-opened: any pull request whose `user.login` is one of the rule's `author` logins (default `dependabot[bot]`, `renovate[bot]`), when its `id` is new. State is not checked, so a bot pull request opened and merged between two polls still counts.
- pr-merged: `merged_at` is not null, the `id` is new, and the rule's branch glob matches `base.ref`.

Seen sets are per kind, so one pull request can fire all three kinds once each. Every rule of a kind is checked against the same item.

```rust
//! The pull request list: `github.review-requested`, `github.dependency-pr-opened`
//! and `github.pr-merged` all read the same answer. Pure.

use std::collections::BTreeSet;

use super::diff::{each_new, kind, Feed, Fire, Spec};
use super::json::{id_key, list, logins, num, text, BodyError, Value};
use super::payload;
use super::text::same_login;
use super::Armed;

/// Reads one answer of the pull request list and returns every firing, with
/// the keys it saw remembered in `feed`. It also adds every pull request
/// number to `prs`, for `github.pr-commented`, whether or not a rule needs the
/// answer. On a body that is not a list nothing changes.
pub fn fires(
    body: &str,
    feed: &mut Feed,
    prs: &mut BTreeSet<u64>,
    rules: &[&Armed],
    me: &str,
) -> Result<Vec<Fire>, BodyError> {
    let items = list(body, None)?;
    prs.extend(items.iter().map(|i| num(i, "/number")).filter(|n| *n != 0));
    let mut out = Vec::new();

    // Open and waiting on you.
    let wants_me = |v: &Value| {
        text(v, "/state") == "open"
            && logins(v, "/requested_reviewers")
                .iter()
                .any(|login| same_login(login, me))
    };
    let requested_key = |v: &Value| wants_me(v).then(|| id_key(v)).flatten();
    let always = |_: &Armed, _: &Value| true;
    let review = Spec {
        kind: kind::REVIEW_REQUESTED,
        key: &requested_key,
        accept: &always,
        build: payload::review,
    };
    each_new(feed, &review, rules, &items, &mut out);
    // A pull request that stops waiting on you is forgotten, so asking again fires again.
    if let Some(seen) = feed.seen.get_mut(kind::REVIEW_REQUESTED) {
        for item in items.iter().filter(|i| !wants_me(i)) {
            if let Some(key) = id_key(item) {
                seen.remove(&key);
            }
        }
    }

    // Opened by one of the rule's authors.
    let by_author = |rule: &Armed, v: &Value| {
        let author = text(v, "/user/login");
        rule.parsed
            .when
            .author
            .iter()
            .any(|wanted| same_login(wanted, author))
    };
    let dependency = Spec {
        kind: kind::DEPENDENCY_PR_OPENED,
        key: &id_key,
        accept: &by_author,
        build: payload::dependency,
    };
    each_new(feed, &dependency, rules, &items, &mut out);

    // Merged into a branch the rule matches.
    let merged_key = |v: &Value| {
        if text(v, "/merged_at").is_empty() {
            None
        } else {
            id_key(v)
        }
    };
    let into_branch = |rule: &Armed, v: &Value| rule.parsed.matches_branch(text(v, "/base/ref"));
    let merged = Spec {
        kind: kind::PR_MERGED,
        key: &merged_key,
        accept: &into_branch,
        build: payload::merged,
    };
    each_new(feed, &merged, rules, &items, &mut out);

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::testutil::{armed, edit, only, without, ME, PULLS};
    use serde_json::json;

    // Ids in tests/fixtures/pulls.json.
    const MERGED_TRUNK: u64 = 4830652819; // #14648
    const RENOVATE: u64 = 4829000001; // #14651
    const DEPENDABOT: u64 = 4829000002; // #14650
    const REVIEW_ME: u64 = 4829000003; // #14649
    const MERGED_RELEASE: u64 = 4829000004; // #14640
    const CLOSED_UNMERGED: u64 = 4829000005; // #14630
    const OLD_OPEN: u64 = 4829000006; // #14620

    struct World {
        feed: Feed,
        prs: BTreeSet<u64>,
        rules: Vec<Armed>,
    }

    impl World {
        fn new(rules: Vec<Armed>) -> World {
            World {
                feed: Feed::default(),
                prs: BTreeSet::new(),
                rules,
            }
        }

        fn poll(&mut self, body: &str) -> Vec<Fire> {
            let refs: Vec<&Armed> = self.rules.iter().collect();
            fires(body, &mut self.feed, &mut self.prs, &refs, ME).unwrap()
        }
    }

    fn rule(id: &str, kind: &str, extra: Value) -> Armed {
        let mut when = json!({ "repo": "cli/cli" });
        for (k, v) in extra.as_object().unwrap() {
            when[k] = v.clone();
        }
        armed(id, kind, when)
    }

    fn all_three() -> Vec<Armed> {
        vec![
            rule("p/review", kind::REVIEW_REQUESTED, json!({})),
            rule("p/deps", kind::DEPENDENCY_PR_OPENED, json!({})),
            rule("p/merged", kind::PR_MERGED, json!({})),
        ]
    }

    fn numbers(fires: &[Fire], rule: &str) -> Vec<u64> {
        fires
            .iter()
            .filter(|f| f.rule == rule)
            .map(|f| f.payload["number"].as_u64().unwrap())
            .collect()
    }

    #[test]
    fn new_pull_requests_fire_the_right_kinds_after_a_seed() {
        let mut w = World::new(all_three());
        let seed = only(PULLS, &[MERGED_RELEASE, CLOSED_UNMERGED, OLD_OPEN]);
        w.poll(&seed);
        let got = w.poll(PULLS);
        assert_eq!(numbers(&got, "p/review"), [14649]);
        assert_eq!(numbers(&got, "p/deps"), [14651, 14650]);
        assert_eq!(numbers(&got, "p/merged"), [14648]);
        assert_eq!(got.len(), 4);
        assert!(w.poll(PULLS).is_empty());
    }

    #[test]
    fn payloads_are_the_documented_shapes() {
        let mut w = World::new(all_three());
        let got = w.poll(&only(PULLS, &[MERGED_TRUNK, DEPENDABOT, REVIEW_ME]));
        let review = got.iter().find(|f| f.rule == "p/review").unwrap();
        assert_eq!(review.kind, kind::REVIEW_REQUESTED);
        assert_eq!(review.payload["head"], json!("octocat-flag"));
        assert_eq!(review.payload["base"], json!("trunk"));
        assert_eq!(review.payload["author"], json!("octocat"));
        let deps = got.iter().find(|f| f.rule == "p/deps").unwrap();
        assert_eq!(deps.payload["author"], json!("dependabot[bot]"));
        let merged = got.iter().find(|f| f.rule == "p/merged").unwrap();
        assert_eq!(merged.payload["merged_at"], json!("2026-10-12T16:14:17Z"));
    }

    #[test]
    fn review_requested_needs_an_open_pull_request_asking_me() {
        let mut w = World::new(vec![rule("p/r", kind::REVIEW_REQUESTED, json!({}))]);
        let got = w.poll(PULLS);
        assert_eq!(numbers(&got, "p/r"), [14649]);
        // #14648 asks sergiou87 but is closed; #14649 asks me and sergiou87.
        let mut w = World::new(vec![rule("p/r", kind::REVIEW_REQUESTED, json!({}))]);
        let closed = edit(PULLS, REVIEW_ME, |i| i["state"] = json!("closed"));
        assert!(w.poll(&closed).is_empty());
    }

    #[test]
    fn logins_are_compared_without_case() {
        let rules = [rule("p/r", kind::REVIEW_REQUESTED, json!({}))];
        let refs: Vec<&Armed> = rules.iter().collect();
        let (mut feed, mut prs) = (Feed::default(), BTreeSet::new());
        let got = fires(PULLS, &mut feed, &mut prs, &refs, "liamdotpro").unwrap();
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn a_new_request_after_it_was_withdrawn_fires_again() {
        let mut w = World::new(vec![rule("p/r", kind::REVIEW_REQUESTED, json!({}))]);
        assert_eq!(w.poll(PULLS).len(), 1);
        assert!(w.poll(PULLS).is_empty());
        let withdrawn = edit(PULLS, REVIEW_ME, |i| {
            i["requested_reviewers"] = json!([{ "login": "sergiou87" }]);
        });
        assert!(w.poll(&withdrawn).is_empty());
        assert_eq!(w.poll(PULLS).len(), 1);
    }

    #[test]
    fn a_pull_request_that_drops_out_of_the_page_is_not_forgotten() {
        let mut w = World::new(vec![rule("p/r", kind::REVIEW_REQUESTED, json!({}))]);
        assert_eq!(w.poll(PULLS).len(), 1);
        let page_without_it = without(PULLS, &[REVIEW_ME]);
        assert!(w.poll(&page_without_it).is_empty());
        assert!(w.poll(PULLS).is_empty());
    }

    #[test]
    fn merged_respects_the_branch_glob_on_the_base() {
        let mut w = World::new(vec![
            rule("p/any", kind::PR_MERGED, json!({})),
            rule("p/trunk", kind::PR_MERGED, json!({ "branch": "trunk" })),
            rule("p/rel", kind::PR_MERGED, json!({ "branch": "release/*" })),
        ]);
        let got = w.poll(PULLS);
        assert_eq!(numbers(&got, "p/any"), [14648, 14640]);
        assert_eq!(numbers(&got, "p/trunk"), [14648]);
        assert_eq!(numbers(&got, "p/rel"), [14640]);
    }

    #[test]
    fn a_closed_pull_request_that_was_not_merged_never_fires_merged() {
        let mut w = World::new(vec![rule("p/m", kind::PR_MERGED, json!({}))]);
        let got = w.poll(&only(PULLS, &[CLOSED_UNMERGED, OLD_OPEN]));
        assert!(got.is_empty());
    }

    #[test]
    fn dependency_prs_use_the_rules_author_list() {
        let mut w = World::new(vec![
            rule("p/default", kind::DEPENDENCY_PR_OPENED, json!({})),
            rule(
                "p/one",
                kind::DEPENDENCY_PR_OPENED,
                json!({ "author": ["DEPENDABOT[bot]"] }),
            ),
            rule(
                "p/human",
                kind::DEPENDENCY_PR_OPENED,
                json!({ "author": ["A4-Tacks"] }),
            ),
        ]);
        let got = w.poll(PULLS);
        assert_eq!(numbers(&got, "p/default"), [14651, 14650]);
        assert_eq!(numbers(&got, "p/one"), [14650]);
        assert_eq!(numbers(&got, "p/human"), [14648]);
    }

    #[test]
    fn two_rules_of_one_kind_both_fire_for_the_same_pull_request() {
        let mut w = World::new(vec![
            rule("p/a", kind::DEPENDENCY_PR_OPENED, json!({})),
            rule("p/b", kind::DEPENDENCY_PR_OPENED, json!({})),
        ]);
        let got = w.poll(&only(PULLS, &[RENOVATE]));
        let rules: Vec<&str> = got.iter().map(|f| f.rule.as_str()).collect();
        assert_eq!(rules, ["p/a", "p/b"]);
    }

    #[test]
    fn every_number_is_noted_even_with_no_rules() {
        let mut w = World::new(vec![]);
        assert!(w.poll(PULLS).is_empty());
        assert_eq!(w.prs.len(), 7);
        assert!(w.prs.contains(&14649));
        assert!(w.feed.seen.is_empty());
    }

    #[test]
    fn a_bad_body_changes_nothing() {
        let mut w = World::new(all_three());
        let refs: Vec<&Armed> = w.rules.iter().collect();
        let err = fires(
            "{\"message\":\"Not Found\"}",
            &mut w.feed,
            &mut w.prs,
            &refs,
            ME,
        );
        assert!(err.is_err());
        assert!(w.prs.is_empty());
        assert!(w.feed.seen.is_empty());
    }
}
```

2. Edit `logic.rs` so it declares the module. Replace the line `pub mod payload;` with the two lines below. Use exactly this edit in `src-tauri/crates/trigger-github/src/logic.rs`.

Find this text, which appears once:

```rust
pub mod payload;
```

Replace it with:

```rust
pub mod payload;
pub mod pulls;
```

3. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib` passes with 81 tests (12 new in this card)
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (the `source`, `readme` and `live` tests included)
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (warnings that `lib.rs` and `os.rs` still have template code are expected until slice C)
- [ ] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- The `diff` function that does seeding. It is T19, and these tests do the seeding by hand (a first call whose result they ignore).
- Changing `payload.rs`, `diff.rs` or the fixtures.
- Removing `#![allow(dead_code)]` from `logic.rs`.

## If stuck

- If a test fails, compare the file with step 1 character by character. Do not change a test or a fixture.
- If clippy says `useless_vec`, a `vec![` in a test should be an array `[`. Only if the file differs from the card.
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
