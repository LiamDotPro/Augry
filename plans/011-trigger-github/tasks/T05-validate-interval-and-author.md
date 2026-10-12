---
id: T05
epic: 011-trigger-github
title: Validate interval_s (at least 60) and author
status: todo
depends_on: [T04]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-github/src/rule.rs
---

## Goal

`rule::parse` refuses an `interval_s` under 60 with an error that names `interval_s`, and an `author` list that is empty or has an empty or spaced login, naming `author`.

## Read first

- `plans/011-trigger-github/epic.md` Decision 13 (`interval_s` under 60 is a `RuleError`) and Decision 4 (why: rate limits)
- `src-tauri/crates/trigger-github/src/rule.rs` the file after T04

## Steps

1. Replace the whole of `src-tauri/crates/trigger-github/src/rule.rs` with this. New since T04: `MIN_INTERVAL_S`, the `field` helper closure in `parse`, the interval and author checks, and the tests `interval_has_a_floor_of_sixty_seconds` and `author_needs_real_logins`.

```rust
//! The `when` object of a rule, typed. Pure.

// Removed in slice C, when the poll loop reads everything in this file.
#![allow(dead_code)]

use serde::Deserialize;
use trigger_core::{Rule, RuleError};

/// Who counts as a dependency bot when `author` is not given.
pub const DEFAULT_AUTHORS: &[&str] = &["dependabot[bot]", "renovate[bot]"];

/// GitHub allows 5000 requests an hour; a floor of a minute keeps many rules
/// well inside that.
pub const MIN_INTERVAL_S: u64 = 60;

fn default_interval_s() -> u64 {
    120
}

fn default_author() -> Vec<String> {
    DEFAULT_AUTHORS.iter().map(|a| a.to_string()).collect()
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct When {
    pub repo: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default = "default_interval_s")]
    pub interval_s: u64,
    #[serde(default = "default_author")]
    pub author: Vec<String>,
}

/// The app fills this in from the pane's origin before the rule starts; the
/// editor's `check` sees it unresolved, so it is accepted as it is.
const REPO_VARIABLE: &str = "${repo.remote}";

/// `owner/name`. The text goes straight into a request path, so only the
/// characters GitHub allows in names are let through.
fn check_repo(repo: &str) -> Result<(), String> {
    if repo == REPO_VARIABLE {
        return Ok(());
    }
    let valid = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    match repo.split_once('/') {
        Some((owner, name)) if valid(owner) && valid(name) => Ok(()),
        _ => Err("must look like owner/name, such as cli/cli".into()),
    }
}

/// A parsed rule: the typed fields plus anything compiled from them.
#[derive(Debug, Clone)]
pub struct Parsed {
    pub when: When,
}

/// Checks that the kind belongs to this crate and the fields parse. Used by
/// `check` (the editor) and by `start`.
pub fn parse(rule: &Rule) -> Result<Parsed, RuleError> {
    if !crate::KINDS.iter().any(|k| k.id == rule.kind) {
        return Err(RuleError::UnknownKind(rule.kind.clone()));
    }
    let value = serde_json::Value::Object(rule.when.clone().into_iter().collect());
    let when: When = serde_json::from_value(value).map_err(|e| RuleError::Field {
        field: "when".into(),
        problem: e.to_string(),
    })?;
    let field = |field: &str, problem: String| RuleError::Field {
        field: field.into(),
        problem,
    };
    check_repo(&when.repo).map_err(|problem| field("repo", problem))?;
    if when.interval_s < MIN_INTERVAL_S {
        return Err(field(
            "interval_s",
            format!("must be at least {MIN_INTERVAL_S} seconds"),
        ));
    }
    if when.author.is_empty()
        || when
            .author
            .iter()
            .any(|a| a.is_empty() || a.contains(char::is_whitespace))
    {
        return Err(field(
            "author",
            "must list at least one login, with no empty or spaced entries".into(),
        ));
    }
    Ok(Parsed { when })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use trigger_core::testkit::rule;

    fn parsed(when: serde_json::Value) -> Result<Parsed, RuleError> {
        parse(&rule("p/t", "github.ci-failed", when))
    }

    fn field_of(err: RuleError) -> String {
        match err {
            RuleError::Field { field, .. } => field,
            other => panic!("expected a field error, got {other:?}"),
        }
    }

    #[test]
    fn defaults_are_filled_in() {
        let when = parsed(json!({ "repo": "cli/cli" })).unwrap().when;
        assert_eq!(when.repo, "cli/cli");
        assert_eq!(when.branch, None);
        assert_eq!(when.interval_s, 120);
        assert_eq!(
            when.author,
            vec!["dependabot[bot]".to_string(), "renovate[bot]".to_string()]
        );
    }

    #[test]
    fn all_fields_are_read() {
        let when = parsed(json!({
            "repo": "cli/cli",
            "branch": "release/*",
            "interval_s": 300,
            "author": ["octocat"]
        }))
        .unwrap()
        .when;
        assert_eq!(when.branch.as_deref(), Some("release/*"));
        assert_eq!(when.interval_s, 300);
        assert_eq!(when.author, vec!["octocat".to_string()]);
    }

    #[test]
    fn every_kind_is_accepted() {
        for kind in crate::KINDS {
            let r = rule("p/t", kind.id, json!({ "repo": "cli/cli" }));
            assert!(parse(&r).is_ok(), "{} should parse", kind.id);
        }
    }

    #[test]
    fn rejects_unknown_fields_wrong_types_missing_repo_and_other_kinds() {
        let typo = parsed(json!({ "repo": "a/b", "brach": "x" }));
        assert_eq!(field_of(typo.unwrap_err()), "when");
        let wrong_type = parsed(json!({ "repo": "a/b", "interval_s": "fast" }));
        assert_eq!(field_of(wrong_type.unwrap_err()), "when");
        let negative = parsed(json!({ "repo": "a/b", "interval_s": -5 }));
        assert_eq!(field_of(negative.unwrap_err()), "when");
        let single = parsed(json!({ "repo": "a/b", "author": "octocat" }));
        assert_eq!(field_of(single.unwrap_err()), "when");
        let missing = parsed(json!({}));
        assert_eq!(field_of(missing.unwrap_err()), "when");
        let other = parse(&rule("p/t", "fs.file-saved", json!({ "repo": "a/b" })));
        assert!(matches!(other, Err(RuleError::UnknownKind(_))));
    }

    #[test]
    fn repo_must_look_like_owner_slash_name() {
        for good in ["cli/cli", "my-org/my_repo.js", "a/b", "${repo.remote}"] {
            assert!(
                parsed(json!({ "repo": good })).is_ok(),
                "{good} should pass"
            );
        }
        for bad in [
            "",
            "cli",
            "cli/",
            "/cli",
            "a/b/c",
            "a b/c",
            "a/b?x=1",
            "a/../b",
            "../b",
            "a/b\n",
            "https://github.com/cli/cli",
            "${repo.root}",
            "${repo.remote}/x",
        ] {
            let err = parsed(json!({ "repo": bad })).unwrap_err();
            assert_eq!(field_of(err), "repo", "{bad:?} should be rejected");
        }
    }

    #[test]
    fn interval_has_a_floor_of_sixty_seconds() {
        for good in [60, 61, 120, 86_400] {
            assert!(parsed(json!({ "repo": "a/b", "interval_s": good })).is_ok());
        }
        for bad in [0, 1, 59] {
            let err = parsed(json!({ "repo": "a/b", "interval_s": bad })).unwrap_err();
            assert_eq!(field_of(err), "interval_s", "{bad} should be rejected");
        }
    }

    #[test]
    fn author_needs_real_logins() {
        let ok = parsed(json!({ "repo": "a/b", "author": ["dependabot[bot]", "me"] }));
        assert!(ok.is_ok());
        for bad in [json!([]), json!([""]), json!(["a b"]), json!(["ok", " "])] {
            let err = parsed(json!({ "repo": "a/b", "author": bad })).unwrap_err();
            assert_eq!(field_of(err), "author");
        }
    }
}
```

2. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib` passes: 7 tests
- [ ] `cd src-tauri && cargo test -p trigger-github` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- A maximum interval, or raising the floor.
- Checking logins against GitHub's real naming rules. Empty and spaced is all that is refused.
- The branch glob (T06) and the README (T07).

## If stuck

- If a test fails, compare the code with step 1 character by character; do not change the test.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
