---
id: T03
epic: 010-trigger-docker
title: Parse Docker container events in logic.rs
status: todo
depends_on: [T02]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-docker/src/logic.rs
  - src-tauri/crates/trigger-docker/src/logic/fixtures.rs
  - src-tauri/crates/trigger-docker/src/logic/tests.rs
---

## Goal

`logic::parse_line` turns one line of `docker events` output into an `EngineEvent` for the three container kinds: start, die (with the exit code) and health_status (healthy or unhealthy). Verbatim captured Docker lines are the test fixtures. The Docker image event, the Podman shape and the matcher come in later cards.

## Read first

- `plans/010-trigger-docker/research/R01-events-json.md` the whole note: the real lines and the parser design (steps 1 to 7)
- `plans/010-trigger-docker/epic.md` Decision 5
- `src-tauri/crates/trigger-docker/src/logic.rs` the file from T02 you replace

## Steps

1. Replace the whole of `src-tauri/crates/trigger-docker/src/logic.rs` with this. Read the parts as you paste: `Engine` and `EventKind` are the vocabulary; `EngineEvent` is what `parse_line` returns; `Raw` is the shared in-between form that both Docker and Podman lines are reduced to; `raw_docker` reads the Docker shape (`Actor` is an object) and `raw_podman` is a stub that a later card fills in; `map_kind` decides the kind from the type and the action, comparing whole strings (never `starts_with`), so `exec_die`, `stop`, `kill`, `create` and `destroy` give `None`. Keep the `#![allow(dead_code)]` line.

```rust
//! Pure decisions: given an event the engine printed, which rules fire and with
//! what payload. No child processes, no threads, no clocks: every case is a
//! unit test.

// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]

use std::collections::BTreeMap;

use serde_json::Map;
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

/// The engine that printed an event, told apart by the shape of the JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Docker,
    Podman,
}

impl Engine {
    pub fn as_str(self) -> &'static str {
        match self {
            Engine::Docker => "docker",
            Engine::Podman => "podman",
        }
    }
}

/// What an engine event means to this crate. Everything else is dropped by
/// `parse_line`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Started,
    Stopped,
    Healthy,
    Unhealthy,
    ImageBuilt,
}

impl EventKind {
    /// The kind id in `KINDS`.
    pub fn id(self) -> &'static str {
        match self {
            EventKind::Started => "docker.container-started",
            EventKind::Stopped => "docker.container-stopped",
            EventKind::Healthy => "docker.container-healthy",
            EventKind::Unhealthy => "docker.container-unhealthy",
            EventKind::ImageBuilt => "docker.image-built",
        }
    }
}

/// One engine event, the same for Docker and Podman. For `ImageBuilt`, `name`
/// and `image` both hold the image reference and `id` may be empty.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineEvent {
    pub engine: Engine,
    pub kind: EventKind,
    /// The id exactly as the engine printed it (64 hex characters for a
    /// container, `sha256:...` for an image).
    pub id: String,
    pub name: String,
    pub image: String,
    pub labels: BTreeMap<String, String>,
    pub exit_code: Option<i32>,
    /// Milliseconds since 1970.
    pub time_ms: i64,
}

/// An engine line before it is mapped to a kind: both shapes reduced to one
/// vocabulary.
struct Raw {
    engine: Engine,
    /// `container`, `image`, `network` ...
    kind_type: String,
    /// The action before any `:`, with Podman's `died` turned into `die`.
    action: String,
    /// The text after the `:` of a Docker action, or Podman's `health_status`.
    detail: String,
    id: String,
    name: String,
    image: String,
    labels: BTreeMap<String, String>,
    exit_code: Option<i32>,
    time_ms: i64,
}

/// Docker puts these in `Actor.Attributes` next to the labels. They are not
/// labels.
const NOT_LABELS: &[&str] = &[
    "name",
    "image",
    "exitCode",
    "execDuration",
    "execID",
    "signal",
    "container",
    "type",
];

fn text(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or("").to_string()
}

/// A number that the engine may print as a JSON number or as a string.
fn number(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// `timeNano` when present, else `time` seconds.
fn time_ms(obj: &Map<String, Value>) -> i64 {
    if let Some(nano) = number(obj.get("timeNano")) {
        return nano / 1_000_000;
    }
    number(obj.get("time")).map_or(0, |s| s.saturating_mul(1000))
}

fn raw_docker(obj: &Map<String, Value>) -> Option<Raw> {
    let actor = obj.get("Actor")?.as_object()?;
    let attrs = actor.get("Attributes").and_then(Value::as_object);
    let attr = |key: &str| attrs.and_then(|a| a.get(key));
    let action = obj.get("Action")?.as_str()?;
    let (base, detail) = match action.split_once(':') {
        Some((base, detail)) => (base.trim(), detail.trim()),
        None => (action.trim(), ""),
    };
    let labels = attrs
        .map(|a| {
            a.iter()
                .filter(|(k, _)| !NOT_LABELS.contains(&k.as_str()))
                .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
                .collect()
        })
        .unwrap_or_default();
    Some(Raw {
        engine: Engine::Docker,
        kind_type: text(obj.get("Type")),
        action: base.to_string(),
        detail: detail.to_string(),
        id: text(actor.get("ID")),
        name: text(attr("name")),
        image: text(attr("image")),
        labels,
        exit_code: number(attr("exitCode")).and_then(|n| i32::try_from(n).ok()),
        time_ms: time_ms(obj),
    })
}

/// Filled in by the Podman card.
fn raw_podman(_obj: &Map<String, Value>) -> Option<Raw> {
    None
}

fn map_kind(raw: &Raw) -> Option<EventKind> {
    match (raw.kind_type.as_str(), raw.action.as_str()) {
        ("container", "start") => Some(EventKind::Started),
        ("container", "die") => Some(EventKind::Stopped),
        ("container", "health_status") => match raw.detail.as_str() {
            "healthy" => Some(EventKind::Healthy),
            "unhealthy" => Some(EventKind::Unhealthy),
            _ => None,
        },
        _ => None,
    }
}

/// Reads one line of `events` output. `None` for anything that is not an event
/// this crate fires on: other actions (`stop`, `kill`, `exec_*`), other types,
/// and lines that are not JSON.
pub fn parse_line(line: &str) -> Option<EngineEvent> {
    let value: Value = serde_json::from_str(line.trim()).ok()?;
    let obj = value.as_object()?;
    // The shape tells the engine apart, not the rule or the binary name.
    let raw = if obj.get("Actor").is_some_and(Value::is_object) {
        raw_docker(obj)?
    } else {
        raw_podman(obj)?
    };
    let kind = map_kind(&raw)?;
    let image = if kind == EventKind::ImageBuilt {
        raw.name.clone()
    } else {
        raw.image
    };
    Some(EngineEvent {
        engine: raw.engine,
        kind,
        id: raw.id,
        name: raw.name,
        image,
        labels: raw.labels,
        exit_code: if kind == EventKind::Stopped {
            raw.exit_code
        } else {
            None
        },
        time_ms: raw.time_ms,
    })
}

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod tests;
```

2. Create `src-tauri/crates/trigger-docker/src/logic/fixtures.rs` with this content. The lines marked "captured" are verbatim from R01. The lines marked "hand made" are shaped like R01's description; do not "correct" them.

```rust
//! Lines the engines print, used by the tests in `tests.rs`.
//!
//! The Docker lines marked "captured" are verbatim output of
//! `docker events --format '{{json .}}'` from Docker Desktop, Engine 29.4.3
//! (research note R01). Lines marked "hand made" follow the same shape but
//! were written by hand. Nothing here comes from a user's machine.

/// Captured: `docker run` of an image with a health check.
pub const DOCKER_START: &str = r#"{"Type":"container","Action":"start","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","com.example.team":"x","desktop.docker.io/ports.scheme":"v2","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837794,"timeNano":1791837794607003258}"#;

/// Captured.
pub const DOCKER_UNHEALTHY: &str = r#"{"Type":"container","Action":"health_status: unhealthy","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","com.example.team":"x","desktop.docker.io/ports.scheme":"v2","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837795,"timeNano":1791837795647713133}"#;

/// Captured.
pub const DOCKER_HEALTHY: &str = r#"{"Type":"container","Action":"health_status: healthy","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","com.example.team":"x","desktop.docker.io/ports.scheme":"v2","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837797,"timeNano":1791837797743493218}"#;

/// Captured: part of `docker stop -t 1`. Must be ignored.
pub const DOCKER_KILL: &str = r#"{"Type":"container","Action":"kill","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","com.example.team":"x","desktop.docker.io/ports.scheme":"v2","image":"r01test:1","name":"r01c","signal":"9"}},"scope":"local","time":1791837807,"timeNano":1791837807655361458}"#;

/// Captured: part of `docker stop`. Must be ignored, `die` is the event used.
pub const DOCKER_STOP: &str = r#"{"Type":"container","Action":"stop","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","com.example.team":"x","desktop.docker.io/ports.scheme":"v2","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837807,"timeNano":1791837807760686416}"#;

/// Captured: the container above ended after `docker stop`.
pub const DOCKER_DIE_137: &str = r#"{"Type":"container","Action":"die","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","com.example.team":"x","desktop.docker.io/ports.scheme":"v2","execDuration":"13","exitCode":"137","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837807,"timeNano":1791837807764166125}"#;

/// Captured: a container that exited by itself with code 3 (no `stop` event).
pub const DOCKER_DIE_3: &str = r#"{"Type":"container","Action":"die","Actor":{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Attributes":{"desktop.docker.io/ports.scheme":"v2","execDuration":"0","exitCode":"3","image":"alpine:3","name":"r01x"}},"scope":"local","time":1791837807,"timeNano":1791837807993939750}"#;

/// Hand made, shaped like R01's description of `exec_die` (one per health
/// probe). Has its own exitCode and execID and must not count as stopped.
pub const DOCKER_EXEC_DIE: &str = r#"{"Type":"container","Action":"exec_die","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","execID":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","exitCode":"1","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837795,"timeNano":1791837795100000000}"#;

/// Hand made. Must be ignored.
pub const DOCKER_EXEC_START: &str = r#"{"Type":"container","Action":"exec_start: test -f /ok","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"execID":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837795,"timeNano":1791837795090000000}"#;

/// Hand made. Must be ignored.
pub const DOCKER_CREATE: &str = r#"{"Type":"container","Action":"create","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837794,"timeNano":1791837794500000000}"#;

/// Hand made. Must be ignored.
pub const DOCKER_DESTROY: &str = r#"{"Type":"container","Action":"destroy","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837808,"timeNano":1791837808000000000}"#;

/// Hand made. Not a container event at all: must be ignored.
pub const DOCKER_NETWORK_CONNECT: &str = r#"{"Type":"network","Action":"connect","Actor":{"ID":"7c1e0f5a2b3d4c5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6","Attributes":{"container":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","name":"bridge","type":"bridge"}},"scope":"local","time":1791837794,"timeNano":1791837794550000000}"#;

/// Hand made, a verdict R01 says may exist: `starting` must be ignored.
pub const DOCKER_HEALTH_STARTING: &str = r#"{"Type":"container","Action":"health_status: starting","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837794,"timeNano":1791837794700000000}"#;
```

3. Replace the whole of `src-tauri/crates/trigger-docker/src/logic/tests.rs` with this content.

```rust
use super::fixtures::*;
use super::*;
use serde_json::json;
use trigger_core::testkit::rule;

const ID_R01C: &str = "ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe";

fn labels(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn r01c_labels() -> BTreeMap<String, String> {
    labels(&[
        ("com.example.k", "v"),
        ("com.example.team", "x"),
        ("desktop.docker.io/ports.scheme", "v2"),
    ])
}

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

#[test]
fn every_event_kind_id_is_a_declared_kind() {
    for kind in [
        EventKind::Started,
        EventKind::Stopped,
        EventKind::Healthy,
        EventKind::Unhealthy,
        EventKind::ImageBuilt,
    ] {
        assert!(crate::KINDS.iter().any(|k| k.id == kind.id()), "{kind:?}");
    }
}

#[test]
fn docker_start() {
    let e = parse_line(DOCKER_START).unwrap();
    assert_eq!(
        e,
        EngineEvent {
            engine: Engine::Docker,
            kind: EventKind::Started,
            id: ID_R01C.into(),
            name: "r01c".into(),
            image: "r01test:1".into(),
            labels: r01c_labels(),
            exit_code: None,
            time_ms: 1_791_837_794_607,
        }
    );
}

#[test]
fn docker_die_carries_the_exit_code_and_keeps_it_out_of_the_labels() {
    let e = parse_line(DOCKER_DIE_137).unwrap();
    assert_eq!(e.kind, EventKind::Stopped);
    assert_eq!(e.exit_code, Some(137));
    assert_eq!(e.labels, r01c_labels());
    assert_eq!(e.time_ms, 1_791_837_807_764);

    let e = parse_line(DOCKER_DIE_3).unwrap();
    assert_eq!(e.kind, EventKind::Stopped);
    assert_eq!(e.exit_code, Some(3));
    assert_eq!(e.name, "r01x");
    assert_eq!(e.image, "alpine:3");
    assert_eq!(
        e.labels,
        labels(&[("desktop.docker.io/ports.scheme", "v2")])
    );
}

#[test]
fn docker_health_verdicts() {
    let healthy = parse_line(DOCKER_HEALTHY).unwrap();
    assert_eq!(healthy.kind, EventKind::Healthy);
    assert_eq!(healthy.name, "r01c");
    let unhealthy = parse_line(DOCKER_UNHEALTHY).unwrap();
    assert_eq!(unhealthy.kind, EventKind::Unhealthy);
    assert_eq!(unhealthy.exit_code, None);
}

#[test]
fn a_health_verdict_without_the_space_still_counts() {
    let line = DOCKER_HEALTHY.replace("health_status: healthy", "health_status:healthy");
    assert_eq!(parse_line(&line).unwrap().kind, EventKind::Healthy);
}

#[test]
fn the_health_verdict_starting_is_ignored() {
    assert_eq!(parse_line(DOCKER_HEALTH_STARTING), None);
}

#[test]
fn an_exit_code_printed_as_a_number_is_read_too() {
    let line = DOCKER_DIE_3.replace(r#""exitCode":"3""#, r#""exitCode":3"#);
    assert_eq!(parse_line(&line).unwrap().exit_code, Some(3));
}

#[test]
fn the_time_falls_back_to_seconds_without_timenano() {
    let line = DOCKER_START.replace(r#","timeNano":1791837794607003258"#, "");
    assert_eq!(parse_line(&line).unwrap().time_ms, 1_791_837_794_000);
    let neither = line.replace(r#","time":1791837794"#, "");
    assert_eq!(parse_line(&neither).unwrap().time_ms, 0);
}

#[test]
fn lines_that_are_not_events_give_none() {
    for line in [
        "",
        "   ",
        "not json",
        "[]",
        "42",
        "null",
        "{}",
        r#"{"Type":"container"}"#,
        r#"{"Type":"container","Action":"start"}"#,
        r#"{"Type":"container","Action":"start","Actor":"x"}"#,
        r#"{"Type":"container","Action":7,"Actor":{"ID":"a"}}"#,
    ] {
        assert_eq!(parse_line(line), None, "{line}");
    }
}

#[test]
fn a_line_with_a_trailing_newline_parses() {
    let line = format!("{DOCKER_START}\r\n");
    assert!(parse_line(&line).is_some());
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-docker`.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker` passes: the library shows `19 passed`
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (it may print warnings for `src/lib.rs` and `src/os.rs` still having template code (both contain the text `not implemented yet`, which slice C removes); those are expected until slice C)
- [ ] `grep -c "fn parse_line" src-tauri/crates/trigger-docker/src/logic.rs` prints `1`
- [ ] `wc -l src-tauri/crates/trigger-docker/src/logic.rs` prints a number under 400
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Mapping Docker `image` events: they give `None` for now (T04).
- Filling in `raw_podman`: it stays a stub returning `None` (T05).
- The matcher and the payload (T06, T07).
- Reading the `engine` rule field; `parse_line` takes only the line.
- Adding fields to `EngineEvent` or `Raw`, or changing the NOT_LABELS list.
- Matching actions with `starts_with` or `contains`; whole strings only.
- Adding a dependency.

## If stuck

- If a test fails, compare your code with the card character by character. Do not change a test or a fixture line to make it pass.
- If clippy reports `dead_code` in a file other than the ones named in this card, a `#![allow(dead_code)]` line from an earlier card is missing or was moved. Check `src/logic.rs`, `src/rule.rs` and `src/os.rs` each have it directly after their `//!` lines.
- If a fixture line looks wrong to you, it is not: R01 captured it from a real Docker. Leave it.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.

- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
