---
id: T03
epic: 011-trigger-github
title: Read branch, interval_s and author in rule.rs
status: todo
depends_on: [T02]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-github/src/rule.rs
---

## Goal

`rule::When` has all four `when` fields (`repo`, `branch`, `interval_s`, `author`) with their defaults, and unit tests prove the defaults and the type errors. No validation of the values yet (T04 to T06).

## Read first

- `plans/011-trigger-github/epic.md` Decision 3 and Decision 13 (the fields and defaults: `interval_s` 120, `author` the two bots)
- `src-tauri/crates/trigger-github/src/rule.rs` the file after T02
- `src-tauri/crates/trigger-fs/src/rule.rs` the same pattern with `#[serde(default)]` for another crate

## Steps

1. Replace the whole of `src-tauri/crates/trigger-github/src/rule.rs` with this. `author` is a list of logins (a single string is a type error), `branch` is optional, and `interval_s` is an unsigned number so a negative value is already refused.

```rust
//! The `when` object of a rule, typed. Pure.

// Removed in slice C, when the poll loop reads everything in this file.
#![allow(dead_code)]

use serde::Deserialize;
use trigger_core::{Rule, RuleError};

/// Who counts as a dependency bot when `author` is not given.
pub const DEFAULT_AUTHORS: &[&str] = &["dependabot[bot]", "renovate[bot]"];

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
}
```

2. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib` passes: 4 tests
- [ ] `cd src-tauri && cargo test -p trigger-github` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Validating `repo`, `interval_s`, `author` or `branch` values (T04 to T06).
- Changing the defaults or turning `author` into a single string. Decided in the epic.
- Editing the README: T07 writes the Rule fields section for T03 to T06 together.
- Removing `#![allow(dead_code)]`.

## If stuck

- If a test fails, compare the code with step 1 character by character; do not change the test.
- If clippy warns about dead code, the `#![allow(dead_code)]` line was lost. Put it back.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
