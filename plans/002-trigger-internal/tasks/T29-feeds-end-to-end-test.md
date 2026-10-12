---
id: T29
epic: 002-trigger-internal
title: Test every app feed signal end to end
status: in-progress
depends_on: [T21, T22, T23, T24, T25, T26, T27, T28]
size: S
model: haiku
files:
  - src-tauri/src/triggers/feeds.rs
---

## Goal

One test in `feeds.rs` arms a rule for each of the nine kinds the app feeds from its own code, publishes a realistic signal for each through a real `TriggerService`, and checks that every rule fires with the documented payload field. The card also confirms by `grep` that every feed point from T21 to T28 is in place.

## Read first

- `plans/002-trigger-internal/epic.md` Decisions 3 and 12
- `src-tauri/src/triggers/feeds.rs` the builders and the `tests` module (T20)
- `src-tauri/src/triggers/mod.rs` the test `a_published_signal_fires_an_armed_tg_rule` (T08), whose shape this test copies
- `src-tauri/src/triggers/internal/logic.rs` `payload_of`, the payload keys the test reads (T05)

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance (it was 7 when this card was written).

2. All edits are in the `tests` module at the bottom of `src-tauri/src/triggers/feeds.rs`. Find the first line of the module body, `    use super::*;`, and put the following directly after it. How it works: the definition file has one trigger per kind with an empty `when`, so each rule matches any signal of its kind; the nine signals are published in a row; each fired payload comes back through the forwarding thread, and the test collects them by kind because their order is not fixed (each rule has its own cool down, so none is dropped).

```rust

    use crate::state::TriggersConfig;
    use crate::triggers::variables::Context;
    use crate::triggers::TriggerService;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    /// The nine kinds the app feeds from its own code (slice C).
    const FED_KINDS: [&str; 9] = [
        "tg.agent-finished",
        "tg.task-reported",
        "tg.pane-opened-in-repo",
        "tg.worktree-created",
        "tg.worktree-removed",
        "tg.review-staged",
        "tg.note-saved",
        "tg.focus-entered",
        "tg.app-started",
    ];

    fn every_fed_signal() -> Vec<InternalSignal> {
        vec![
            InternalSignal::AgentFinished {
                pane: "p1".into(),
                cwd: "/work/app".into(),
                agent: "claude".into(),
            },
            task_reported("p1", "/work/app", Some("Fixing tests")).unwrap(),
            InternalSignal::PaneOpened {
                pane: "p1".into(),
                cwd: "/work/app/src".into(),
                repo_root: Some("/work/app".into()),
            },
            InternalSignal::WorktreeCreated {
                repo: "/work/app".into(),
                path: "/work/app-x".into(),
                branch: Some("x".into()),
            },
            InternalSignal::WorktreeRemoved {
                repo: "/work/app".into(),
                path: "/work/app-x".into(),
            },
            review_staged("/work/app", 2),
            note_saved(Path::new("/notes"), "work/plan.md"),
            focus_entered(None, "p1", "/work/app").unwrap(),
            InternalSignal::AppStarted,
        ]
    }

    #[test]
    fn every_fed_signal_fires_its_rule_with_the_documented_payload() {
        let dir = std::env::temp_dir().join(format!("tg-feeds-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let triggers: Vec<_> = FED_KINDS
            .iter()
            .map(|kind| json!({ "id": kind.trim_start_matches("tg."), "kind": kind, "when": {} }))
            .collect();
        let definition = json!({ "schemaVersion": 1, "name": "Feeds", "triggers": triggers });
        std::fs::write(dir.join("feeds.json"), definition.to_string()).unwrap();

        let service = TriggerService::new(dir.clone());
        let config = TriggersConfig {
            enabled: true,
            armed: vec!["feeds".into()],
        };
        let ctx = Context {
            home: PathBuf::from("/home/me"),
            downloads: None,
            notes_root: PathBuf::from("/notes"),
            repo_roots: Vec::new(),
            screenshots: || None,
        };
        service.refresh(&config, &ctx);
        let (tx, rx) = channel();
        service.start_forwarding(move |payload| {
            let _ = tx.send(payload);
        });

        for signal in every_fed_signal() {
            service.publish_internal(signal);
        }

        let mut fired = BTreeMap::new();
        for _ in 0..FED_KINDS.len() {
            let got = rx
                .recv_timeout(Duration::from_secs(5))
                .expect("a fired trigger");
            fired.insert(got.kind.clone(), got.payload);
        }
        for kind in FED_KINDS {
            assert!(fired.contains_key(kind), "{kind} did not fire");
        }
        assert_eq!(fired["tg.agent-finished"]["agent"], json!("claude"));
        assert_eq!(fired["tg.task-reported"]["pane"], json!("p1"));
        assert_eq!(fired["tg.pane-opened-in-repo"]["repo"], json!("/work/app"));
        assert_eq!(fired["tg.worktree-created"]["branch"], json!("x"));
        assert_eq!(fired["tg.worktree-removed"]["path"], json!("/work/app-x"));
        assert_eq!(fired["tg.review-staged"]["files"], json!(2));
        assert_eq!(fired["tg.note-saved"]["relative"], json!("work/plan.md"));
        assert_eq!(fired["tg.focus-entered"]["cwd"], json!("/work/app"));
        assert!(fired["tg.app-started"].is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
```

3. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::feeds` passes and reports 6 passed
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/002-trigger-internal/tasks/`
- [ ] `grep -rn "feeds::publish\|publish_internal" src-tauri/src --include=*.rs | grep -v "^src-tauri/src/triggers/" | wc -l` prints at least `8` (agent finished, task reported, pane opened, worktree created, worktree removed, review staged, note saved, focus entered; slice B adds more)
- [ ] `grep -n "publish_internal(InternalSignal::AppStarted)" src-tauri/src/triggers/mod.rs` prints three lines: one in `start` and two in T28's test `app_started_fires_only_once_the_rules_are_armed`

## Out of scope

- Any change outside the `tests` module of `feeds.rs`.
- Tests for the filter fields (`repo`, `agent`, `glob`): `internal/logic/tests.rs` has them.
- Tests for the command and output feeds: slice B.
- Fixing existing clippy warnings, renaming anything, or changing a file not listed under `files:`.

## If stuck

- If an assertion fails with `no entry found for key`, a payload field name differs from `payload_of` in `internal/logic.rs`: quote the key and the payload, set `status: blocked`; do not change `logic.rs`.
- If a rule is rejected (the first `recv_timeout` times out), `sources()` does not include the `tg` source: T08 is not applied, set `status: blocked`.
- If `Context` has gained a field, add it to the struct literal with the value the other tests in `triggers/mod.rs` use, and note it in the Log.
- If `cargo fmt` changes the code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-13 haiku: built; clippy baseline 7, after 7. feeds 6 passed, workspace tests pass, fmt --check 0, check --no-default-features 0, feed call grep 9 (>=8). NOT PASSED: `grep -n "publish_internal(InternalSignal::AppStarted)" src-tauri/src/triggers/mod.rs` prints 3 lines (175 in start; 425 and 429 in the existing test `app_started_fires_only_once_the_rules_are_armed`), not one. Line 175 is the expected one; the fix would need mod.rs, which is outside files:, so left for the checker.
- 2026-10-13 architect: the AppStarted grep also matches T28's test (2 lines); corrected the expected count to 3. Sent to the checker.
