---
id: T02
epic: 010-trigger-docker
title: Add the five kinds, rule parsing and the logic skeleton
status: done
depends_on: [T01]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-docker/src/lib.rs
  - src-tauri/crates/trigger-docker/src/rule.rs
  - src-tauri/crates/trigger-docker/src/logic.rs
  - src-tauri/crates/trigger-docker/src/logic/tests.rs
  - src-tauri/crates/trigger-docker/src/os.rs
  - src-tauri/crates/trigger-docker/README.md
---

## Goal

`trigger-docker` has its five real kinds, `rule::parse` reads and validates the `when` object (compiling the globs and checking `label` and `engine`), and `logic.rs` has the `Armed` type. `os.rs` is a stub. `README.md` shows the real Kinds table, the Rule fields and a real Example.

## Read first

- `plans/010-trigger-docker/epic.md` Decisions 2, 3 and 10 (kinds, `when`, payload)
- `src-tauri/crates/trigger-fs/src/lib.rs:1-60` the model for `KINDS` written with shared `FieldInfo` constants
- `src-tauri/crates/trigger-fs/src/rule.rs` the model for `parse` and for how tests build rules
- `src-tauri/crates/trigger-core/src/lib.rs:100-170` `KindInfo`, `FieldInfo`, `Rule`, `RuleError`

## Steps

1. Replace the whole of `src-tauri/crates/trigger-docker/src/lib.rs` with this. It has the five kinds (Decision 2) and a `start` that parses and arms the rules but does not run anything yet; slice C finishes it. The doc strings are what the README table and the editor show, so keep them as written. The payload lists are per kind: image-built has no `container_id`, only stopped has `exit_code`, only the two health kinds have `health`.

```rust
//! Fires on container and image events from Docker and Podman. See README.md.
//!
//! Glue only: the kinds this crate fires, the `Source` implementation, and the
//! thread that feeds engine events through the pure logic into the sink. Rules
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
        name: "name",
        doc: "Glob on the container name, or on the image reference for image-built",
        required: false,
    },
    FieldInfo {
        name: "image",
        doc: "Glob on the image. docker.io/library/ and :latest are optional",
        required: false,
    },
    FieldInfo {
        name: "label",
        doc: "A label the container or image must have, as key=value",
        required: false,
    },
    FieldInfo {
        name: "engine",
        doc: "docker, podman or auto. Default auto",
        required: false,
    },
];

const ENGINE: FieldInfo = FieldInfo {
    name: "engine",
    doc: "docker or podman",
    required: true,
};
const CONTAINER_ID: FieldInfo = FieldInfo {
    name: "container_id",
    doc: "First 12 characters of the container id",
    required: true,
};
const NAME: FieldInfo = FieldInfo {
    name: "name",
    doc: "Container name, or the image reference for image-built",
    required: true,
};
const IMAGE: FieldInfo = FieldInfo {
    name: "image",
    doc: "Image the container runs, as the engine reports it",
    required: true,
};
const TIME: FieldInfo = FieldInfo {
    name: "time",
    doc: "When the engine reported it, in milliseconds since 1970",
    required: true,
};
const EXIT_CODE: FieldInfo = FieldInfo {
    name: "exit_code",
    doc: "Exit code of the main process, -1 when the engine did not report one",
    required: true,
};
const HEALTH: FieldInfo = FieldInfo {
    name: "health",
    doc: "healthy or unhealthy",
    required: true,
};

const PAYLOAD: &[FieldInfo] = &[ENGINE, CONTAINER_ID, NAME, IMAGE, TIME];
const PAYLOAD_STOPPED: &[FieldInfo] = &[ENGINE, CONTAINER_ID, NAME, IMAGE, TIME, EXIT_CODE];
const PAYLOAD_HEALTH: &[FieldInfo] = &[ENGINE, CONTAINER_ID, NAME, IMAGE, TIME, HEALTH];
const PAYLOAD_IMAGE: &[FieldInfo] = &[ENGINE, NAME, IMAGE, TIME];

/// Every kind this crate fires. The ids are a contract with saved process
/// definitions: add kinds, never rename one.
pub static KINDS: &[KindInfo] = &[
    KindInfo {
        id: "docker.container-started",
        title: "Container started",
        summary: "A container started.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD,
    },
    KindInfo {
        id: "docker.container-stopped",
        title: "Container stopped",
        summary: "A container's main process ended, by itself or because it was stopped.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD_STOPPED,
    },
    KindInfo {
        id: "docker.container-healthy",
        title: "Container healthy",
        summary: "A container's health check started passing.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD_HEALTH,
    },
    KindInfo {
        id: "docker.container-unhealthy",
        title: "Container unhealthy",
        summary: "A container's health check started failing.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD_HEALTH,
    },
    KindInfo {
        id: "docker.image-built",
        title: "Image built",
        summary: "An image was built and tagged. A manual tag also counts.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD_IMAGE,
    },
];

pub struct DockerSource;

impl Source for DockerSource {
    fn id(&self) -> &'static str {
        "docker"
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
                rule::parse(r).map(|when| logic::Armed {
                    id: r.id.clone(),
                    kind: r.kind.clone(),
                    when,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| SourceError::Os(e.to_string()))?;
        // The engine child process and the thread are added by the glue card
        // in slice C.
        let _ = sink;
        Err(SourceError::Unavailable("not implemented yet".into()))
    }
}
```

2. Replace the whole of `src-tauri/crates/trigger-docker/src/rule.rs` with this. `label` is split at the first `=` and must have a non empty key. `engine` is one of `docker`, `podman`, `auto` and defaults to `auto`. Globs are case sensitive and `*` crosses `/`, because image references contain slashes.

```rust
//! The `when` object of a rule, typed, with its globs compiled. Pure.

// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]

use globset::{GlobBuilder, GlobMatcher};
use serde::Deserialize;
use trigger_core::{Rule, RuleError};

/// Which engine a rule listens to. `Auto` means whichever answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineChoice {
    Docker,
    Podman,
    #[default]
    Auto,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct When {
    pub name: Option<String>,
    pub image: Option<String>,
    pub label: Option<String>,
    #[serde(default)]
    pub engine: EngineChoice,
}

/// A parsed rule: the typed fields plus the compiled matchers.
#[derive(Debug, Clone)]
pub struct Parsed {
    pub when: When,
    pub name: Option<GlobMatcher>,
    pub image: Option<GlobMatcher>,
    /// The label split at the first `=`.
    pub label: Option<(String, String)>,
}

/// `*` crosses `/` because image references have slashes. Case sensitive:
/// container names and image references are.
fn build_glob(pattern: &str) -> Result<GlobMatcher, String> {
    let glob = GlobBuilder::new(pattern)
        .literal_separator(false)
        .build()
        .map_err(|e| format!("{pattern}: {e}"))?;
    Ok(glob.compile_matcher())
}

fn field_error(field: &str, problem: impl Into<String>) -> RuleError {
    RuleError::Field {
        field: field.into(),
        problem: problem.into(),
    }
}

/// Checks that the kind belongs to this crate and the fields parse. Used by
/// `check` (the editor) and by `start`.
pub fn parse(rule: &Rule) -> Result<Parsed, RuleError> {
    if !crate::KINDS.iter().any(|k| k.id == rule.kind) {
        return Err(RuleError::UnknownKind(rule.kind.clone()));
    }
    let value = serde_json::Value::Object(rule.when.clone().into_iter().collect());
    let when: When =
        serde_json::from_value(value).map_err(|e| field_error("when", e.to_string()))?;
    let name = match &when.name {
        Some(p) => Some(build_glob(p).map_err(|e| field_error("name", e))?),
        None => None,
    };
    let image = match &when.image {
        Some(p) => Some(build_glob(p).map_err(|e| field_error("image", e))?),
        None => None,
    };
    let label = match &when.label {
        Some(text) => match text.split_once('=') {
            Some((key, value)) if !key.trim().is_empty() => {
                Some((key.trim().to_string(), value.trim().to_string()))
            }
            _ => return Err(field_error("label", "must be key=value")),
        },
        None => None,
    };
    Ok(Parsed {
        when,
        name,
        image,
        label,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use trigger_core::testkit::rule;

    fn parsed(when: serde_json::Value) -> Result<Parsed, RuleError> {
        parse(&rule("p/t", "docker.container-started", when))
    }

    fn field_of(err: RuleError) -> String {
        match err {
            RuleError::Field { field, .. } => field,
            other => panic!("expected a field error, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_when_is_valid_and_defaults_to_auto() {
        let p = parsed(json!({})).unwrap();
        assert_eq!(p.when.engine, EngineChoice::Auto);
        assert!(p.name.is_none() && p.image.is_none() && p.label.is_none());
    }

    #[test]
    fn all_fields_are_read() {
        let p = parsed(
            json!({ "name": "web-*", "image": "nginx*", "label": "com.example.team=x", "engine": "podman" }),
        )
        .unwrap();
        assert_eq!(p.when.engine, EngineChoice::Podman);
        assert!(p.name.unwrap().is_match("web-1"));
        assert!(p.image.unwrap().is_match("nginx:1.27"));
        assert_eq!(
            p.label,
            Some(("com.example.team".to_string(), "x".to_string()))
        );
    }

    #[test]
    fn the_engine_is_one_of_three_words() {
        for ok in ["docker", "podman", "auto"] {
            assert!(parsed(json!({ "engine": ok })).is_ok(), "{ok}");
        }
        let bad = parsed(json!({ "engine": "lxc" })).unwrap_err();
        assert_eq!(field_of(bad), "when");
    }

    #[test]
    fn a_label_must_be_key_equals_value() {
        assert!(parsed(json!({ "label": "a=" })).is_ok());
        for bad in ["team", "=x", " =x", ""] {
            let err = parsed(json!({ "label": bad })).unwrap_err();
            assert_eq!(field_of(err), "label", "{bad}");
        }
    }

    #[test]
    fn a_label_value_may_contain_equals() {
        let p = parsed(json!({ "label": "a=b=c" })).unwrap();
        assert_eq!(p.label, Some(("a".to_string(), "b=c".to_string())));
    }

    #[test]
    fn a_bad_glob_names_its_field() {
        let name = parsed(json!({ "name": "[" })).unwrap_err();
        assert_eq!(field_of(name), "name");
        let image = parsed(json!({ "image": "{a" })).unwrap_err();
        assert_eq!(field_of(image), "image");
    }

    #[test]
    fn globs_cross_slashes_and_are_case_sensitive() {
        let p = parsed(json!({ "image": "myorg/*" })).unwrap();
        let m = p.image.unwrap();
        assert!(m.is_match("myorg/app/api:1"));
        assert!(!m.is_match("MyOrg/app"));
    }

    #[test]
    fn rejects_unknown_fields_and_kinds() {
        let typo = parsed(json!({ "imge": "x" })).unwrap_err();
        assert_eq!(field_of(typo), "when");
        let other = parse(&rule("p/t", "fs.file-saved", json!({})));
        assert!(matches!(other, Err(RuleError::UnknownKind(_))));
    }
}
```

3. Replace the whole of `src-tauri/crates/trigger-docker/src/logic.rs` with this. It is only the `Armed` type for now; the cards after this one add the parser, the matcher and the payload. The `#![allow(dead_code)]` is there because lib.rs does not read these items until slice C; do not remove it.

```rust
//! Pure decisions: given an event the engine printed, which rules fire and with
//! what payload. No child processes, no threads, no clocks: every case is a
//! unit test.

// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]

use std::collections::BTreeMap;

use trigger_core::{RuleId, Value};

use crate::rule::Parsed;

/// A rule this crate is watching for.
#[derive(Debug, Clone)]
pub struct Armed {
    pub id: RuleId,
    pub kind: String,
    pub when: Parsed,
}

pub type Payload = BTreeMap<String, Value>;

#[cfg(test)]
mod tests;
```

4. Create the folder `src-tauri/crates/trigger-docker/src/logic/` and in it the file `tests.rs` with this content. (`logic.rs` ends with `mod tests;`, which loads it. Tests live beside the code in their own files so no file passes the 400 line limit of `check-crates.mjs`.)

```rust
use super::*;
use serde_json::json;
use trigger_core::testkit::rule;

#[test]
fn an_armed_rule_holds_its_parsed_when() {
    let r = rule(
        "p/t",
        "docker.container-started",
        json!({ "name": "web-*" }),
    );
    let parsed = crate::rule::parse(&r).unwrap();
    let armed = Armed {
        id: r.id.clone(),
        kind: r.kind.clone(),
        when: parsed,
    };
    assert_eq!(armed.kind, "docker.container-started");
    assert!(armed.when.name.is_some());
}
```

5. Replace the whole of `src-tauri/crates/trigger-docker/src/os.rs` with this stub. The template version refers to types that no longer exist.

```rust
//! The only file that talks to the operating system. Slice C of the epic
//! replaces this stub with `find_engine`, the version check and the real
//! engine; everything else is in `logic.rs`.

// Removed when lib.rs uses everything in this file.
#![allow(dead_code)]

use trigger_core::Availability;

pub fn availability() -> Availability {
    Availability::Unsupported {
        reason: "not implemented yet".into(),
    }
}
```

6. Run `cd src-tauri && cargo fmt -p trigger-docker`.

7. Refresh the README Kinds table: run `cd src-tauri && cargo run -q -p trigger-docker --example readme`. It prints a header row, a separator row and five rows, `docker.container-started`, `docker.container-stopped`, `docker.container-healthy`, `docker.container-unhealthy`, `docker.image-built`. In `src-tauri/crates/trigger-docker/README.md` replace everything between the line `<!-- kinds -->` and the line `<!-- /kinds -->` (keep both marker lines) with that output.

8. In the same README make two more replacements. Each replaces a whole line of the template.

   a. In the "Rule fields" table, replace the row `| `example` | yes | | Replace with the real fields |` with these four rows:

```
| `name` | no | any | Glob on the container name. For `docker.image-built` it is tested against the image reference |
| `image` | no | any | Glob on the image. `docker.io/library/` and `:latest` are optional, so `alpine` matches `docker.io/library/alpine:latest` |
| `label` | no | any | `key=value`. The container (or image) must carry that label with exactly that value |
| `engine` | no | `auto` | `docker`, `podman` or `auto`. Which engine the rule listens to |
```

   b. In the "Example" section, replace the line `{ "id": "example", "kind": "docker.example", "when": { "example": "value" } }` with:

```
{ "id": "web-up", "kind": "docker.container-started", "when": { "name": "web-*", "engine": "auto" } }
```

   Leave the "Payload" table (card T07 writes it) and every other section as they are.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker` passes: 9 tests in the library (8 in `rule`, 1 in `logic`), 1 in `readme`, `live` shows 1 ignored
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (it may print warnings for `src/lib.rs` and `src/os.rs` still having template code (both contain the text `not implemented yet`, which slice C removes); those are expected until slice C)
- [ ] `cd src-tauri && cargo run -q -p trigger-docker --example readme | grep -c "^| docker\."` prints `5`
- [ ] `grep -c "docker.example" src-tauri/crates/trigger-docker/README.md` prints `0`
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Any parsing of engine output, the matcher or the payload (T03 to T07).
- Starting a child process or a thread in `start`. It still ends in `Err(SourceError::Unavailable(...))`; slice C replaces it.
- Real OS code in `os.rs` (slice C).
- Changing a kind id, a default, a doc string or the field names. They are decided in the epic.
- Splitting `KINDS` or `rule.rs` into more files.
- Writing the README Payload, Platforms, Permissions or Known limits sections.
- Editing `tests/live.rs`; it is the template's and slice C replaces it. It still compiles and is ignored.

## If stuck

- If a test fails, compare your code with the card character by character. Do not change a test or a fixture line to make it pass.
- If clippy reports `dead_code` in a file other than the ones named in this card, a `#![allow(dead_code)]` line from an earlier card is missing or was moved. Check `src/logic.rs`, `src/rule.rs` and `src/os.rs` each have it directly after their `//!` lines.
- If `cargo test` fails in `tests/readme.rs`, repeat step 7; the table must be the exact program output.
- If `trigger_core::testkit::rule` or `RuleError::Field` does not exist, `001-trigger-core` did not land. Set status blocked.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.

- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (check-crates exits 0 with warnings: README Payload and Platforms still template text, which this card leaves; lib.rs and os.rs template warnings are expected until slice C)
- 2026-10-12 checker: pass
