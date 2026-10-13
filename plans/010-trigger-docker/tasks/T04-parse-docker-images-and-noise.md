---
id: T04
epic: 010-trigger-docker
title: Parse Docker image tags and prove noise is ignored
status: done
depends_on: [T03]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-docker/src/logic.rs
  - src-tauri/crates/trigger-docker/src/logic/fixtures.rs
  - src-tauri/crates/trigger-docker/src/logic/tests.rs
  - src-tauri/crates/trigger-docker/README.md
---

## Goal

A Docker `image tag` event gives `docker.image-built`, and tests prove every other line R01 lists as noise (kill, stop, exec_die, exec_start, create, destroy, network, image create, pull, untag) gives `None`. The README states the `docker tag` limit.

## Read first

- `plans/010-trigger-docker/research/R01-events-json.md` the "Image build" part and the "Doubts" about `tag`
- `plans/010-trigger-docker/epic.md` Decision 5, last sentence
- `src-tauri/crates/trigger-docker/src/logic.rs:165-185` `map_kind`

## Steps

1. In `src-tauri/crates/trigger-docker/src/logic.rs`, in the function `map_kind`, find the line

```rust
        ("container", "die") => Some(EventKind::Stopped),
```

and add this line directly under it (the guard keeps Podman's own image word out of Docker's arm):

```rust
        ("image", "tag") if raw.engine == Engine::Docker => Some(EventKind::ImageBuilt),
```

2. Append this to the end of `src-tauri/crates/trigger-docker/src/logic/fixtures.rs` after a blank line.

```rust
/// Captured: BuildKit with the containerd image store, the first of the two
/// lines a whole build prints. Must be ignored (it also fires once per step
/// of a legacy build).
pub const DOCKER_IMAGE_CREATE: &str = r#"{"Type":"image","Action":"create","Actor":{"ID":"sha256:d760162063cb2959aa76446b17be2d95caa8289dd13aca56a65e12ec54eafa9f","Attributes":{"com.example.team":"x","name":"sha256:d760162063cb2959aa76446b17be2d95caa8289dd13aca56a65e12ec54eafa9f"}},"scope":"local","time":1791837793,"timeNano":1791837793998815716}"#;

/// Captured: the second line of that build, the one that means "built".
pub const DOCKER_IMAGE_TAG: &str = r#"{"Type":"image","Action":"tag","Actor":{"ID":"sha256:d760162063cb2959aa76446b17be2d95caa8289dd13aca56a65e12ec54eafa9f","Attributes":{"com.example.team":"x","name":"r01test:1"}},"scope":"local","time":1791837794,"timeNano":1791837794011737591}"#;

/// Hand made. Must be ignored.
pub const DOCKER_IMAGE_PULL: &str = r#"{"Type":"image","Action":"pull","Actor":{"ID":"docker.io/library/alpine:3","Attributes":{"name":"docker.io/library/alpine"}},"scope":"local","time":1791837700,"timeNano":1791837700000000000}"#;

/// Hand made. Must be ignored.
pub const DOCKER_IMAGE_UNTAG: &str = r#"{"Type":"image","Action":"untag","Actor":{"ID":"sha256:d760162063cb2959aa76446b17be2d95caa8289dd13aca56a65e12ec54eafa9f","Attributes":{"name":"sha256:d760162063cb2959aa76446b17be2d95caa8289dd13aca56a65e12ec54eafa9f"}},"scope":"local","time":1791837900,"timeNano":1791837900000000000}"#;
```

3. Append this to the end of `src-tauri/crates/trigger-docker/src/logic/tests.rs` after a blank line.

```rust
#[test]
fn docker_image_tag_means_built() {
    let e = parse_line(DOCKER_IMAGE_TAG).unwrap();
    assert_eq!(
        e,
        EngineEvent {
            engine: Engine::Docker,
            kind: EventKind::ImageBuilt,
            id: "sha256:d760162063cb2959aa76446b17be2d95caa8289dd13aca56a65e12ec54eafa9f".into(),
            name: "r01test:1".into(),
            image: "r01test:1".into(),
            labels: labels(&[("com.example.team", "x")]),
            exit_code: None,
            time_ms: 1_791_837_794_011,
        }
    );
}

#[test]
fn the_noise_the_engine_prints_gives_none() {
    for (what, line) in [
        ("kill", DOCKER_KILL),
        ("stop", DOCKER_STOP),
        ("exec_die", DOCKER_EXEC_DIE),
        ("exec_start", DOCKER_EXEC_START),
        ("create", DOCKER_CREATE),
        ("destroy", DOCKER_DESTROY),
        ("network connect", DOCKER_NETWORK_CONNECT),
        ("image create", DOCKER_IMAGE_CREATE),
        ("image pull", DOCKER_IMAGE_PULL),
        ("image untag", DOCKER_IMAGE_UNTAG),
    ] {
        assert_eq!(parse_line(line), None, "{what} must be ignored");
    }
}

#[test]
fn actions_are_compared_whole_not_by_prefix() {
    let longer = DOCKER_DIE_3.replace(r#""Action":"die""#, r#""Action":"die_later""#);
    assert_eq!(parse_line(&longer), None);
    let shorter = DOCKER_START.replace(r#""Action":"start""#, r#""Action":"star""#);
    assert_eq!(parse_line(&shorter), None);
}

#[test]
fn a_container_die_is_never_taken_from_an_image_or_network_line() {
    let line = DOCKER_DIE_3.replace(r#""Type":"container""#, r#""Type":"network""#);
    assert_eq!(parse_line(&line), None);
}

#[test]
fn a_tag_on_a_container_is_not_an_image_built() {
    let line = DOCKER_IMAGE_TAG.replace(r#""Type":"image""#, r#""Type":"container""#);
    assert_eq!(parse_line(&line), None);
}
```

4. Append these lines to the very end of `src-tauri/crates/trigger-docker/README.md` (the "Known limits" section is last), after a blank line:

```
- `docker.image-built` fires on any `image tag` event, so a manual `docker tag` fires it too. Docker has no event that says a build ran; a tagged build always ends with a tag event, so this is the best signal. An untagged build fires nothing.
```

5. Run `cd src-tauri && cargo fmt -p trigger-docker`.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker` passes: the library shows `24 passed`
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (it may print warnings for `src/lib.rs` and `src/os.rs` still having template code (both contain the text `not implemented yet`, which slice C removes); those are expected until slice C)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- The Podman `build` event (T05).
- Mapping `image create`, `pull`, `untag` or `delete`. They must stay `None`.
- Changing `parse_line` or any other function.
- Writing the other README sections.

## If stuck

- If a test fails, compare your code with the card character by character. Do not change a test or a fixture line to make it pass.
- If `map_kind` does not contain the `die` line shown in step 1, T03 did not land as written. Set status blocked.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.

- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
