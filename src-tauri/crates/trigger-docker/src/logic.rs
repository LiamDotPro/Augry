//! Pure decisions: given an event the engine printed, which rules fire and with
//! what payload. No child processes, no threads, no clocks: every case is a
//! unit test.

use std::collections::BTreeMap;

use serde_json::Map;
use trigger_core::{RuleId, Value};

use crate::rule::{EngineChoice, Parsed};

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

fn map_kind(raw: &Raw) -> Option<EventKind> {
    match (raw.kind_type.as_str(), raw.action.as_str()) {
        ("container", "start") => Some(EventKind::Started),
        ("container", "die") => Some(EventKind::Stopped),
        ("image", "tag") if raw.engine == Engine::Docker => Some(EventKind::ImageBuilt),
        ("image", "build") if raw.engine == Engine::Podman => Some(EventKind::ImageBuilt),
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

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod match_tests;
#[cfg(test)]
mod tests;
