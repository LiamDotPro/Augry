---
id: T21
epic: 001-trigger-core
title: Add the fs kinds, rule parsing and logic types
status: done
depends_on: [T20]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-fs/src/lib.rs
  - src-tauri/crates/trigger-fs/src/rule.rs
  - src-tauri/crates/trigger-fs/src/logic.rs
  - src-tauri/crates/trigger-fs/src/logic/tests.rs
  - src-tauri/crates/trigger-fs/src/os.rs
  - src-tauri/crates/trigger-fs/README.md
---

## Goal

`trigger-fs` has its five real kinds, `rule::parse` reads and validates the `when` object (including compiling the globs), and `logic.rs` has the types the state machine will use. `README.md` shows the real Kinds table.

## Read first

- `plans/001-trigger-core/epic.md` Decisions 6 to 8 (ignores, kinds, payload) and 22 (the state machine)
- `plans/001-trigger-core/research/R02-globset.md` why every glob is built the same way
- `src-tauri/crates/trigger-fs/src/lib.rs` the template file you replace
- `src-tauri/crates/trigger-core/src/lib.rs` `KindInfo`, `FieldInfo`, `Rule`, `RuleError`

## Steps

1. Replace the whole of `src-tauri/crates/trigger-fs/src/lib.rs` with this. It has the five kinds (Decision 7) and a `start` that parses and arms the rules but does not watch anything yet; card T27 finishes it. The doc strings in `KINDS` are what the README table is built from, so keep them as written.

```rust
//! Fires when files change under a watched folder. See README.md.
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

const WHEN: &[FieldInfo] = &[
    FieldInfo {
        name: "path",
        doc: "Folder to watch, absolute. Everything below it is watched.",
        required: true,
    },
    FieldInfo {
        name: "glob",
        doc: "Which files count, matched against the path below the folder. Default **/*",
        required: false,
    },
    FieldInfo {
        name: "ignore",
        doc: "Globs to skip, added to the built in ones (.git, node_modules, target, temp and partial download files)",
        required: false,
    },
    FieldInfo {
        name: "settle_ms",
        doc: "Milliseconds a file must be quiet before it counts as changed. Default 500",
        required: false,
    },
];

const PAYLOAD: &[FieldInfo] = &[
    FieldInfo {
        name: "path",
        doc: "Absolute path of the file",
        required: true,
    },
    FieldInfo {
        name: "name",
        doc: "File name",
        required: true,
    },
    FieldInfo {
        name: "relative",
        doc: "Path below the watched folder, with / separators",
        required: true,
    },
    FieldInfo {
        name: "ext",
        doc: "Extension without the dot, empty when there is none",
        required: true,
    },
];

const PAYLOAD_RENAMED: &[FieldInfo] = &[
    FieldInfo {
        name: "path",
        doc: "Absolute path of the file after the rename",
        required: true,
    },
    FieldInfo {
        name: "name",
        doc: "File name after the rename",
        required: true,
    },
    FieldInfo {
        name: "relative",
        doc: "Path below the watched folder, with / separators",
        required: true,
    },
    FieldInfo {
        name: "ext",
        doc: "Extension without the dot, empty when there is none",
        required: true,
    },
    FieldInfo {
        name: "from",
        doc: "Absolute path before the rename",
        required: true,
    },
];

/// Every kind this crate fires. The ids are a contract with saved process
/// definitions: add kinds, never rename one.
pub static KINDS: &[KindInfo] = &[
    KindInfo {
        id: "fs.file-saved",
        title: "File saved",
        summary: "A file that already existed was written to.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD,
    },
    KindInfo {
        id: "fs.file-created",
        title: "File created",
        summary: "A new file appeared.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD,
    },
    KindInfo {
        id: "fs.file-removed",
        title: "File removed",
        summary: "A file was deleted.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD,
    },
    KindInfo {
        id: "fs.file-renamed",
        title: "File renamed",
        summary: "A file was renamed or moved inside the watched folder.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD_RENAMED,
    },
    KindInfo {
        id: "fs.download-finished",
        title: "Download finished",
        summary: "A browser finished a download and the file got its final name.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD,
    },
];

pub struct FsSource;

impl Source for FsSource {
    fn id(&self) -> &'static str {
        "fs"
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
                rule::parse(r).map(|p| logic::Armed {
                    id: r.id.clone(),
                    kind: r.kind.clone(),
                    root: p.when.path.clone(),
                    when: p.when,
                    glob: p.glob,
                    ignore: p.ignore,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| SourceError::Os(e.to_string()))?;
        // The watcher and the thread are added by the glue card (T27).
        let _ = sink;
        Err(SourceError::Unavailable("not implemented yet".into()))
    }
}
```

2. Replace the whole of `src-tauri/crates/trigger-fs/src/rule.rs` with this. It reads the four `when` fields (Decision 7), compiles the globs once (Decision 6, R02: every glob is case insensitive and `*` does not cross `/`) and names the bad field in a `RuleError::Field`. The JSON key is `settle_ms`, with an underscore.

```rust
//! The `when` object of a rule, typed, with its globs compiled. Pure.

use std::path::PathBuf;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::Deserialize;
use trigger_core::{Rule, RuleError};

/// Always ignored, before the user's own `ignore` globs. Directory names come
/// in two forms so an event on the directory itself is ignored too.
pub const DEFAULT_IGNORES: &[&str] = &[
    "**/.git",
    "**/.git/**",
    "**/node_modules",
    "**/node_modules/**",
    "**/target",
    "**/target/**",
    "**/.DS_Store",
    "**/*.crdownload",
    "**/*.part",
    "**/*.download",
    "**/*.tmp",
    "**/*~",
    "**/*.swp",
    "**/*.swx",
];

fn default_glob() -> String {
    "**/*".to_string()
}

fn default_settle_ms() -> u64 {
    500
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct When {
    pub path: PathBuf,
    #[serde(default = "default_glob")]
    pub glob: String,
    #[serde(default)]
    pub ignore: Vec<String>,
    #[serde(default = "default_settle_ms")]
    pub settle_ms: u64,
}

/// A parsed rule: the typed fields plus the two compiled matchers. Both
/// matchers are applied to the path relative to `when.path`.
#[derive(Debug, Clone)]
pub struct Parsed {
    pub when: When,
    pub glob: GlobSet,
    pub ignore: GlobSet,
}

/// Case insensitive, `*` does not cross `/`. macOS and Windows file systems
/// are case insensitive, so `.PART` must match `*.part`.
fn build_set<'a>(patterns: impl Iterator<Item = &'a str>) -> Result<GlobSet, String> {
    let mut set = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = GlobBuilder::new(pattern)
            .literal_separator(true)
            .case_insensitive(true)
            .build()
            .map_err(|e| format!("{pattern}: {e}"))?;
        set.add(glob);
    }
    set.build().map_err(|e| e.to_string())
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
    if !when.path.is_absolute() {
        return Err(RuleError::Field {
            field: "path".into(),
            problem: "must be an absolute path".into(),
        });
    }
    let glob =
        build_set(std::iter::once(when.glob.as_str())).map_err(|problem| RuleError::Field {
            field: "glob".into(),
            problem,
        })?;
    let patterns = DEFAULT_IGNORES
        .iter()
        .copied()
        .chain(when.ignore.iter().map(String::as_str));
    let ignore = build_set(patterns).map_err(|problem| RuleError::Field {
        field: "ignore".into(),
        problem,
    })?;
    Ok(Parsed { when, glob, ignore })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;
    use trigger_core::testkit::rule;

    fn dir() -> String {
        std::env::temp_dir().to_string_lossy().into_owned()
    }

    fn parsed(when: serde_json::Value) -> Result<Parsed, RuleError> {
        parse(&rule("p/t", "fs.file-saved", when))
    }

    fn field_of(err: RuleError) -> String {
        match err {
            RuleError::Field { field, .. } => field,
            other => panic!("expected a field error, got {other:?}"),
        }
    }

    #[test]
    fn defaults_are_filled_in() {
        let p = parsed(json!({ "path": dir() })).unwrap();
        assert_eq!(p.when.glob, "**/*");
        assert!(p.when.ignore.is_empty());
        assert_eq!(p.when.settle_ms, 500);
    }

    #[test]
    fn all_fields_are_read() {
        let p = parsed(
            json!({ "path": dir(), "glob": "**/*.md", "ignore": ["drafts/**"], "settle_ms": 50 }),
        )
        .unwrap();
        assert_eq!(p.when.glob, "**/*.md");
        assert_eq!(p.when.ignore, vec!["drafts/**".to_string()]);
        assert_eq!(p.when.settle_ms, 50);
    }

    #[test]
    fn rejects_unknown_fields_kinds_and_missing_path() {
        let typo = parsed(json!({ "path": dir(), "globs": "x" }));
        assert_eq!(field_of(typo.unwrap_err()), "when");
        let missing = parsed(json!({ "glob": "**/*.md" }));
        assert_eq!(field_of(missing.unwrap_err()), "when");
        let other = parse(&rule("p/t", "git.commit", json!({ "path": dir() })));
        assert!(matches!(other, Err(RuleError::UnknownKind(_))));
    }

    #[test]
    fn rejects_a_relative_path() {
        let err = parsed(json!({ "path": "docs/notes" })).unwrap_err();
        assert_eq!(field_of(err), "path");
    }

    #[test]
    fn a_bad_glob_or_ignore_names_its_field() {
        let glob = parsed(json!({ "path": dir(), "glob": "[" })).unwrap_err();
        assert_eq!(field_of(glob), "glob");
        let ignore = parsed(json!({ "path": dir(), "ignore": ["{a"] })).unwrap_err();
        assert_eq!(field_of(ignore), "ignore");
    }

    #[test]
    fn glob_matches_relative_paths_without_crossing_separators() {
        let p = parsed(json!({ "path": dir(), "glob": "*.md" })).unwrap();
        assert!(p.glob.is_match(Path::new("a.md")));
        assert!(p.glob.is_match(Path::new("A.MD")));
        assert!(!p.glob.is_match(Path::new("docs/a.md")));
        let deep = parsed(json!({ "path": dir(), "glob": "**/*.md" })).unwrap();
        assert!(deep.glob.is_match(Path::new("docs/a.md")));
        assert!(deep.glob.is_match(Path::new("top.md")));
        assert!(!deep.glob.is_match(Path::new("docs/a.txt")));
    }

    #[test]
    fn default_ignores_cover_the_usual_noise() {
        let p = parsed(json!({ "path": dir() })).unwrap();
        for ignored in [
            ".git",
            "a/.git/HEAD",
            "node_modules/x/y.js",
            "src/target/x",
            ".DS_Store",
            "a/.DS_Store",
            "x.PART",
            "f.crdownload",
            "a/f.tmp",
            "notes.md~",
            "a/.f.swp",
        ] {
            assert!(
                p.ignore.is_match(Path::new(ignored)),
                "{ignored} should be ignored"
            );
        }
        for kept in ["docs/a.md", "a/targetx/f", ".gitignore", "a.txt"] {
            assert!(
                !p.ignore.is_match(Path::new(kept)),
                "{kept} should not be ignored"
            );
        }
    }

    #[test]
    fn user_ignores_are_added_to_the_defaults() {
        let p = parsed(json!({ "path": dir(), "ignore": ["drafts/**"] })).unwrap();
        assert!(p.ignore.is_match(Path::new("drafts/a.md")));
        assert!(p.ignore.is_match(Path::new("x.tmp")));
    }

    #[cfg(windows)]
    #[test]
    fn windows_separators_are_normalised_by_globset() {
        let p = parsed(json!({ "path": dir(), "glob": "**/*.md" })).unwrap();
        assert!(p.glob.is_match(Path::new(r"docs\a.md")));
    }
}
```

3. Replace the whole of `src-tauri/crates/trigger-fs/src/logic.rs` with this. It is the skeleton of the state machine: the types that cards T22 to T25 fill in, and a `step` that fires nothing yet. The `#![allow(dead_code)]` is there because nothing uses these types until T27 wires them up; do not remove it.

```rust
//! Pure decisions: given what the OS reported, which rules fire and with what
//! payload. No OS calls, no threads, no clocks: time and "is this a file"
//! come in as arguments, so every case is a unit test.

// Removed in T27, when lib.rs reads everything in this file.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use globset::GlobSet;
use trigger_core::{RuleId, Value};

use crate::rule::When;

/// A rule this crate is watching for. `root` is `when.path` made canonical.
#[derive(Debug, Clone)]
pub struct Armed {
    pub id: RuleId,
    pub kind: String,
    pub when: When,
    pub root: PathBuf,
    pub glob: GlobSet,
    pub ignore: GlobSet,
}

/// What `os.rs` makes of a `notify` event kind, so this file does not depend
/// on `notify`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawKind {
    Create,
    Remove,
    /// Data, metadata or unspecified change.
    Modify,
    /// One half of a rename (`Name(From)`, `Name(To)`, `Name(Any)`).
    Name,
    /// Both halves of a rename in one event, `paths` is `[from, to]` (Linux).
    Both,
    /// The OS lost events: forget what is pending and scan again.
    Rescan,
    /// Access events and anything else that never changes a file.
    Ignore,
}

/// Time is a number of milliseconds on any clock that only goes forward. The
/// caller reads the clock; nothing in here does.
#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Raw {
        at: u64,
        kind: RawKind,
        paths: Vec<PathBuf>,
    },
    Tick {
        now: u64,
    },
}

/// Everything remembered between signals for one rule.
#[derive(Debug, Default)]
pub struct State {}

pub type Payload = BTreeMap<String, Value>;

/// The events a signal fires for this rule: (kind, payload). `is_file` says
/// whether a path is an existing regular file.
pub fn step(
    _state: &mut State,
    _armed: &Armed,
    _signal: Signal,
    _is_file: &dyn Fn(&Path) -> bool,
) -> Vec<(String, Payload)> {
    Vec::new()
}

#[cfg(test)]
mod tests;
```

4. Create the folder `src-tauri/crates/trigger-fs/src/logic/` and in it the file `tests.rs` with this content. (`logic.rs` ends with `mod tests;`, which loads it. Tests live in their own file so no file grows past the 400 line limit of `check-crates.mjs`.)

```rust
use super::*;
use serde_json::json;
use trigger_core::testkit::rule;

#[test]
fn the_empty_state_machine_fires_nothing() {
    let root = std::env::temp_dir();
    let parsed =
        crate::rule::parse(&rule("p/t", "fs.file-saved", json!({ "path": root }))).unwrap();
    let armed = Armed {
        id: "p/t".into(),
        kind: "fs.file-saved".into(),
        root: root.clone(),
        when: parsed.when,
        glob: parsed.glob,
        ignore: parsed.ignore,
    };
    let mut state = State::default();
    let out = step(&mut state, &armed, Signal::Tick { now: 1000 }, &|_| false);
    assert!(out.is_empty());
}
```

5. In `src-tauri/crates/trigger-fs/src/os.rs`, directly after the last `//!` line at the top of the file (the one ending `behind the same two functions.`), add a blank line and then these two lines. Change nothing else in the file.

```rust
// Removed in T27, when lib.rs uses everything in this file.
#![allow(dead_code)]
```

6. Run `cd src-tauri && cargo fmt -p trigger-fs`. It re-wraps `os.rs`; that is expected.

7. Refresh the README Kinds table: run `cd src-tauri && cargo run -p trigger-fs --example readme` and replace everything between the `<!-- kinds -->` and `<!-- /kinds -->` lines of `src-tauri/crates/trigger-fs/README.md` with the output (keep the two marker lines). The output has a header row, a separator row and five rows, `fs.file-saved`, `fs.file-created`, `fs.file-removed`, `fs.file-renamed`, `fs.download-finished`. The first row reads `| fs.file-saved | File saved | macOS, Windows, Linux | none | path*, glob, ignore, settle_ms | path, name, relative, ext |`. The README's other sections stay as they are: card T29 writes them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-fs --lib` passes: 9 tests on macOS and Linux (10 on Windows)
- [ ] `cd src-tauri && cargo test -p trigger-fs` passes (this includes the README table test)
- [ ] `cd src-tauri && cargo run -q -p trigger-fs --example readme | grep -c "^| fs\."` prints `5`
- [ ] `cd src-tauri && cargo clippy -p trigger-fs --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-fs --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Any logic: `step` stays a stub that returns an empty list (T22 to T25).
- Starting a watcher or a thread in `start`. It still ends in `Err(SourceError::Unavailable(...))`; T27 replaces it.
- The OS code in `os.rs` beyond the one added line (T26, T27).
- Changing a default ignore, a default value, a doc string or a kind id. They are decided in the epic.
- Writing README sections other than the Kinds table (T29).
- Splitting `KINDS` or `rule.rs` into more files.

## If stuck

- If a test in `rule.rs` fails, compare the code with step 2 character by character; do not change the test.
- If `cargo clippy` warns about dead code in `rule.rs` or `lib.rs`, the code differs from the steps. Re-copy it.
- If `cargo test` fails in `tests/readme.rs`, repeat step 7; the table must be the exact program output.
- If `trigger_core::testkit::rule` or `RuleError::Field` does not exist, T06 or T04 did not land. Set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
