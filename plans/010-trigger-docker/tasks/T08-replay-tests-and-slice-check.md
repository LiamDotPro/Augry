---
id: T08
epic: 010-trigger-docker
title: Replay whole engine sessions and check the slice
status: done
depends_on: [T07]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-docker/src/logic/match_tests.rs
---

## Goal

Whole R01 sessions (a Docker one in captured order with the noise between, and a Podman one) are replayed through `parse_line` and `fire`, proving each real thing fires exactly once. The card ends with the slice A checks: sizes, README test and crate rules.

## Read first

- `plans/010-trigger-docker/research/R01-events-json.md` the "Fixtures" paragraph
- `src-tauri/crates/trigger-docker/src/logic/fixtures.rs` the constants the tests use
- `src-tauri/crates/trigger-docker/src/logic/match_tests.rs` the file you append to

## Steps

1. Append this to the end of `src-tauri/crates/trigger-docker/src/logic/match_tests.rs` after a blank line.

```rust
/// One rule per kind, no `when` fields: what fires for a stream of lines.
fn fired_kinds(lines: &[&str], rules: &[Armed]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| parse_line(line))
        .flat_map(|event| fire(rules, &event))
        .map(|(_, kind, _)| kind)
        .collect()
}

fn one_rule_per_kind() -> Vec<Armed> {
    crate::KINDS
        .iter()
        .map(|k| armed(k.id, json!({})))
        .collect()
}

#[test]
fn a_docker_session_fires_each_thing_once() {
    // The order R01 captured, with the noise in between.
    let lines = [
        DOCKER_IMAGE_CREATE,
        DOCKER_IMAGE_TAG,
        DOCKER_CREATE,
        DOCKER_NETWORK_CONNECT,
        DOCKER_START,
        DOCKER_EXEC_START,
        DOCKER_EXEC_DIE,
        DOCKER_UNHEALTHY,
        DOCKER_HEALTHY,
        DOCKER_KILL,
        DOCKER_STOP,
        DOCKER_DIE_137,
        DOCKER_DESTROY,
        DOCKER_DIE_3,
    ];
    assert_eq!(
        fired_kinds(&lines, &one_rule_per_kind()),
        vec![
            "docker.image-built",
            "docker.container-started",
            "docker.container-unhealthy",
            "docker.container-healthy",
            "docker.container-stopped",
            "docker.container-stopped",
        ]
    );
}

#[test]
fn a_podman_session_fires_each_thing_once() {
    let lines = [
        PODMAN_CREATE,
        PODMAN_START,
        PODMAN_HEALTHY,
        PODMAN_UNHEALTHY,
        PODMAN_STOP,
        PODMAN_DIED_137,
        PODMAN_IMAGE_PULL,
        PODMAN_BUILD,
    ];
    assert_eq!(
        fired_kinds(&lines, &one_rule_per_kind()),
        vec![
            "docker.container-started",
            "docker.container-healthy",
            "docker.container-unhealthy",
            "docker.container-stopped",
            "docker.image-built",
        ]
    );
}

#[test]
fn a_label_rule_skips_the_container_without_that_label() {
    let rules = vec![armed(
        "docker.container-stopped",
        json!({ "label": "com.example.team=x" }),
    )];
    let fired = fired_kinds(&[DOCKER_DIE_137, DOCKER_DIE_3], &rules);
    assert_eq!(fired, vec!["docker.container-stopped"]);
}

#[test]
fn a_podman_only_rule_ignores_a_docker_stream() {
    let rules = vec![armed(
        "docker.container-started",
        json!({ "engine": "podman" }),
    )];
    assert!(fired_kinds(&[DOCKER_START], &rules).is_empty());
    assert_eq!(fired_kinds(&[PODMAN_START], &rules).len(), 1);
}
```

2. Run `cd src-tauri && cargo fmt -p trigger-docker`.

3. Check sizes: `wc -l src-tauri/crates/trigger-docker/src/logic.rs src-tauri/crates/trigger-docker/src/logic/*.rs src-tauri/crates/trigger-docker/src/rule.rs src-tauri/crates/trigger-docker/src/lib.rs`. Every file is under 400 lines. If one is not, use If stuck.

4. Check the README: `src-tauri/crates/trigger-docker/README.md` has the eight sections, the Kinds table with five rows, four rows under Rule fields, seven rows under Payload and two bullets under Known limits. Platforms and Permissions are still the template's empty tables; slice C fills them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker` passes: the library shows `52 passed`
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (it may print warnings for `src/lib.rs` and `src/os.rs` still having template code (both contain the text `not implemented yet`, which slice C removes); those are expected until slice C)
- [ ] `cd src-tauri && cargo test -p trigger-docker --test readme` passes
- [ ] `grep -c "Replace with" src-tauri/crates/trigger-docker/README.md` prints `0`
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Starting a child process, a supervisor, `find_engine` or any OS code (slices B and C).
- Registering the crate in `src-tauri/Cargo.toml` or `sources()` (slice D).
- Completing the README Platforms and Permissions tables, or the restart limit bullet (slice C).
- Refactoring any file. Keep the `#![allow(dead_code)]` lines.

## If stuck

- If a test fails, compare your code with the card character by character. Do not change a test or a fixture line to make it pass.
- If a file is over 400 lines, set status blocked and say which file and how long. Do not split files yourself.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.

- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
