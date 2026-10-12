---
id: T07
epic: 010-trigger-docker
title: Add the payload builder and logic::fire
status: done
depends_on: [T06]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-docker/src/logic.rs
  - src-tauri/crates/trigger-docker/src/logic/match_tests.rs
  - src-tauri/crates/trigger-docker/README.md
---

## Goal

`logic::payload(event)` builds the payload each kind documents (Decision 10) and `logic::fire(armed, event)` returns the (rule id, kind, payload) for every rule that matches. A test checks the payload keys equal the ones `KINDS` documents. The README Payload table is written.

## Read first

- `plans/010-trigger-docker/epic.md` Decision 10 (payload)
- `src-tauri/crates/trigger-docker/src/lib.rs:36-80` the payload `FieldInfo` constants the keys must equal
- `src-tauri/crates/trigger-docker/src/logic.rs` the end of the file

## Steps

1. In `src-tauri/crates/trigger-docker/src/logic.rs`, find the lines

```rust
#[cfg(test)]
mod fixtures;
```

near the end of the file and insert this block directly above them (leave one blank line between). `short_id` takes the first 12 characters and drops a `sha256:` prefix. `exit_code` is `-1` when the engine did not report one. `image-built` has no `container_id`.

```rust
/// The first 12 characters of an id, without a `sha256:` prefix.
fn short_id(id: &str) -> String {
    id.strip_prefix("sha256:")
        .unwrap_or(id)
        .chars()
        .take(12)
        .collect()
}

/// What an event carries to the steps. The keys are the ones `KINDS` documents
/// for the event's kind: a test checks they stay in step.
pub fn payload(event: &EngineEvent) -> Payload {
    let mut out = Payload::new();
    out.insert("engine".into(), Value::from(event.engine.as_str()));
    out.insert("name".into(), Value::from(event.name.clone()));
    out.insert("image".into(), Value::from(event.image.clone()));
    out.insert("time".into(), Value::from(event.time_ms));
    match event.kind {
        EventKind::ImageBuilt => {}
        EventKind::Started => {
            out.insert("container_id".into(), Value::from(short_id(&event.id)));
        }
        EventKind::Stopped => {
            out.insert("container_id".into(), Value::from(short_id(&event.id)));
            out.insert(
                "exit_code".into(),
                Value::from(event.exit_code.unwrap_or(-1)),
            );
        }
        EventKind::Healthy | EventKind::Unhealthy => {
            out.insert("container_id".into(), Value::from(short_id(&event.id)));
            let verdict = if event.kind == EventKind::Healthy {
                "healthy"
            } else {
                "unhealthy"
            };
            out.insert("health".into(), Value::from(verdict));
        }
    }
    out
}

/// The rules an event fires: (rule id, kind, payload).
pub fn fire(armed: &[Armed], event: &EngineEvent) -> Vec<(RuleId, String, Payload)> {
    armed
        .iter()
        .filter(|a| matches(a, event))
        .map(|a| (a.id.clone(), a.kind.clone(), payload(event)))
        .collect()
}
```

2. Append this to the end of `src-tauri/crates/trigger-docker/src/logic/match_tests.rs` after a blank line.

```rust
fn payload_of(line: &str) -> Payload {
    payload(&parse_line(line).unwrap())
}

#[test]
fn started_payload() {
    let p = payload_of(DOCKER_START);
    assert_eq!(p["engine"], json!("docker"));
    assert_eq!(p["container_id"], json!("ef63764e9679"));
    assert_eq!(p["name"], json!("r01c"));
    assert_eq!(p["image"], json!("r01test:1"));
    assert_eq!(p["time"], json!(1_791_837_794_607_i64));
    assert!(!p.contains_key("exit_code") && !p.contains_key("health"));
}

#[test]
fn stopped_payload_has_the_exit_code() {
    assert_eq!(payload_of(DOCKER_DIE_137)["exit_code"], json!(137));
    assert_eq!(payload_of(PODMAN_DIED_3)["exit_code"], json!(3));
    let none = PODMAN_DIED_3.replace(r#""ContainerExitCode":3,"#, "");
    assert_eq!(payload_of(&none)["exit_code"], json!(-1));
}

#[test]
fn health_payload_has_the_verdict() {
    assert_eq!(payload_of(DOCKER_HEALTHY)["health"], json!("healthy"));
    assert_eq!(payload_of(DOCKER_UNHEALTHY)["health"], json!("unhealthy"));
    assert_eq!(payload_of(PODMAN_UNHEALTHY)["health"], json!("unhealthy"));
}

#[test]
fn image_built_payload_has_no_container_id() {
    let p = payload_of(DOCKER_IMAGE_TAG);
    assert_eq!(p["name"], json!("r01test:1"));
    assert_eq!(p["image"], json!("r01test:1"));
    assert!(!p.contains_key("container_id"));
    let podman = payload_of(PODMAN_BUILD);
    assert_eq!(podman["engine"], json!("podman"));
    assert_eq!(podman["name"], json!("localhost/r01test:1"));
}

#[test]
fn a_podman_container_id_is_shortened_too() {
    assert_eq!(
        payload_of(PODMAN_START)["container_id"],
        json!("b2b0cf48d0c0")
    );
}

#[test]
fn payload_keys_are_exactly_the_ones_kinds_documents() {
    for line in [
        DOCKER_START,
        DOCKER_DIE_137,
        DOCKER_HEALTHY,
        DOCKER_UNHEALTHY,
        DOCKER_IMAGE_TAG,
        PODMAN_START,
        PODMAN_DIED_3,
        PODMAN_HEALTHY,
        PODMAN_BUILD,
    ] {
        let event = parse_line(line).unwrap();
        let info = crate::KINDS
            .iter()
            .find(|k| k.id == event.kind.id())
            .unwrap();
        let mut documented: Vec<&str> = info.payload.iter().map(|f| f.name).collect();
        documented.sort();
        let p = payload(&event);
        let mut actual: Vec<&str> = p.keys().map(String::as_str).collect();
        actual.sort();
        assert_eq!(actual, documented, "{}", event.kind.id());
    }
}

#[test]
fn fire_returns_one_entry_per_matching_rule() {
    let rules = vec![
        Armed {
            id: "p/a".into(),
            ..armed("docker.container-started", json!({ "name": "r01*" }))
        },
        Armed {
            id: "p/b".into(),
            ..armed("docker.container-started", json!({ "name": "web-*" }))
        },
        Armed {
            id: "p/c".into(),
            ..armed("docker.container-stopped", json!({}))
        },
        Armed {
            id: "p/d".into(),
            ..armed(
                "docker.container-started",
                json!({ "label": "com.example.k=v" }),
            )
        },
    ];
    let fired = fire(&rules, &started());
    let ids: Vec<&str> = fired.iter().map(|(id, _, _)| id.as_str()).collect();
    assert_eq!(ids, vec!["p/a", "p/d"]);
    assert_eq!(fired[0].1, "docker.container-started");
    assert_eq!(fired[0].2["name"], json!("r01c"));
}

#[test]
fn fire_with_no_rules_is_empty() {
    assert!(fire(&[], &started()).is_empty());
}
```

3. In `src-tauri/crates/trigger-docker/README.md`, in the "Payload" section, replace the table row `| `example` | Replace with the real fields |` with these rows:

```
| `engine` | `docker` or `podman`, whichever printed the event |
| `container_id` | First 12 characters of the container id. Not on `docker.image-built` |
| `name` | Container name, or the image reference for `docker.image-built` |
| `image` | Image the container runs, as the engine reports it. For `docker.image-built` the same as `name` |
| `time` | When the engine reported it, in milliseconds since 1970 |
| `exit_code` | `docker.container-stopped` only. Exit code of the main process, `-1` when the engine did not report one |
| `health` | `docker.container-healthy` and `docker.container-unhealthy` only: `healthy` or `unhealthy` |
```

4. Run `cd src-tauri && cargo fmt -p trigger-docker`.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker` passes: the library shows `48 passed`
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (it may print warnings for `src/lib.rs` and `src/os.rs` still having template code (both contain the text `not implemented yet`, which slice C removes); those are expected until slice C)
- [ ] `grep -c "Replace with" src-tauri/crates/trigger-docker/README.md` prints `0`
- [ ] `wc -l src-tauri/crates/trigger-docker/src/logic.rs` prints a number under 400
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Adding payload fields (labels, the full id, `signal`). Paths, names, ids and numbers only, as decided.
- Changing the `KINDS` payload lists. If the key test fails, the code in step 1 is wrong, not `KINDS`.
- Debounce or de-duplication state. Docker prints each event once.
- Sending the events anywhere: `fire` only returns the list. The sink is slice C.

## If stuck

- If a test fails, compare your code with the card character by character. Do not change a test or a fixture line to make it pass.
- If clippy reports `dead_code` in a file other than the ones named in this card, a `#![allow(dead_code)]` line from an earlier card is missing or was moved. Check `src/logic.rs`, `src/rule.rs` and `src/os.rs` each have it directly after their `//!` lines.
- If the key test `payload_keys_are_exactly_the_ones_kinds_documents` fails, compare the `insert` calls with step 1; do not edit `KINDS`.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.

- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
