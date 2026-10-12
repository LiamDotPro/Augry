---
id: T24
epic: 010-trigger-docker
title: Wire start to the supervisors and remove the stubs
status: todo
depends_on: [T23]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-docker/src/lib.rs
  - src-tauri/crates/trigger-docker/src/logic.rs
  - src-tauri/crates/trigger-docker/src/rule.rs
  - src-tauri/crates/trigger-docker/src/os.rs
  - src-tauri/crates/trigger-docker/src/supervisor.rs
  - src-tauri/crates/trigger-docker/src/plan.rs
---

## Goal

`DockerSource::start` works. It parses the rules, plans the engines with `plan::plan` and `os::probe`, starts one `Supervisor` per engine over a `RealEngine`, turns each fired rule into a `trigger_core::Event` on the sink, and returns a `Handle` that stops every supervisor, which kills each child. Every temporary `#![allow(dead_code)]` in the crate is gone, and with it the "not implemented yet" text.

## Read first

- `plans/010-trigger-docker/epic.md` Decision 8 (what `start` does) and Decision 9
- `src-tauri/crates/trigger-docker/src/lib.rs:130-166` the `Source` impl and its stub `start`
- `src-tauri/crates/trigger-docker/src/supervisor.rs` `Supervisor::start`, `Emit`, `real_pause`, `Supervisor::stop`
- `src-tauri/crates/trigger-docker/src/plan.rs` `plan` and `Group`
- `src-tauri/crates/trigger-core/src/lib.rs:139-149` `Event` and `Sink`; `:168-182` `Handle`
- `src-tauri/crates/trigger-fs/src/lib.rs:164-190` how the reference source builds its `Handle`

## Steps

1. Remove the temporary allow in five files. In each of these files delete the two lines `// Removed when ...` and `#![allow(dead_code)]` (the comment is the line directly above the attribute) and the one blank line after them:

- `src-tauri/crates/trigger-docker/src/logic.rs`
- `src-tauri/crates/trigger-docker/src/rule.rs`
- `src-tauri/crates/trigger-docker/src/os.rs`
- `src-tauri/crates/trigger-docker/src/supervisor.rs`
- `src-tauri/crates/trigger-docker/src/plan.rs`

Then `grep -rn "allow(dead_code)" src-tauri/crates/trigger-docker/src` must print nothing.

2. In `src-tauri/crates/trigger-docker/src/lib.rs` replace this import

```rust
use trigger_core::{
    Availability, FieldInfo, Handle, KindInfo, Platforms, Rule, RuleError, Sink, Source,
    SourceError,
};
```

with

```rust
use std::sync::Arc;
use std::time::SystemTime;

use supervisor::{real_pause, Emit, Supervisor};
use trigger_core::{
    Availability, Event, FieldInfo, Handle, KindInfo, Platforms, Rule, RuleError, Sink, Source,
    SourceError,
};
```

3. In the same file, delete everything from the line `    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError> {` to the end of the file (that removes the stub, its `not implemented yet` error and the closing brace of the `impl`), and put this in its place. It ends the `impl` and adds three small tests. The version check can take five seconds per engine, which is fine: `start` runs on the hub's thread, not the UI thread.

```rust
    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError> {
        let armed = rules
            .iter()
            .map(|r| {
                rule::parse(r).map(|when| logic::Armed {
                    id: r.id.clone(),
                    kind: r.kind.clone(),
                    when,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| SourceError::Os(e.to_string()))?;
        // Asks each engine to answer `version`, which can take five seconds.
        let groups = plan::plan(armed, &os::probe).map_err(SourceError::Unavailable)?;
        let emit: Emit = Arc::new(move |rule, kind, payload| {
            let event = Event {
                rule,
                kind,
                at: SystemTime::now(),
                payload,
            };
            // The receiver is gone while the app shuts down.
            let _ = sink.send(event);
        });
        let mut running = Vec::new();
        for group in groups {
            let engine = group.engine;
            let process = Arc::new(os::RealEngine::new(engine, group.path));
            let supervisor =
                Supervisor::start(process, group.rules, emit.clone(), Arc::new(real_pause))
                    .map_err(|e| {
                        SourceError::Os(format!(
                            "could not start the {} reader: {e}",
                            engine.as_str()
                        ))
                    })?;
            running.push(supervisor);
        }
        // Dropping the handle stops every supervisor, which kills its child.
        Ok(Handle::new(move || {
            for supervisor in running {
                supervisor.stop();
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::mpsc::channel;
    use trigger_core::testkit::rule;

    #[test]
    fn every_kind_belongs_to_the_docker_source() {
        assert!(KINDS.iter().all(|k| k.id.starts_with("docker.")));
        assert_eq!(DockerSource.id(), "docker");
    }

    #[test]
    fn start_with_no_rules_runs_nothing_and_stops_cleanly() {
        let (tx, _rx) = channel();
        let handle = DockerSource.start(Vec::new(), tx).unwrap();
        drop(handle);
    }

    #[test]
    fn start_rejects_a_rule_of_another_source() {
        let (tx, _rx) = channel();
        let other = rule("p/t", "fs.file-saved", json!({}));
        let err = DockerSource.start(vec![other], tx).err().unwrap();
        assert!(matches!(err, SourceError::Os(_)));
    }
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-docker`, then the acceptance commands.

## Acceptance

- [ ] `grep -rn "allow(dead_code)" src-tauri/crates/trigger-docker/src` prints nothing
- [ ] `grep -c "not implemented yet" src-tauri/crates/trigger-docker/src/lib.rs src-tauri/crates/trigger-docker/src/os.rs` prints `0` after each of the two file names (grep exits 1 for a count of 0; that is expected)
- [ ] `cd src-tauri && cargo test -p trigger-docker --lib -- every_kind_belongs_to_the_docker_source start_with_no_rules_runs_nothing_and_stops_cleanly start_rejects_a_rule_of_another_source` prints `3 passed`
- [ ] `cd src-tauri && cargo test -p trigger-docker` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (README warnings about template text may remain until T27)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/010-trigger-docker/tasks/`

## Out of scope

- The README (T27) and the live tests (T25, T26).
- Registering the crate in the app: the feature in `src-tauri/Cargo.toml` and the `sources()` line are slice D.
- Retrying a failed `start`, caching probe results between calls, or probing in the background. `start` probes every time it runs.
- Adding a new `#[allow]` anywhere to silence a warning.
- Changing `supervisor.rs`, `plan.rs` or `os.rs` beyond deleting the allow lines.

## If stuck

- If clippy reports `dead_code` for an item, an earlier card left something nothing uses. Do not add an allow and do not delete the item: set status blocked and name the item in the Log.
- If `cargo test` shows a failure in code from another card, do not touch it: set status blocked and name the test.
- If the `impl Source for DockerSource` block does not end with `start` as the last method, set status blocked and say what follows it.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
