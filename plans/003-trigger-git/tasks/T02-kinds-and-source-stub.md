---
id: T02
epic: 003-trigger-git
title: Add the eleven git kinds, a stub source and When
status: done
depends_on: [T01]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/lib.rs
  - src-tauri/crates/trigger-git/src/os.rs
  - src-tauri/crates/trigger-git/src/logic.rs
  - src-tauri/crates/trigger-git/src/rule.rs
  - src-tauri/crates/trigger-git/README.md
---

## Goal

`trigger-git` has its eleven real kinds with their `when` and payload fields, a `GitSource` whose `start` checks the rules and then reports `Unavailable`, a typed `When` with defaults, and a README Kinds table that matches.

## Read first

- `plans/003-trigger-git/epic.md` Decisions 2, 4, 5, 11 and the Shape section
- `src-tauri/crates/trigger-fs/src/lib.rs:1-60` how `KINDS`, `WHEN` and `PAYLOAD` are written
- `src-tauri/crates/trigger-core/src/lib.rs:100-125` `FieldInfo`, `KindInfo`
- `plans/_templates/trigger-crate/src/lib.rs` the file you replace

## Steps

1. Replace the whole of `src-tauri/crates/trigger-git/src/lib.rs` with this. It has the eleven kinds (Decisions 2, 4, 5 and 11) and a `start` that checks the rules but watches nothing yet; slice C finishes it. The doc strings are what the README table is built from, so keep them as written. `branch` is a glob matched against the branch in the event: the new branch for `git.branch-switched`, the branch left for `git.detached-head`.

```rust
//! Fires when something happens in a git repository. See README.md.
//!
//! Glue only: the kinds this crate fires, the `Source` implementation, and the
//! thread that feeds OS signals through the pure logic into the sink. Rules
//! live in `rule.rs`, decisions in `logic.rs`, the OS in `os.rs`.

mod logic;
mod os;
mod rule;

use trigger_core::{
    Availability, FieldInfo, Handle, KindInfo, Platforms, Rule, RuleError, Sink, Source,
    SourceError,
};

const REPO: FieldInfo = FieldInfo {
    name: "repo",
    doc: "Absolute path of the repository, usually ${repo.root}",
    required: true,
};

const BRANCH: FieldInfo = FieldInfo {
    name: "branch",
    doc: "Glob for the branch name, `*` does not cross /. Default: any branch",
    required: false,
};

const INTERVAL_S: FieldInfo = FieldInfo {
    name: "interval_s",
    doc: "Seconds between checks, at least 30. Default 300",
    required: false,
};

const FETCH: FieldInfo = FieldInfo {
    name: "fetch",
    doc: "Run git fetch before each check. Default false",
    required: false,
};

const OLDER_THAN_MIN: FieldInfo = FieldInfo {
    name: "older_than_min",
    doc: "Minutes the oldest uncommitted change must have waited. Default 60",
    required: false,
};

const FOLDER: FieldInfo = FieldInfo {
    name: "folder",
    doc: "Absolute path of the folder whose new children are watched",
    required: true,
};

const WHEN_REPO: &[FieldInfo] = &[REPO, BRANCH];
const WHEN_BEHIND: &[FieldInfo] = &[REPO, BRANCH, INTERVAL_S, FETCH];
const WHEN_UNCOMMITTED: &[FieldInfo] = &[REPO, BRANCH, INTERVAL_S, OLDER_THAN_MIN];
const WHEN_CLONED: &[FieldInfo] = &[FOLDER, INTERVAL_S];

const fn field(name: &'static str, doc: &'static str) -> FieldInfo {
    FieldInfo {
        name,
        doc,
        required: true,
    }
}

const P_REPO: FieldInfo = field("repo", "Absolute path of the repository");
const P_BRANCH: FieldInfo = field("branch", "Branch name, empty when HEAD is detached");
const P_COMMIT: FieldInfo = field("commit", "Short hash of HEAD");
const P_MESSAGE: FieldInfo = field("message", "First line of the commit message");

const PAYLOAD_COMMIT: &[FieldInfo] = &[P_REPO, P_BRANCH, P_COMMIT, P_MESSAGE];
const PAYLOAD_SWITCHED: &[FieldInfo] = &[
    P_REPO,
    P_BRANCH,
    P_COMMIT,
    field("from", "Branch switched away from"),
    field("to", "Branch switched to"),
];
const PAYLOAD_CONFLICT: &[FieldInfo] = &[
    P_REPO,
    P_BRANCH,
    field("files", "Number of files with conflicts"),
];
const PAYLOAD_BRANCH_COMMIT: &[FieldInfo] = &[P_REPO, P_BRANCH, P_COMMIT];
const PAYLOAD_TAG: &[FieldInfo] = &[P_REPO, P_BRANCH, field("tag", "Name of the new tag")];
const PAYLOAD_STASH: &[FieldInfo] = &[P_REPO, P_BRANCH];
const PAYLOAD_BEHIND: &[FieldInfo] = &[
    P_REPO,
    P_BRANCH,
    field("behind", "Commits the branch is behind its upstream"),
];
const PAYLOAD_UNCOMMITTED: &[FieldInfo] = &[
    P_REPO,
    P_BRANCH,
    field(
        "minutes",
        "Minutes the oldest uncommitted change has waited",
    ),
];
const PAYLOAD_CLONED: &[FieldInfo] = &[field("path", "Absolute path of the new repository")];

/// Every kind this crate fires. The ids are a contract with saved process
/// definitions: add kinds, never rename one.
pub static KINDS: &[KindInfo] = &[
    KindInfo {
        id: "git.commit",
        title: "Commit made",
        summary: "A commit was added to the current branch, or a cherry pick landed.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_COMMIT,
    },
    KindInfo {
        id: "git.branch-switched",
        title: "Branch switched",
        summary: "HEAD moved from one branch to another. The branch glob matches the new branch.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_SWITCHED,
    },
    KindInfo {
        id: "git.pulled",
        title: "Pulled",
        summary: "The current branch moved by a pull or a merge.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_COMMIT,
    },
    KindInfo {
        id: "git.merge-conflict",
        title: "Merge conflict",
        summary: "A merge stopped with conflicting files.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_CONFLICT,
    },
    KindInfo {
        id: "git.rebase-stopped",
        title: "Rebase stopped",
        summary: "A rebase stopped for a conflict or an edit.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_BRANCH_COMMIT,
    },
    KindInfo {
        id: "git.tag-created",
        title: "Tag created",
        summary: "A tag was created in the repository.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_TAG,
    },
    KindInfo {
        id: "git.stash-created",
        title: "Stash created",
        summary: "Work was stashed.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_STASH,
    },
    KindInfo {
        id: "git.detached-head",
        title: "HEAD detached",
        summary: "HEAD left a branch and points at a commit. The branch glob matches the branch left.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_BRANCH_COMMIT,
    },
    KindInfo {
        id: "git.behind-remote",
        title: "Behind remote",
        summary: "The branch fell behind its upstream. Checked on an interval.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_BEHIND,
        payload: PAYLOAD_BEHIND,
    },
    KindInfo {
        id: "git.uncommitted-older-than",
        title: "Uncommitted work is old",
        summary: "The oldest uncommitted change to a tracked file passed the age limit. Checked on an interval.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_UNCOMMITTED,
        payload: PAYLOAD_UNCOMMITTED,
    },
    KindInfo {
        id: "git.repo-cloned",
        title: "Repository cloned",
        summary: "A new repository appeared directly inside a folder. Checked on an interval.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_CLONED,
        payload: PAYLOAD_CLONED,
    },
];

pub struct GitSource;

impl Source for GitSource {
    fn id(&self) -> &'static str {
        "git"
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
        for r in &rules {
            rule::parse(r).map_err(|e| SourceError::Os(e.to_string()))?;
        }
        // The watcher and the thread are added by the glue card (slice C).
        let _ = sink;
        Err(SourceError::Unavailable("not implemented yet".into()))
    }
}
```

2. Replace the whole of `src-tauri/crates/trigger-git/src/os.rs` with this. Slice C adds the watcher; the template's `listen` is removed now because it names types that no longer exist.

```rust
//! The only file that talks to the operating system. Slice C adds the watcher
//! and the folder scan here; for now it only answers `availability`.

use trigger_core::Availability;

pub fn availability() -> Availability {
    Availability::Unsupported {
        reason: "not implemented yet".into(),
    }
}
```

3. Replace the whole of `src-tauri/crates/trigger-git/src/logic.rs` with this placeholder. The `#![allow(dead_code)]` is there because nothing reads the file's items until slice C wires them up; do not remove it.

```rust
//! Pure decisions: which changes a pair of repository snapshots shows. No git
//! calls, no threads, no clocks: everything comes in as arguments, so every
//! case is a unit test. Cards T04 to T08 fill it in.

// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]
```

4. Replace the whole of `src-tauri/crates/trigger-git/src/rule.rs` with this. It is the typed `when` object with its defaults and the kind check. T03 adds the per kind checks and the branch glob.

```rust
//! The `when` object of a rule, typed. Pure.

// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]

use std::path::PathBuf;

use serde::Deserialize;
use trigger_core::{Rule, RuleError};

pub const MIN_INTERVAL_S: u64 = 30;

fn default_interval_s() -> u64 {
    300
}

fn default_older_than_min() -> u64 {
    60
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct When {
    #[serde(default)]
    pub repo: Option<PathBuf>,
    #[serde(default)]
    pub folder: Option<PathBuf>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default = "default_interval_s")]
    pub interval_s: u64,
    #[serde(default = "default_older_than_min")]
    pub older_than_min: u64,
    #[serde(default)]
    pub fetch: bool,
}

/// Checks that the kind belongs to this crate and the fields parse. Used by
/// `check` (the editor) and by `start`.
pub fn parse(rule: &Rule) -> Result<When, RuleError> {
    if !crate::KINDS.iter().any(|k| k.id == rule.kind) {
        return Err(RuleError::UnknownKind(rule.kind.clone()));
    }
    let value = serde_json::Value::Object(rule.when.clone().into_iter().collect());
    serde_json::from_value(value).map_err(|e| RuleError::Field {
        field: "when".into(),
        problem: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use trigger_core::testkit::rule;

    #[test]
    fn defaults_are_filled_in() {
        let r = rule("p/t", "git.commit", json!({ "repo": "/r" }));
        let when = parse(&r).unwrap();
        assert_eq!(when.repo, Some(PathBuf::from("/r")));
        assert_eq!(when.folder, None);
        assert_eq!(when.branch, None);
        assert_eq!(when.interval_s, 300);
        assert_eq!(when.older_than_min, 60);
        assert!(!when.fetch);
    }

    #[test]
    fn rejects_unknown_fields_and_kinds() {
        let typo = rule("p/t", "git.commit", json!({ "repo": "/r", "brnach": "x" }));
        assert!(parse(&typo).is_err());
        let other = rule("p/t", "fs.file-saved", json!({ "repo": "/r" }));
        assert!(matches!(parse(&other), Err(RuleError::UnknownKind(_))));
    }
}
```

5. Run `cd src-tauri && cargo fmt -p trigger-git`. It should change nothing.

6. Refresh the README Kinds table: run `cd src-tauri && cargo run -q -p trigger-git --example readme`. It prints a header row, a separator row and eleven rows. In `src-tauri/crates/trigger-git/README.md` replace everything between the line `<!-- kinds -->` and the line `<!-- /kinds -->` (keep both marker lines) with that output. The first row reads `| git.commit | Commit made | macOS, Windows, Linux | none | repo*, branch | repo, branch, commit, message |`. The README keeps all its other text for now (T09 writes it).

7. Do not edit `tests/live.rs` or `examples/readme.rs`. `live.rs` still names `GitSource` and a `git.example` kind as a string; it compiles and is `#[ignore]`d. Slice C replaces it.

Note on `check-crates.mjs`: `check-crates.mjs` prints warnings, not errors, about template text. The ones it may print for this crate until slice C are: `README.md still has template text`, `src/lib.rs still has template code`, `src/os.rs still has template code`. Warnings are fine here; only errors fail the check. Do not try to remove them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes: 2 tests in the library, 1 in `readme`, `live` shows 1 ignored
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (see the note under Steps about the warnings it prints)
- [ ] `cd src-tauri && cargo run -q -p trigger-git --example readme | grep -c "^| git\."` prints `11`
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Per kind checks, absolute path checks, the interval minimum and the branch glob (T03).
- Any logic, `RepoSnapshot` or `step` (T04 onwards).
- A watcher, a thread, or running git (slices B and C). `start` ends in `Err(SourceError::Unavailable(...))`.
- Changing a kind id, a title, a doc string or a field name. They are decided in the epic.
- Writing README sections other than the Kinds table.
- Editing `tests/live.rs`.

## If stuck

- If `cargo clippy` warns about dead code in `lib.rs`, `os.rs` or `rule.rs`, the code differs from the steps. Re-copy it.
- If `cargo test` fails in `tests/readme.rs`, repeat step 6; the table must be the exact program output.
- If `cargo fmt -p trigger-git --check` fails, run `cd src-tauri && cargo fmt -p trigger-git` once.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (cargo test: 2 lib, 1 readme, 1 ignored; clippy and fmt --check exit 0; check-crates 0 errors, 3 expected template warnings; readme count 11)
- 2026-10-12 checker: pass
