---
id: T05
epic: 010-trigger-docker
title: Parse Podman events (synthetic fixtures)
status: done
depends_on: [T04]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-docker/src/logic.rs
  - src-tauri/crates/trigger-docker/src/logic/fixtures.rs
  - src-tauri/crates/trigger-docker/src/logic/tests.rs
  - src-tauri/crates/trigger-docker/README.md
---

## Goal

`parse_line` also reads Podman's flat JSON: start, `died` (numeric exit code), `health_status` (verdict in a top level field) and the image `build` event. The Podman fixture lines are synthetic (made from Podman's own event struct, not captured) and are labelled so.

## Read first

- `plans/010-trigger-docker/research/R01-events-json.md` the Podman lines, "Field differences" and "Doubts"
- `plans/010-trigger-docker/epic.md` Decision 5
- `src-tauri/crates/trigger-docker/src/logic.rs:160-185` the `raw_podman` stub and `map_kind`

## Steps

1. In `src-tauri/crates/trigger-docker/src/logic.rs`, replace the stub function `raw_podman` (the doc comment line `/// Filled in by the Podman card.`, the signature line and the body `None`, four lines in all) with this function:

```rust
/// Podman prints a flat object: the container is at the top level and only the
/// labels are under `Attributes`, which is `null` for some events.
fn raw_podman(obj: &Map<String, Value>) -> Option<Raw> {
    let status = obj.get("Status")?.as_str()?.trim();
    let action = if status == "died" { "die" } else { status };
    let labels = obj
        .get("Attributes")
        .and_then(Value::as_object)
        .map(|a| {
            a.iter()
                .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
                .collect()
        })
        .unwrap_or_default();
    Some(Raw {
        engine: Engine::Podman,
        kind_type: text(obj.get("Type")),
        action: action.to_string(),
        detail: text(obj.get("health_status")).trim().to_string(),
        id: text(obj.get("ID")),
        name: text(obj.get("Name")),
        image: text(obj.get("Image")),
        labels,
        exit_code: number(obj.get("ContainerExitCode")).and_then(|n| i32::try_from(n).ok()),
        time_ms: time_ms(obj),
    })
}
```

2. In the same file, in `map_kind`, find the line

```rust
        ("image", "tag") if raw.engine == Engine::Docker => Some(EventKind::ImageBuilt),
```

and add this line directly under it:

```rust
        ("image", "build") if raw.engine == Engine::Podman => Some(EventKind::ImageBuilt),
```

3. Append this to the end of `src-tauri/crates/trigger-docker/src/logic/fixtures.rs` after a blank line. Keep the SYNTHETIC comment exactly; it is the honest label for these lines.

```rust
// Podman lines. SYNTHETIC: R01 had no Podman to capture from. They were made by
// marshalling Podman v5.6.0's own event struct, so key names and order are
// right for v5, but they are not captured from a running Podman. Replace them
// with a capture when a machine with Podman is available.

/// Synthetic.
pub const PODMAN_START: &str = r#"{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"start","time":1791799200,"timeNano":1791799200123456789,"Type":"container","Attributes":{"com.example.k":"v"}}"#;

/// Synthetic.
pub const PODMAN_DIED_3: &str = r#"{"ContainerExitCode":3,"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"died","time":1791799200,"timeNano":1791799200123456789,"Type":"container","Attributes":{"com.example.k":"v"}}"#;

/// Synthetic.
pub const PODMAN_DIED_137: &str = r#"{"ContainerExitCode":137,"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"died","time":1791799200,"timeNano":1791799200123456789,"Type":"container","Attributes":{"com.example.k":"v"}}"#;

/// Synthetic. Must be ignored.
pub const PODMAN_STOP: &str = r#"{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"stop","time":1791799200,"timeNano":1791799200123456789,"Type":"container","Attributes":{"com.example.k":"v"}}"#;

/// Synthetic.
pub const PODMAN_HEALTHY: &str = r#"{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"health_status","time":1791799200,"timeNano":1791799200123456789,"Type":"container","health_status":"healthy","Attributes":{"com.example.k":"v"}}"#;

/// Synthetic.
pub const PODMAN_UNHEALTHY: &str = r#"{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"health_status","time":1791799200,"timeNano":1791799200123456789,"Type":"container","health_status":"unhealthy","Attributes":{"com.example.k":"v"}}"#;

/// Synthetic. Podman's image build event: no id, `Attributes` is null.
pub const PODMAN_BUILD: &str = r#"{"Name":"localhost/r01test:1","Status":"build","time":1791799200,"timeNano":1791799200123456789,"Type":"image","Attributes":null}"#;

/// Hand made. Must be ignored.
pub const PODMAN_CREATE: &str = r#"{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"create","time":1791799200,"timeNano":1791799200123456789,"Type":"container","Attributes":{"com.example.k":"v"}}"#;

/// Hand made. Must be ignored: a pulled image is not a built one.
pub const PODMAN_IMAGE_PULL: &str = r#"{"ID":"sha256:d760162063cb2959aa76446b17be2d95caa8289dd13aca56a65e12ec54eafa9f","Name":"docker.io/library/alpine:3","Status":"pull","time":1791799200,"timeNano":1791799200123456789,"Type":"image","Attributes":null}"#;
```

4. Append this to the end of `src-tauri/crates/trigger-docker/src/logic/tests.rs` after a blank line.

```rust
// The Podman fixtures are synthetic: see the note in fixtures.rs.

const PODMAN_ID: &str = "b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244";

#[test]
fn podman_start() {
    let e = parse_line(PODMAN_START).unwrap();
    assert_eq!(
        e,
        EngineEvent {
            engine: Engine::Podman,
            kind: EventKind::Started,
            id: PODMAN_ID.into(),
            name: "r01c".into(),
            image: "docker.io/library/alpine:3".into(),
            labels: labels(&[("com.example.k", "v")]),
            exit_code: None,
            time_ms: 1_791_799_200_123,
        }
    );
}

#[test]
fn podman_died_is_stopped_with_a_numeric_exit_code() {
    let e = parse_line(PODMAN_DIED_3).unwrap();
    assert_eq!(e.engine, Engine::Podman);
    assert_eq!(e.kind, EventKind::Stopped);
    assert_eq!(e.exit_code, Some(3));
    assert_eq!(parse_line(PODMAN_DIED_137).unwrap().exit_code, Some(137));
}

#[test]
fn podman_died_without_an_exit_code_is_still_stopped() {
    let line = PODMAN_DIED_3.replace(r#""ContainerExitCode":3,"#, "");
    let e = parse_line(&line).unwrap();
    assert_eq!(e.kind, EventKind::Stopped);
    assert_eq!(e.exit_code, None);
}

#[test]
fn podman_health_verdict_comes_from_the_top_level_field() {
    assert_eq!(parse_line(PODMAN_HEALTHY).unwrap().kind, EventKind::Healthy);
    assert_eq!(
        parse_line(PODMAN_UNHEALTHY).unwrap().kind,
        EventKind::Unhealthy
    );
    let starting = PODMAN_HEALTHY.replace(
        r#""health_status":"healthy""#,
        r#""health_status":"starting""#,
    );
    assert_eq!(parse_line(&starting), None);
    let none = PODMAN_HEALTHY.replace(r#""health_status":"healthy","#, "");
    assert_eq!(parse_line(&none), None);
}

#[test]
fn podman_build_is_image_built_with_null_attributes() {
    let e = parse_line(PODMAN_BUILD).unwrap();
    assert_eq!(
        e,
        EngineEvent {
            engine: Engine::Podman,
            kind: EventKind::ImageBuilt,
            id: String::new(),
            name: "localhost/r01test:1".into(),
            image: "localhost/r01test:1".into(),
            labels: BTreeMap::new(),
            exit_code: None,
            time_ms: 1_791_799_200_123,
        }
    );
}

#[test]
fn podman_noise_gives_none() {
    for (what, line) in [
        ("stop", PODMAN_STOP),
        ("create", PODMAN_CREATE),
        ("image pull", PODMAN_IMAGE_PULL),
    ] {
        assert_eq!(parse_line(line), None, "{what} must be ignored");
    }
}

#[test]
fn each_engine_only_builds_on_its_own_image_event() {
    // Docker's `tag` shape with Podman's `build` word, and the reverse.
    let docker_build = DOCKER_IMAGE_TAG.replace(r#""Action":"tag""#, r#""Action":"build""#);
    assert_eq!(parse_line(&docker_build), None);
    let podman_tag = PODMAN_BUILD.replace(r#""Status":"build""#, r#""Status":"tag""#);
    assert_eq!(parse_line(&podman_tag), None);
}

#[test]
fn the_shape_not_the_caller_decides_the_engine() {
    assert_eq!(parse_line(DOCKER_START).unwrap().engine, Engine::Docker);
    assert_eq!(parse_line(PODMAN_START).unwrap().engine, Engine::Podman);
}
```

5. Append this line to the very end of `src-tauri/crates/trigger-docker/README.md`, after a blank line:

```
- The Podman parsing is checked against lines made from Podman v5.6.0's own event struct, not lines captured from a running Podman. Podman 6 and remote Podman (`podman machine`) are not checked.
```

6. Run `cd src-tauri && cargo fmt -p trigger-docker`.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker` passes: the library shows `32 passed`
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (it may print warnings for `src/lib.rs` and `src/os.rs` still having template code (both contain the text `not implemented yet`, which slice C removes); those are expected until slice C)
- [ ] `grep -c SYNTHETIC src-tauri/crates/trigger-docker/src/logic/fixtures.rs` prints `1`
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Reading `engine` from anywhere but the shape of the JSON.
- Mapping Podman `tag`, `pull`, `create`, `remove`, `init`, `cleanup` or `stop`. They stay `None`.
- Replacing the synthetic lines with captures: no Podman is available, and slice C adds a live test.
- The matcher and the payload (T06, T07).

## If stuck

- If a test fails, compare your code with the card character by character. Do not change a test or a fixture line to make it pass.
- If `raw_podman` is not a stub returning `None`, or `map_kind` has no `("image", "tag")` line, T03 or T04 did not land as written. Set status blocked.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.

- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
