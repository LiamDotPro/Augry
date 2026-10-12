---
id: T34
epic: 001-trigger-core
title: Add the status types and the fired payload
status: todo
depends_on: [T33]
size: M
model: haiku
files:
  - src-tauri/src/triggers/mod.rs
  - src-tauri/src/triggers/status.rs
---

## Goal

`src-tauri/src/triggers/status.rs` holds the data the app sends the frontend: the triggers status (`TriggersStatus` and the types under it), the `trigger://fired` payload (`FiredPayload`), and the one log line per fire. All are pure and have tests that pin the exact JSON.

## Read first

- `plans/001-trigger-core/epic.md` Decisions 14 to 16 and the "Commands" table in the Shape
- `src-tauri/src/triggers/rules.rs` `Plan`, `PlannedProcess`, `PlannedRule` and `process_of` (T33)
- `src-tauri/crates/trigger-hub/src/lib.rs` `Fired` and `RuleState`
- `plans/_context/conventions.md` the sentence about log lines

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance.

2. In `src-tauri/src/triggers/mod.rs` add this line, keeping the `pub mod` lines in alphabetical order (it goes between `pub mod rules;` and `pub mod store;`):

```rust
pub mod status;
```

3. Create `src-tauri/src/triggers/status.rs` with exactly this content. `build_status` joins a `Plan` with the answer of `Hub::apply`; a rule that the plan skipped shows as `skipped`, a rule the hub did not answer for shows as `rejected`. `RuleStatus` flattens its state, so the JSON is `{ "id", "kind", "state", "reason"? }`. The log line holds ids and the kind only, never the payload, because the payload has paths from the user's disk.

```rust
//! What the app tells the frontend about triggers: the status of every
//! process and the payload of the `trigger://fired` event. Pure, so the JSON
//! the frontend sees is pinned by tests. Design: plans/001-trigger-core/epic.md
//! Decisions 15 and 16.

use std::collections::BTreeMap;
use std::time::UNIX_EPOCH;

use serde::Serialize;
use serde_json::Value;
use trigger_core::RuleId;
use trigger_hub::{Fired, RuleState};

use super::rules::{process_of, Plan};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggersStatus {
    pub enabled: bool,
    pub processes: Vec<ProcessStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessStatus {
    pub id: String,
    pub name: String,
    pub armed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    pub rules: Vec<RuleStatus>,
}

/// Serialises as `{ "id", "kind", "state", "reason"? }`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleStatus {
    pub id: RuleId,
    pub kind: String,
    #[serde(flatten)]
    pub state: RuleStatusState,
}

/// The hub's three states plus `Skipped`, for a rule that was never given to
/// the hub because a variable had no value.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum RuleStatusState {
    Active,
    Rejected { reason: String },
    SourceFailed { reason: String },
    Skipped { reason: String },
}

/// Joins the plan with what `Hub::apply` answered.
pub fn build_status(
    enabled: bool,
    plan: &Plan,
    states: &BTreeMap<RuleId, RuleState>,
) -> TriggersStatus {
    let processes = plan
        .processes
        .iter()
        .map(|process| ProcessStatus {
            id: process.id.clone(),
            name: process.name.clone(),
            armed: process.armed,
            problem: process.problem.clone(),
            rules: process
                .rules
                .iter()
                .map(|rule| RuleStatus {
                    id: rule.id.clone(),
                    kind: rule.kind.clone(),
                    state: match (&rule.skipped, states.get(&rule.id)) {
                        (Some(reason), _) => RuleStatusState::Skipped {
                            reason: reason.clone(),
                        },
                        (None, Some(RuleState::Active)) => RuleStatusState::Active,
                        (None, Some(RuleState::Rejected { reason })) => RuleStatusState::Rejected {
                            reason: reason.clone(),
                        },
                        (None, Some(RuleState::SourceFailed { reason })) => {
                            RuleStatusState::SourceFailed {
                                reason: reason.clone(),
                            }
                        }
                        (None, None) => RuleStatusState::Rejected {
                            reason: "the rule was not applied".into(),
                        },
                    },
                })
                .collect(),
        })
        .collect();
    TriggersStatus { enabled, processes }
}

/// The payload of `trigger://fired`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FiredPayload {
    pub process_id: String,
    pub process_name: String,
    pub rule_id: RuleId,
    pub kind: String,
    /// Milliseconds since the Unix epoch.
    pub at: i64,
    pub payload: BTreeMap<String, Value>,
    pub coalesced: u32,
}

/// `names` maps a process id to its display name; an unknown process shows
/// its id.
pub fn fired_payload(fired: &Fired, names: &BTreeMap<String, String>) -> FiredPayload {
    let process_id = process_of(&fired.event.rule).to_string();
    let process_name = names
        .get(&process_id)
        .cloned()
        .unwrap_or_else(|| process_id.clone());
    let at = fired
        .event
        .at
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    FiredPayload {
        process_id,
        process_name,
        rule_id: fired.event.rule.clone(),
        kind: fired.event.kind.clone(),
        at,
        payload: fired.event.payload.clone(),
        coalesced: fired.coalesced,
    }
}

/// The one log line per fire. It carries ids and the kind only, never the
/// payload, which holds paths from the user's disk.
pub fn log_line(payload: &FiredPayload) -> String {
    format!(
        "terminal-grid: trigger fired process={} rule={} kind={}",
        payload.process_id, payload.rule_id, payload.kind
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triggers::rules::{PlannedProcess, PlannedRule};
    use serde_json::json;
    use std::time::Duration;
    use trigger_core::Event;

    fn plan() -> Plan {
        Plan {
            rules: Vec::new(),
            processes: vec![PlannedProcess {
                id: "tidy".into(),
                name: "Keep the docs tidy".into(),
                armed: true,
                problem: None,
                rules: vec![
                    PlannedRule {
                        id: "tidy/a".into(),
                        kind: "fs.file-saved".into(),
                        skipped: None,
                    },
                    PlannedRule {
                        id: "tidy/b".into(),
                        kind: "fs.file-saved".into(),
                        skipped: None,
                    },
                    PlannedRule {
                        id: "tidy/c".into(),
                        kind: "fs.file-saved".into(),
                        skipped: Some("no downloads folder".into()),
                    },
                ],
            }],
        }
    }

    #[test]
    fn status_joins_the_plan_with_the_hub_answer() {
        let mut states = BTreeMap::new();
        states.insert("tidy/a".to_string(), RuleState::Active);
        states.insert(
            "tidy/b".to_string(),
            RuleState::SourceFailed {
                reason: "no access".into(),
            },
        );
        let status = build_status(true, &plan(), &states);
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(
            json,
            json!({
                "enabled": true,
                "processes": [{
                    "id": "tidy",
                    "name": "Keep the docs tidy",
                    "armed": true,
                    "rules": [
                        { "id": "tidy/a", "kind": "fs.file-saved", "state": "active" },
                        { "id": "tidy/b", "kind": "fs.file-saved", "state": "sourceFailed", "reason": "no access" },
                        { "id": "tidy/c", "kind": "fs.file-saved", "state": "skipped", "reason": "no downloads folder" }
                    ]
                }]
            })
        );
    }

    #[test]
    fn a_problem_is_serialised_only_when_there() {
        let mut plan = plan();
        plan.processes[0].problem = Some("name is missing".into());
        let json = serde_json::to_value(build_status(true, &plan, &BTreeMap::new())).unwrap();
        assert_eq!(json["processes"][0]["problem"], json!("name is missing"));
    }

    fn fired() -> Fired {
        let mut payload = BTreeMap::new();
        payload.insert("path".to_string(), json!("/secret/place/a.md"));
        Fired {
            event: Event {
                rule: "tidy/saved#2".into(),
                kind: "fs.file-saved".into(),
                at: UNIX_EPOCH + Duration::from_millis(1_760_000_000_123),
                payload,
            },
            coalesced: 3,
        }
    }

    #[test]
    fn the_fired_payload_has_the_shape_the_frontend_reads() {
        let mut names = BTreeMap::new();
        names.insert("tidy".to_string(), "Keep the docs tidy".to_string());
        let json = serde_json::to_value(fired_payload(&fired(), &names)).unwrap();
        assert_eq!(
            json,
            json!({
                "processId": "tidy",
                "processName": "Keep the docs tidy",
                "ruleId": "tidy/saved#2",
                "kind": "fs.file-saved",
                "at": 1_760_000_000_123_i64,
                "payload": { "path": "/secret/place/a.md" },
                "coalesced": 3
            })
        );
    }

    #[test]
    fn an_unknown_process_shows_its_id() {
        let payload = fired_payload(&fired(), &BTreeMap::new());
        assert_eq!(payload.process_name, "tidy");
    }

    #[test]
    fn the_log_line_has_no_payload_values() {
        let line = log_line(&fired_payload(&fired(), &BTreeMap::new()));
        assert_eq!(
            line,
            "terminal-grid: trigger fired process=tidy rule=tidy/saved#2 kind=fs.file-saved"
        );
        assert!(!line.contains("secret"));
    }
}
```

4. Run `cd src-tauri && cargo fmt` and then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::status` shows 5 tests, all ok
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-core -p trigger-hub -p trigger-fs --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Emitting the event or calling `build_status`. T35 does.
- Adding fields to the payload beyond the seven in the epic.
- Changing how the hub names its states. This card maps them, it does not rename them.

## If stuck

- If `trigger_hub::Fired` or `RuleState` cannot be imported from the crate root, open `src-tauri/crates/trigger-hub/src/lib.rs`; if they are in a module, set status blocked and quote the path. Do not change the crate.
- If a `RuleState` variant differs from `Active`, `Rejected { reason }`, `SourceFailed { reason }`, set status blocked and quote the enum.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
