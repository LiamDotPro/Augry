---
id: T06
epic: 011-trigger-github
title: Compile the branch glob with globset
status: todo
depends_on: [T05]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-github/src/rule.rs
---

## Goal

`rule::parse` compiles `when.branch` once with `globset`, refusing a bad glob with an error that names `branch`. `Parsed` gets `matches_branch(name)` for the client side match and `literal_branch()` so slice B can ask GitHub for one exact branch when the setting has no glob characters (R01).

## Read first

- `plans/011-trigger-github/epic.md` Decision 13 (`branch` is a glob matched client side, globset as in `trigger-fs`)
- `plans/011-trigger-github/research/R01-endpoints.md` the CI rule: `branch=` on the API is exact match only
- `src-tauri/crates/trigger-fs/src/rule.rs` `build_set`, the same builder for file globs (this card differs on purpose, see step 1)
- `src-tauri/crates/trigger-github/src/rule.rs` the file after T05

## Steps

1. Replace the whole of `src-tauri/crates/trigger-github/src/rule.rs` with this. New since T05: the `globset` import, `Parsed.branch`, the `impl Parsed`, `compile_branch`, the branch step in `parse`, and four tests. Branch names are case sensitive and contain `/`, so unlike the file globs in trigger-fs the glob is case sensitive and `*` crosses `/`. That difference is intended.

```rust
//! The `when` object of a rule, typed. Pure.

// Removed in slice C, when the poll loop reads everything in this file.
#![allow(dead_code)]

use globset::{GlobBuilder, GlobMatcher};
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

/// A parsed rule: the typed fields plus the compiled branch glob.
#[derive(Debug, Clone)]
pub struct Parsed {
    pub when: When,
    /// None when `when.branch` is not given, which matches every branch.
    pub branch: Option<GlobMatcher>,
}

impl Parsed {
    /// Whether a branch name passes `when.branch`.
    pub fn matches_branch(&self, name: &str) -> bool {
        self.branch.as_ref().is_none_or(|glob| glob.is_match(name))
    }

    /// The branch when it has no glob characters, so the request can ask
    /// GitHub for exactly that branch. None for a glob or no branch at all.
    pub fn literal_branch(&self) -> Option<&str> {
        let branch = self.when.branch.as_deref()?;
        (!branch.contains(['*', '?', '[', ']', '{', '}', '\\'])).then_some(branch)
    }
}

/// Branch names are case sensitive and contain `/`, so unlike the file globs
/// in trigger-fs, `*` here does cross `/`: `release/*` matches `release/1/x`.
fn compile_branch(pattern: &str) -> Result<GlobMatcher, String> {
    if pattern.is_empty() {
        return Err("must not be empty".into());
    }
    GlobBuilder::new(pattern)
        .literal_separator(false)
        .case_insensitive(false)
        .build()
        .map(|glob| glob.compile_matcher())
        .map_err(|e| e.to_string())
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
    let branch = when
        .branch
        .as_deref()
        .map(compile_branch)
        .transpose()
        .map_err(|problem| field("branch", problem))?;
    Ok(Parsed { when, branch })
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

    #[test]
    fn no_branch_matches_every_branch() {
        let p = parsed(json!({ "repo": "a/b" })).unwrap();
        assert!(p.matches_branch("main"));
        assert!(p.matches_branch("feature/x/y"));
        assert_eq!(p.literal_branch(), None);
    }

    #[test]
    fn branch_globs_are_case_sensitive_and_cross_slashes() {
        let main = parsed(json!({ "repo": "a/b", "branch": "main" })).unwrap();
        assert!(main.matches_branch("main"));
        assert!(!main.matches_branch("Main"));
        assert!(!main.matches_branch("main2"));
        let release = parsed(json!({ "repo": "a/b", "branch": "release/*" })).unwrap();
        assert!(release.matches_branch("release/1.x"));
        assert!(release.matches_branch("release/1/x"));
        assert!(!release.matches_branch("hotfix/1"));
        let either = parsed(json!({ "repo": "a/b", "branch": "{main,trunk}" })).unwrap();
        assert!(either.matches_branch("trunk"));
        assert!(!either.matches_branch("dev"));
    }

    #[test]
    fn a_bad_or_empty_branch_names_its_field() {
        for bad in ["[", "{a", ""] {
            let err = parsed(json!({ "repo": "a/b", "branch": bad })).unwrap_err();
            assert_eq!(field_of(err), "branch", "{bad:?} should be rejected");
        }
    }

    #[test]
    fn literal_branch_is_only_set_without_glob_characters() {
        let literal = parsed(json!({ "repo": "a/b", "branch": "feature/login" })).unwrap();
        assert_eq!(literal.literal_branch(), Some("feature/login"));
        for glob in ["release/*", "v?", "[ab]", "{a,b}"] {
            let p = parsed(json!({ "repo": "a/b", "branch": glob })).unwrap();
            assert_eq!(p.literal_branch(), None, "{glob} is a glob");
        }
    }
}
```

2. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib` passes: 11 tests
- [ ] `cd src-tauri && cargo test -p trigger-github` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Using `matches_branch` or `literal_branch` anywhere (slice B builds the request paths and filters).
- Making the glob case insensitive or stopping `*` at `/`.
- Adding `globset` to `Cargo.toml` (T01 did).
- The README (T07).

## If stuck

- If a test fails, compare the code with step 1 character by character; do not change the test.
- If `is_none_or` is not found, the Rust toolchain is older than 1.82. Replace `.is_none_or(|glob| glob.is_match(name))` with `.map_or(true, |glob| glob.is_match(name))`, and say so in the Log.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
