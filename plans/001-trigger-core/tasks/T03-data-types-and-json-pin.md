---
id: T03
epic: 001-trigger-core
title: Add the data types and pin their JSON
status: done
depends_on: [T02]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-core/src/lib.rs
---

## Goal

`Permission`, `FieldInfo`, `KindInfo`, `Availability`, `RuleId`, `Rule`, `Event` and `Sink` exist, and a unit test pins the JSON the frontend will see.

## Read first

- `src-tauri/crates/trigger-core/src/lib.rs` the file you edit (T01, T02)
- `plans/001-trigger-core/research/R06-contract-compiles.md` the JSON shapes in "Evidence"

## Steps

1. Directly under the `//!` doc comment at the top of `lib.rs` (above `/// Bit set of the platforms...`), add these imports and re-export, followed by a blank line:

```rust
use std::collections::BTreeMap;
use std::sync::mpsc::Sender;
use std::time::SystemTime;

pub use serde_json::Value;
```

2. Insert the following after the `ms_since_epoch` module and before `#[cfg(test)] mod platforms_tests`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    Accessibility,
    ScreenRecording,
    Location,
    Calendar,
    Reminders,
    Folder(&'static str),
    Microphone,
    Camera,
    Notifications,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldInfo {
    pub name: &'static str,
    pub doc: &'static str,
    pub required: bool,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub platforms: Platforms,
    pub permission: Option<Permission>,
    pub when: &'static [FieldInfo],
    pub payload: &'static [FieldInfo],
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Availability {
    Available,
    Unsupported { reason: String },
    NeedsPermission { permission: Permission, how: String },
}

pub type RuleId = String;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rule {
    pub id: RuleId,
    pub kind: String,
    pub when: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub rule: RuleId,
    pub kind: String,
    #[serde(with = "ms_since_epoch")]
    pub at: SystemTime,
    pub payload: BTreeMap<String, Value>,
}

pub type Sink = Sender<Event>;
```

3. Append this test module at the very end of the file:

```rust
#[cfg(test)]
mod json_tests {
    use super::*;
    use serde_json::json;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn permission_json() {
        assert_eq!(serde_json::to_value(Permission::Accessibility).unwrap(), json!("accessibility"));
        assert_eq!(serde_json::to_value(Permission::ScreenRecording).unwrap(), json!("screenRecording"));
        assert_eq!(serde_json::to_value(Permission::Folder("Downloads")).unwrap(), json!({"folder": "Downloads"}));
    }

    #[test]
    fn event_at_is_a_number_of_milliseconds() {
        let event = Event {
            rule: "r".into(),
            kind: "k".into(),
            at: UNIX_EPOCH + Duration::from_millis(1_760_000_000_123),
            payload: BTreeMap::new(),
        };
        let v = serde_json::to_value(&event).unwrap();
        assert!(v["at"].is_number());
        assert_eq!(v, json!({"rule": "r", "kind": "k", "at": 1_760_000_000_123i64, "payload": {}}));
    }

    #[test]
    fn kind_info_json() {
        let kind = KindInfo {
            id: "fs.x",
            title: "T",
            summary: "s",
            platforms: Platforms::MACOS.or(Platforms::LINUX),
            permission: Some(Permission::Folder("Downloads")),
            when: &[FieldInfo { name: "path", doc: "d", required: true }],
            payload: &[],
        };
        assert_eq!(
            serde_json::to_value(kind).unwrap(),
            json!({
                "id": "fs.x", "title": "T", "summary": "s",
                "platforms": ["macos", "linux"],
                "permission": {"folder": "Downloads"},
                "when": [{"name": "path", "doc": "d", "required": true}],
                "payload": []
            })
        );
    }

    #[test]
    fn availability_json() {
        assert_eq!(serde_json::to_value(Availability::Available).unwrap(), json!({"state": "available"}));
        assert_eq!(
            serde_json::to_value(Availability::NeedsPermission {
                permission: Permission::Folder("Downloads"),
                how: "x".into()
            })
            .unwrap(),
            json!({"state": "needsPermission", "permission": {"folder": "Downloads"}, "how": "x"})
        );
    }

    #[test]
    fn rule_reads_from_json() {
        let rule: Rule = serde_json::from_value(json!({"id": "a", "kind": "fs.x", "when": {"path": "/tmp"}})).unwrap();
        assert_eq!(rule.when["path"], json!("/tmp"));
    }
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-core`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-core` passes (4 tests in `platforms_tests`, 5 in `json_tests`)
- [ ] `cd src-tauri && cargo clippy -p trigger-core --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-core --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Errors, `Handle` and the `Source` trait (T04).
- Adding `Deserialize` to `Event`, `KindInfo` or `Permission`.
- Changing field names, derives or serde attributes. The frontend contract depends on them.

## If stuck

- If a JSON test fails, do not edit the expected value. Compare the type definitions with step 2 exactly (attribute lines included).
- If the compiler says `Platforms: Serialize` is not satisfied, T02 is not in place. Set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
