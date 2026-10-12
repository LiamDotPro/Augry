---
id: T08
epic: 011-trigger-github
title: Test the GithubSource public API
status: done
depends_on: [T07]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-github/tests/source.rs
---

## Goal

An integration test file proves what the app will rely on: the source id is `github`, the eight kind ids are in the decided order, every kind documents `repo` and `interval_s`, every `when` field a kind documents is accepted by `check`, bad rules are named by field, and `start` refuses a bad rule before doing anything. This is the last card of slice A; slices B and C build on the crate as it stands here.

## Read first

- `plans/011-trigger-github/epic.md` Decisions 2, 3 and 13
- `src-tauri/crates/trigger-github/src/lib.rs` `KINDS`, `GithubSource`
- `src-tauri/crates/trigger-core/src/testkit.rs` `rule`, `RecordingSink`

## Steps

1. Create `src-tauri/crates/trigger-github/tests/source.rs` with exactly this:

```rust
//! The public face of the crate: the source id, the kinds, and `check` and
//! `start` on bad rules. Needs no `gh` and no network.

use std::collections::BTreeSet;

use serde_json::json;
use trigger_core::testkit::{rule, RecordingSink};
use trigger_core::{RuleError, Source, SourceError};
use trigger_github::{GithubSource, KINDS};

const EXPECTED: [&str; 8] = [
    "github.issue-assigned",
    "github.review-requested",
    "github.ci-failed",
    "github.ci-passed",
    "github.pr-commented",
    "github.pr-merged",
    "github.release-published",
    "github.dependency-pr-opened",
];

#[test]
fn the_source_has_the_eight_kinds() {
    let source = GithubSource;
    assert_eq!(source.id(), "github");
    let ids: Vec<&str> = source.kinds().iter().map(|k| k.id).collect();
    assert_eq!(ids, EXPECTED);
}

#[test]
fn every_kind_documents_repo_and_interval_and_a_payload() {
    for kind in KINDS {
        let when: Vec<&str> = kind.when.iter().map(|f| f.name).collect();
        assert!(when.contains(&"repo"), "{} has no repo field", kind.id);
        assert!(
            when.contains(&"interval_s"),
            "{} has no interval_s",
            kind.id
        );
        let repo = kind.when.iter().find(|f| f.name == "repo").unwrap();
        assert!(repo.required, "{} must require repo", kind.id);
        assert!(!kind.payload.is_empty(), "{} has no payload", kind.id);
        let names: BTreeSet<&str> = kind.payload.iter().map(|f| f.name).collect();
        assert_eq!(
            names.len(),
            kind.payload.len(),
            "{} repeats a payload field",
            kind.id
        );
    }
}

#[test]
fn every_documented_when_field_is_accepted_by_check() {
    let source = GithubSource;
    for kind in KINDS {
        let mut when = serde_json::Map::new();
        for field in kind.when {
            let value = match field.name {
                "repo" => json!("cli/cli"),
                "branch" => json!("main"),
                "interval_s" => json!(60),
                "author" => json!(["octocat"]),
                other => panic!("{} documents an unknown field {other}", kind.id),
            };
            when.insert(field.name.to_string(), value);
        }
        let r = rule("p/t", kind.id, serde_json::Value::Object(when));
        assert!(
            source.check(&r).is_ok(),
            "{} should accept its own fields",
            kind.id
        );
    }
}

#[test]
fn check_names_the_bad_field() {
    let source = GithubSource;
    let bad = rule("p/t", "github.ci-failed", json!({ "repo": "nope" }));
    match source.check(&bad) {
        Err(RuleError::Field { field, .. }) => assert_eq!(field, "repo"),
        other => panic!("expected a repo error, got {other:?}"),
    }
    let other = rule("p/t", "fs.file-saved", json!({ "repo": "a/b" }));
    assert!(matches!(
        source.check(&other),
        Err(RuleError::UnknownKind(_))
    ));
}

#[test]
fn start_refuses_a_bad_rule_before_doing_anything() {
    let sink = RecordingSink::new();
    let bad = rule(
        "p/t",
        "github.ci-failed",
        json!({ "repo": "a/b", "interval_s": 5 }),
    );
    match GithubSource.start(vec![bad], sink.tx.clone()) {
        Err(SourceError::Os(message)) => assert!(message.contains("interval_s"), "{message}"),
        Err(other) => panic!("expected SourceError::Os, got {other:?}"),
        Ok(_) => panic!("a bad rule must not start"),
    }
}
```

2. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --test source` passes: 5 tests
- [ ] `cd src-tauri && cargo test -p trigger-github` passes: 11 library tests, 5 in `source`, 1 in `readme`, `live` 1 ignored
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (warnings that `lib.rs` and `os.rs` still have template code are expected; slice C clears them)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Tests that run `gh` or touch the network (slice C's `tests/live.rs`).
- Changing `KINDS`, the rule code or the README to make a test pass.
- Registering the crate in the app.

## If stuck

- If `the_source_has_the_eight_kinds` fails on order, compare `KINDS` in `lib.rs` with the `EXPECTED` list; `KINDS` is right, since T02 fixed it. If `KINDS` differs from T02, set status blocked.
- If another test fails, compare the file with step 1 character by character; do not change `src/`.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
