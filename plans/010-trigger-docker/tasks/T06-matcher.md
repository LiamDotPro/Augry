---
id: T06
epic: 010-trigger-docker
title: Add logic::matches for the four when fields
status: done
depends_on: [T05]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-docker/src/logic.rs
  - src-tauri/crates/trigger-docker/src/logic/match_tests.rs
---

## Goal

`logic::matches(armed, event)` says whether a rule fires for an event: the kind, the engine, and each of `name`, `image` and `label` the rule set. The image glob is tried with `docker.io/library/` removed and with `:latest` added or removed.

## Read first

- `plans/010-trigger-docker/epic.md` Decisions 3 and 10 (the image glob rule)
- `plans/010-trigger-docker/research/R01-events-json.md` "Implications", item 8
- `src-tauri/crates/trigger-docker/src/rule.rs` `Parsed` and `EngineChoice`, the types the matcher reads
- `src-tauri/crates/trigger-docker/src/logic.rs` the end of the file, where the new code goes

## Steps

1. In `src-tauri/crates/trigger-docker/src/logic.rs`, change the line

```rust
use crate::rule::Parsed;
```

to

```rust
use crate::rule::{EngineChoice, Parsed};
```

2. In the same file find these four lines near the end:

```rust
#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod tests;
```

Insert the block below directly above them (leave one blank line between). Then change those four lines so the end of the file reads exactly:

```rust
#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod match_tests;
#[cfg(test)]
mod tests;
```

```rust
/// The forms of an image reference a rule's `image` glob is tried against: as
/// printed, without a leading `docker.io/library/`, and each of those with
/// `:latest` added or removed. So `alpine` matches `docker.io/library/alpine`
/// and `alpine:latest` matches `alpine`.
fn image_candidates(image: &str) -> Vec<String> {
    let stripped = image.strip_prefix("docker.io/library/").unwrap_or(image);
    let mut out = Vec::new();
    for base in [image, stripped] {
        out.push(base.to_string());
        if let Some(without) = base.strip_suffix(":latest") {
            out.push(without.to_string());
        } else {
            let last_segment = base.rsplit('/').next().unwrap_or(base);
            if !last_segment.contains(':') && !last_segment.contains('@') {
                out.push(format!("{base}:latest"));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Whether the rule fires for the event: the kind, the engine, and every `when`
/// field the rule set. For `image-built` the event's name is the image
/// reference, so `name` and `image` globs both test that.
pub fn matches(armed: &Armed, event: &EngineEvent) -> bool {
    if armed.kind != event.kind.id() {
        return false;
    }
    let when = &armed.when;
    let engine_ok = match when.when.engine {
        EngineChoice::Auto => true,
        EngineChoice::Docker => event.engine == Engine::Docker,
        EngineChoice::Podman => event.engine == Engine::Podman,
    };
    if !engine_ok {
        return false;
    }
    if let Some(name) = &when.name {
        if !name.is_match(&event.name) {
            return false;
        }
    }
    if let Some(image) = &when.image {
        if !image_candidates(&event.image)
            .iter()
            .any(|candidate| image.is_match(candidate))
        {
            return false;
        }
    }
    if let Some((key, value)) = &when.label {
        if event.labels.get(key) != Some(value) {
            return false;
        }
    }
    true
}
```

3. Create `src-tauri/crates/trigger-docker/src/logic/match_tests.rs` with this content. These tests are in their own file so `tests.rs` stays under the size limit.

```rust
use super::fixtures::*;
use super::*;
use serde_json::json;
use trigger_core::testkit::rule;

fn armed(kind: &str, when: serde_json::Value) -> Armed {
    let r = rule("p/t", kind, when);
    let parsed = crate::rule::parse(&r).unwrap();
    Armed {
        id: r.id,
        kind: r.kind,
        when: parsed,
    }
}

fn started() -> EngineEvent {
    parse_line(DOCKER_START).unwrap()
}

#[test]
fn an_empty_when_matches_every_event_of_its_kind_only() {
    let rule = armed("docker.container-started", json!({}));
    assert!(matches(&rule, &started()));
    assert!(!matches(&rule, &parse_line(DOCKER_DIE_3).unwrap()));
}

#[test]
fn the_name_glob_tests_the_container_name() {
    let hit = armed("docker.container-started", json!({ "name": "r01*" }));
    let miss = armed("docker.container-started", json!({ "name": "web-*" }));
    assert!(matches(&hit, &started()));
    assert!(!matches(&miss, &started()));
}

#[test]
fn the_label_must_match_exactly() {
    let hit = armed(
        "docker.container-started",
        json!({ "label": "com.example.team=x" }),
    );
    assert!(matches(&hit, &started()));
    for label in [
        "com.example.team=y",
        "com.example.other=x",
        "com.example.team=",
    ] {
        let miss = armed("docker.container-started", json!({ "label": label }));
        assert!(!matches(&miss, &started()), "{label}");
    }
}

#[test]
fn the_engine_field_is_tested_against_the_engine_that_printed_the_event() {
    let podman_only = armed("docker.container-started", json!({ "engine": "podman" }));
    let docker_only = armed("docker.container-started", json!({ "engine": "docker" }));
    let auto = armed("docker.container-started", json!({ "engine": "auto" }));
    let from_podman = parse_line(PODMAN_START).unwrap();
    assert!(matches(&podman_only, &from_podman));
    assert!(!matches(&podman_only, &started()));
    assert!(matches(&docker_only, &started()));
    assert!(!matches(&docker_only, &from_podman));
    assert!(matches(&auto, &started()) && matches(&auto, &from_podman));
}

#[test]
fn every_set_field_must_match() {
    let rule = armed(
        "docker.container-started",
        json!({ "name": "r01c", "image": "r01test:1", "label": "com.example.k=v" }),
    );
    assert!(matches(&rule, &started()));
    let one_wrong = armed(
        "docker.container-started",
        json!({ "name": "r01c", "image": "r01test:2", "label": "com.example.k=v" }),
    );
    assert!(!matches(&one_wrong, &started()));
}

#[test]
fn image_globs_ignore_docker_io_library_and_latest() {
    let cases = [
        // (glob, image the engine printed, should match)
        ("alpine", "docker.io/library/alpine:latest", true),
        ("alpine", "docker.io/library/alpine", true),
        ("alpine:3", "docker.io/library/alpine:3", true),
        ("alpine", "alpine:3", false),
        ("alpine:latest", "alpine", true),
        ("alpine:latest", "docker.io/library/alpine", true),
        ("alpine*", "docker.io/library/alpine:3", true),
        ("nginx:1.27", "nginx:1.27", true),
        ("nginx:1.27", "nginx:1.28", false),
        ("localhost:5000/app", "localhost:5000/app", true),
        ("localhost:5000/app:latest", "localhost:5000/app", true),
        ("myorg/app", "myorg/app:latest", true),
        // Only docker.io/library/ is dropped, not other docker.io paths.
        ("myorg/app", "docker.io/myorg/app:latest", false),
        ("app", "app@sha256:abc", false),
    ];
    for (glob, image, expected) in cases {
        let rule = armed("docker.container-started", json!({ "image": glob }));
        let mut event = started();
        event.image = image.to_string();
        assert_eq!(matches(&rule, &event), expected, "{glob} vs {image}");
    }
}

#[test]
fn for_image_built_both_name_and_image_test_the_reference() {
    let event = parse_line(DOCKER_IMAGE_TAG).unwrap();
    let by_name = armed("docker.image-built", json!({ "name": "r01test:*" }));
    let by_image = armed("docker.image-built", json!({ "image": "r01test:1" }));
    let wrong_kind = armed("docker.container-started", json!({}));
    assert!(matches(&by_name, &event));
    assert!(matches(&by_image, &event));
    assert!(!matches(&wrong_kind, &event));
}

#[test]
fn health_kinds_are_told_apart() {
    let healthy = armed("docker.container-healthy", json!({}));
    let unhealthy = armed("docker.container-unhealthy", json!({}));
    let event = parse_line(DOCKER_HEALTHY).unwrap();
    assert!(matches(&healthy, &event));
    assert!(!matches(&unhealthy, &event));
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-docker`.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker` passes: the library shows `40 passed`
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (it may print warnings for `src/lib.rs` and `src/os.rs` still having template code (both contain the text `not implemented yet`, which slice C removes); those are expected until slice C)
- [ ] `wc -l src-tauri/crates/trigger-docker/src/logic.rs` prints a number under 400
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Reading `engine` anywhere except `EngineChoice` in the rule against `event.engine`.
- Normalising the `name` glob. Only the `image` glob gets the `docker.io/library/` and `:latest` treatment.
- Stripping other registries such as `docker.io/myorg/`. The test documents that they are not stripped.
- The payload (T07).
- Making globs case insensitive.

## If stuck

- If a test fails, compare your code with the card character by character. Do not change a test or a fixture line to make it pass.
- If clippy reports `dead_code` in a file other than the ones named in this card, a `#![allow(dead_code)]` line from an earlier card is missing or was moved. Check `src/logic.rs`, `src/rule.rs` and `src/os.rs` each have it directly after their `//!` lines.
- If `EngineChoice` is not found, T02 did not land as written. Set status blocked.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.

- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
