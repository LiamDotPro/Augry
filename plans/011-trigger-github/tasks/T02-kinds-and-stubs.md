---
id: T02
epic: 011-trigger-github
title: Add the eight GitHub kinds and stub the rest around them
status: done
depends_on: [T01]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-github/src/lib.rs
  - src-tauri/crates/trigger-github/src/rule.rs
  - src-tauri/crates/trigger-github/src/logic.rs
  - src-tauri/crates/trigger-github/src/os.rs
  - src-tauri/crates/trigger-github/tests/live.rs
  - src-tauri/crates/trigger-github/README.md
---

## Goal

`trigger-github` has its eight real kinds with their `when` and payload fields, `GithubSource` checks rules (kind and unknown fields only so far) and `start` parses them but does not poll yet. The README shows the real Kinds table. The template's example code is gone, so the crate still builds, tests and lints clean.

## Read first

- `plans/011-trigger-github/epic.md` Decisions 2, 3 and 13 (kinds, `when`, payload)
- `plans/011-trigger-github/research/R01-endpoints.md` the "Implications for the cards" table: where each payload field comes from
- `src-tauri/crates/trigger-github/src/lib.rs` the template file you replace
- `src-tauri/crates/trigger-core/src/lib.rs` `KindInfo`, `FieldInfo`, `Rule`, `RuleError`

## Steps

1. Replace the whole of `src-tauri/crates/trigger-github/src/lib.rs` with this. It has the eight kinds (Decision 2) and a `start` that parses and arms the rules but does not poll yet; slice C finishes it. The doc strings in `KINDS` are what the README table is built from, so keep them as written. The two `const fn` helpers keep each field to one line; payload field names are flat (`author` is `user.login`, `base` is `base.ref`) and slice B builds them from the response. The `branch` field has two docs because it means the run's branch on CI kinds and the target branch on `github.pr-merged`.

```rust
//! Fires on GitHub issues, reviews, CI runs, pull requests and releases for the signed in user. See README.md.
//!
//! Glue only: the kinds this crate fires, the `Source` implementation, and the
//! thread that feeds `gh` answers through the pure logic into the sink. Rules
//! live in `rule.rs`, decisions in `logic.rs`, the OS in `os.rs`.

mod logic;
mod os;
mod rule;

use trigger_core::{
    Availability, FieldInfo, Handle, KindInfo, Platforms, Rule, RuleError, Sink, Source,
    SourceError,
};

const fn req(name: &'static str, doc: &'static str) -> FieldInfo {
    FieldInfo {
        name,
        doc,
        required: true,
    }
}

const fn opt(name: &'static str, doc: &'static str) -> FieldInfo {
    FieldInfo {
        name,
        doc,
        required: false,
    }
}

const REPO: FieldInfo = req(
    "repo",
    "Repository as owner/name, such as cli/cli. Use ${repo.remote} for the repository of the pane's origin",
);
const INTERVAL: FieldInfo = opt(
    "interval_s",
    "Seconds between checks. Default 120, never below 60",
);
const BRANCH_CI: FieldInfo = opt(
    "branch",
    "Glob for the branch the run was on. Default every branch",
);
const BRANCH_BASE: FieldInfo = opt(
    "branch",
    "Glob for the branch the pull request was merged into. Default every branch",
);
const AUTHOR: FieldInfo = opt(
    "author",
    "Logins whose pull requests count, as a list. Default dependabot[bot] and renovate[bot]",
);

const WHEN_REPO: &[FieldInfo] = &[REPO, INTERVAL];
const WHEN_CI: &[FieldInfo] = &[REPO, BRANCH_CI, INTERVAL];
const WHEN_MERGED: &[FieldInfo] = &[REPO, BRANCH_BASE, INTERVAL];
const WHEN_DEPENDENCY: &[FieldInfo] = &[REPO, AUTHOR, INTERVAL];

const PAYLOAD_ISSUE: &[FieldInfo] = &[
    req("number", "Issue number"),
    req("title", "Issue title"),
    req("url", "Web address of the issue"),
    req("author", "Login of whoever opened it"),
    req("assignees", "Logins of everyone assigned, comma separated"),
];

const PAYLOAD_REVIEW: &[FieldInfo] = &[
    req("number", "Pull request number"),
    req("title", "Pull request title"),
    req("url", "Web address of the pull request"),
    req("author", "Login of whoever opened it"),
    req("head", "Branch the changes are on"),
    req("base", "Branch the changes would merge into"),
];

const PAYLOAD_CI: &[FieldInfo] = &[
    req("id", "Workflow run id"),
    req("name", "Workflow name"),
    req("branch", "Branch the run was on"),
    req("sha", "Commit the run was on"),
    req(
        "conclusion",
        "How the run ended, such as failure, timed_out or success",
    ),
    req("url", "Web address of the run"),
    req(
        "event",
        "What started the run, such as push or pull_request",
    ),
    req("actor", "Login of whoever triggered the run"),
    req("updated_at", "When the run finished, ISO 8601"),
];

const PAYLOAD_COMMENT: &[FieldInfo] = &[
    req("id", "Comment id"),
    req("url", "Web address of the comment"),
    req("author", "Login of whoever commented"),
    req("body", "The comment text, cut to 200 characters"),
    req("created_at", "When it was written, ISO 8601"),
];

const PAYLOAD_MERGED: &[FieldInfo] = &[
    req("number", "Pull request number"),
    req("title", "Pull request title"),
    req("url", "Web address of the pull request"),
    req("author", "Login of whoever opened it"),
    req("merged_at", "When it was merged, ISO 8601"),
    req("base", "Branch it was merged into"),
];

const PAYLOAD_RELEASE: &[FieldInfo] = &[
    req("tag", "Tag name of the release"),
    req("name", "Release title"),
    req("prerelease", "true for a pre-release, otherwise false"),
    req("url", "Web address of the release"),
    req("published_at", "When it was published, ISO 8601"),
    req("author", "Login of whoever published it"),
];

const PAYLOAD_DEPENDENCY: &[FieldInfo] = &[
    req("number", "Pull request number"),
    req("title", "Pull request title"),
    req("url", "Web address of the pull request"),
    req("author", "Login of the bot that opened it"),
    req("base", "Branch it targets"),
];

/// Every kind this crate fires. The ids are a contract with saved process
/// definitions: add kinds, never rename one.
pub static KINDS: &[KindInfo] = &[
    KindInfo {
        id: "github.issue-assigned",
        title: "Issue assigned",
        summary: "An open issue was assigned to you.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_ISSUE,
    },
    KindInfo {
        id: "github.review-requested",
        title: "Review requested",
        summary: "Someone asked you to review a pull request.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_REVIEW,
    },
    KindInfo {
        id: "github.ci-failed",
        title: "CI failed",
        summary: "A workflow run finished with a failure on a matching branch.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_CI,
        payload: PAYLOAD_CI,
    },
    KindInfo {
        id: "github.ci-passed",
        title: "CI passed",
        summary: "A workflow run finished successfully on a matching branch.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_CI,
        payload: PAYLOAD_CI,
    },
    KindInfo {
        id: "github.pr-commented",
        title: "Pull request commented",
        summary: "Someone else commented on a pull request.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_COMMENT,
    },
    KindInfo {
        id: "github.pr-merged",
        title: "Pull request merged",
        summary: "A pull request was merged into a matching branch.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_MERGED,
        payload: PAYLOAD_MERGED,
    },
    KindInfo {
        id: "github.release-published",
        title: "Release published",
        summary: "A new release was published.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_RELEASE,
    },
    KindInfo {
        id: "github.dependency-pr-opened",
        title: "Dependency update opened",
        summary: "Dependabot or Renovate opened a pull request.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_DEPENDENCY,
        payload: PAYLOAD_DEPENDENCY,
    },
];

pub struct GithubSource;

impl Source for GithubSource {
    fn id(&self) -> &'static str {
        "github"
    }

    fn kinds(&self) -> &'static [KindInfo] {
        KINDS
    }

    fn availability(&self) -> Availability {
        os::availability()
    }

    fn check(&self, rule: &Rule) -> Result<(), RuleError> {
        rule::parse(rule).map(|_| ())
    }

    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError> {
        let _armed = rules
            .iter()
            .map(|r| {
                rule::parse(r).map(|parsed| logic::Armed {
                    id: r.id.clone(),
                    kind: r.kind.clone(),
                    parsed,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| SourceError::Os(e.to_string()))?;
        // The poll thread is added by slice C.
        let _ = sink;
        Err(SourceError::Unavailable("not implemented yet".into()))
    }
}
```

2. Replace the whole of `src-tauri/crates/trigger-github/src/rule.rs` with this. It only knows `repo` so far; T03 to T06 add the rest. The `#![allow(dead_code)]` is there because nothing reads these types until slice C; do not remove it.

```rust
//! The `when` object of a rule, typed. Pure.

// Removed in slice C, when the poll loop reads everything in this file.
#![allow(dead_code)]

use serde::Deserialize;
use trigger_core::{Rule, RuleError};

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct When {
    pub repo: String,
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

    fn field_of(err: RuleError) -> String {
        match err {
            RuleError::Field { field, .. } => field,
            other => panic!("expected a field error, got {other:?}"),
        }
    }

    #[test]
    fn parses_the_repo() {
        let r = rule("p/t", "github.issue-assigned", json!({ "repo": "cli/cli" }));
        assert_eq!(parse(&r).unwrap().when.repo, "cli/cli");
    }

    #[test]
    fn every_kind_is_accepted() {
        for kind in crate::KINDS {
            let r = rule("p/t", kind.id, json!({ "repo": "cli/cli" }));
            assert!(parse(&r).is_ok(), "{} should parse", kind.id);
        }
    }

    #[test]
    fn rejects_unknown_fields_missing_repo_and_other_kinds() {
        let typo = rule(
            "p/t",
            "github.ci-failed",
            json!({ "repo": "a/b", "brach": "x" }),
        );
        assert_eq!(field_of(parse(&typo).unwrap_err()), "when");
        let missing = rule("p/t", "github.ci-failed", json!({}));
        assert_eq!(field_of(parse(&missing).unwrap_err()), "when");
        let other = rule("p/t", "fs.file-saved", json!({ "repo": "a/b" }));
        assert!(matches!(parse(&other), Err(RuleError::UnknownKind(_))));
    }
}
```

3. Replace the whole of `src-tauri/crates/trigger-github/src/logic.rs` with this. It is only the types the later logic needs.

```rust
//! Pure decisions: given what `gh` answered, which rules fire and with what
//! payload. No child processes, no threads, no clocks: everything it needs
//! comes in as arguments, so every case is a unit test. Slice B fills it in.

// Removed in slice C, when the poll loop reads everything in this file.
#![allow(dead_code)]

use trigger_core::RuleId;

use crate::rule::Parsed;

/// A rule this crate is polling for.
#[derive(Debug, Clone)]
pub struct Armed {
    pub id: RuleId,
    pub kind: String,
    pub parsed: Parsed,
}

/// Everything remembered between polls (seen ids, etags). Filled in by slice B.
#[derive(Debug, Default)]
pub struct State {}
```

4. Replace the whole of `src-tauri/crates/trigger-github/src/os.rs` with this. Slice C replaces it with the code that finds and runs `gh`.

```rust
//! The only file that talks to the operating system: finds `gh` and runs it.
//! Slice C fills it in.

use trigger_core::Availability;

pub fn availability() -> Availability {
    Availability::Unsupported {
        reason: "not implemented yet".into(),
    }
}
```

5. Replace the whole of `src-tauri/crates/trigger-github/tests/live.rs` with this. It is still only a placeholder (it is `#[ignore]`d and `start` is a stub); slice C writes the real live tests.

```rust
//! Tests against the real GitHub, through the user's own `gh` login. Not run by
//! `cargo test`; run them on a machine where `gh auth login` was done with:
//!
//!     cargo test -p trigger-github -- --ignored
//!
//! Each test arms a rule, waits for the event with `RecordingSink::wait_for`,
//! and reads GitHub only. Keep them few and generous with time.

use std::time::Duration;

use trigger_core::testkit::{rule, RecordingSink};
use trigger_core::Source;

#[test]
#[ignore]
fn fires_for_the_real_thing() {
    let sink = RecordingSink::new();
    let source = trigger_github::GithubSource;
    let _handle = source
        .start(
            vec![rule(
                "p/t",
                "github.issue-assigned",
                serde_json::json!({ "repo": "cli/cli" }),
            )],
            sink.tx.clone(),
        )
        .expect("start");
    assert!(sink
        .wait_for("github.issue-assigned", Duration::from_secs(5))
        .is_some());
}
```

6. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing; if it re-wraps a line, that is fine.

7. Refresh the README Kinds table: run `cd src-tauri && cargo run -q -p trigger-github --example readme`. In `src-tauri/crates/trigger-github/README.md` replace everything between the line `<!-- kinds -->` and the line `<!-- /kinds -->` (keep both marker lines) with that output. It is this, a header row, a separator row and eight kind rows:

```
| Kind | Title | Platforms | Permission | When | Payload |
| --- | --- | --- | --- | --- | --- |
| github.issue-assigned | Issue assigned | macOS, Windows, Linux | none | repo*, interval_s | number, title, url, author, assignees |
| github.review-requested | Review requested | macOS, Windows, Linux | none | repo*, interval_s | number, title, url, author, head, base |
| github.ci-failed | CI failed | macOS, Windows, Linux | none | repo*, branch, interval_s | id, name, branch, sha, conclusion, url, event, actor, updated_at |
| github.ci-passed | CI passed | macOS, Windows, Linux | none | repo*, branch, interval_s | id, name, branch, sha, conclusion, url, event, actor, updated_at |
| github.pr-commented | Pull request commented | macOS, Windows, Linux | none | repo*, interval_s | id, url, author, body, created_at |
| github.pr-merged | Pull request merged | macOS, Windows, Linux | none | repo*, branch, interval_s | number, title, url, author, merged_at, base |
| github.release-published | Release published | macOS, Windows, Linux | none | repo*, interval_s | tag, name, prerelease, url, published_at, author |
| github.dependency-pr-opened | Dependency update opened | macOS, Windows, Linux | none | repo*, author, interval_s | number, title, url, author, base |
```

The README keeps all its other text for now: T07 writes the Rule fields, Payload and Example sections; slice C writes the rest.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib` passes: 3 tests
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (this includes the README table test; `live` shows 1 ignored)
- [ ] `cd src-tauri && cargo run -q -p trigger-github --example readme | grep -c "^| github\."` prints `8`
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (warnings that `lib.rs` and `os.rs` still have template code, and the README still has template text, are expected; later slices clear them)
- [ ] `git status --porcelain` lists only paths under `files:` (and `src-tauri/Cargo.lock` must not appear)

## Out of scope

- Any rule validation beyond what the code above does (T03 to T06).
- Starting a thread or running `gh`. `start` still ends in `Err(SourceError::Unavailable(...))`; slice C replaces it.
- Changing a kind id, a title, a doc string or a field name. They are decided in the epic and R01.
- Splitting `KINDS` into another file. `lib.rs` stays under 400 lines.
- Writing README sections other than the Kinds table.
- Registering the crate in the app.

## If stuck

- If a test in `rule.rs` fails, compare the code with step 2 character by character; do not change the test.
- If `cargo clippy` warns about dead code in `rule.rs`, `logic.rs` or `lib.rs`, the code differs from the steps. Re-copy it.
- If `cargo test` fails in `tests/readme.rs`, repeat step 7; the table must be the exact program output.
- If `trigger_core::testkit::rule` or `RuleError::Field` does not exist, the core crate is not as expected. Set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (lib --lib 3 tests, all 3 tests + readme + 1 ignored, example grep 8, clippy 0, fmt check 0, check-crates 0 with expected template warnings, git status limited to files:)
- 2026-10-12 checker: pass
