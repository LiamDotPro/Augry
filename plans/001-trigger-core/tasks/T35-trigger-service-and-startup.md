---
id: T35
epic: 001-trigger-core
title: Add TriggerService and start it with the app
status: todo
depends_on: [T34]
size: M
model: haiku
files:
  - src-tauri/src/triggers/mod.rs
  - src-tauri/src/lib.rs
---

## Goal

`TriggerService` exists in `AppState`: it owns the hub, works out the rules on `refresh`, gives them to the hub only when they changed, and forwards each fired trigger as a `trigger://fired` event with one log line. It starts with the app and refreshes once. The commands and the refresh calls on pane changes come in T36.

## Read first

- `plans/001-trigger-core/epic.md` Decisions 13, 14 and the Risks entry about `refresh()` being cheap
- `src-tauri/src/lib.rs:36-110` `AppState` and the `setup` closure
- `src-tauri/src/agent/mod.rs:250-290` how an event is emitted to the `main` window
- `src-tauri/src/git/mod.rs:157-175` `GitService::info`, which `repo_roots` calls
- `src-tauri/crates/trigger-fs/src/lib.rs` the `FsSource` unit struct

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance.

2. Replace the whole of `src-tauri/src/triggers/mod.rs` with this. Notes on how it works: `refresh` reads the definition files only while triggers are switched on; it compares the planned rules with the ones last given to the hub and calls `Hub::apply` only when they differ; `start_forwarding` takes a closure so a test can stand in for the window; `sources()` is the one place a new source crate is registered; `refresh_now` skips the git lookups while the feature is off.

```rust
//! Process triggers in the app: the glue between the local process
//! definitions, the open panes and `trigger-hub`. The rules themselves are
//! worked out by pure code in `rules.rs`, `variables.rs` and `store.rs`.
//! Design: plans/001-trigger-core/epic.md Decisions 10 to 16.

pub mod rules;
pub mod status;
pub mod store;
pub mod variables;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use trigger_core::{Rule, RuleError, RuleId, Source};
use trigger_hub::{Hub, RuleState, SourceInfo};

use crate::state::TriggersConfig;
use status::{FiredPayload, TriggersStatus};
use variables::Context;

pub const EVENT_FIRED: &str = "trigger://fired";

/// How stale a cached git answer may be when working out repository roots.
const GIT_MAX_AGE: Duration = Duration::from_secs(15);

/// The sources this build runs. A new source crate adds one line here, behind
/// its own cargo feature.
#[allow(clippy::vec_init_then_push)]
fn sources() -> Vec<Box<dyn Source>> {
    // With every source feature off the list stays empty, and `mut` is unused.
    #[allow(unused_mut)]
    let mut list: Vec<Box<dyn Source>> = Vec::new();
    #[cfg(feature = "trigger-fs")]
    list.push(Box::new(trigger_fs::FsSource));
    list
}

pub struct TriggerService {
    hub: Hub,
    /// `<app config dir>/processes`.
    dir: PathBuf,
    /// Process id to display name, shared with the forward thread.
    names: Arc<Mutex<BTreeMap<String, String>>>,
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// The rules last given to the hub; None before the first refresh.
    applied: Option<Vec<Rule>>,
    /// What the hub answered for `applied`.
    states: BTreeMap<RuleId, RuleState>,
    /// How many times `Hub::apply` was called.
    #[cfg(test)]
    applies: usize,
}

impl TriggerService {
    pub fn new(dir: PathBuf) -> TriggerService {
        TriggerService::with_hub(dir, Hub::new(sources()))
    }

    fn with_hub(dir: PathBuf, hub: Hub) -> TriggerService {
        TriggerService {
            hub,
            dir,
            names: Arc::new(Mutex::new(BTreeMap::new())),
            inner: Mutex::new(Inner::default()),
        }
    }

    pub fn catalogue(&self) -> Vec<SourceInfo> {
        self.hub.catalogue()
    }

    pub fn check(&self, rule: &Rule) -> std::result::Result<(), RuleError> {
        self.hub.check(rule)
    }

    /// Works out the rules from the config, the definition files and the open
    /// panes, gives them to the hub only when they differ from the last ones,
    /// and returns the status the Settings list shows. Cheap when nothing
    /// changed, so it is called after every pane change.
    pub fn refresh(&self, config: &TriggersConfig, ctx: &Context) -> TriggersStatus {
        let mut inner = self.inner.lock().expect("triggers lock");
        let defs = if config.enabled {
            store::load(&self.dir)
        } else {
            Vec::new()
        };
        let plan = rules::plan(&defs, config, ctx);
        *self.names.lock().expect("triggers names") = plan
            .processes
            .iter()
            .map(|process| (process.id.clone(), process.name.clone()))
            .collect();
        if inner.applied.as_ref() != Some(&plan.rules) {
            inner.states = self.hub.apply(plan.rules.clone());
            inner.applied = Some(plan.rules.clone());
            #[cfg(test)]
            {
                inner.applies += 1;
            }
        }
        status::build_status(config.enabled, &plan, &inner.states)
    }

    /// Starts the thread that turns each fired trigger into a payload for
    /// `emit` and one log line. It ends when the hub is dropped.
    pub fn start_forwarding(&self, emit: impl Fn(FiredPayload) + Send + 'static) {
        let rx = self.hub.subscribe();
        let names = Arc::clone(&self.names);
        let spawned = std::thread::Builder::new()
            .name("trigger-forward".into())
            .spawn(move || {
                for fired in rx {
                    let payload = {
                        let names = names.lock().expect("triggers names");
                        status::fired_payload(&fired, &names)
                    };
                    eprintln!("{}", status::log_line(&payload));
                    emit(payload);
                }
            });
        if let Err(error) = spawned {
            eprintln!("terminal-grid: trigger forwarding unavailable: {error}");
        }
    }
}

/// Starts forwarding fired triggers to the window as `trigger://fired`, then
/// applies the saved rules. Called once from the app setup.
pub fn start(app: &AppHandle) {
    let state = app.state::<crate::AppState>();
    let handle = app.clone();
    state.triggers.start_forwarding(move |payload| {
        let _ = handle.emit_to("main", EVENT_FIRED, payload);
    });
    refresh_app(app);
}

/// Repository roots of the open panes, from the git cache where it is fresh.
fn repo_roots(state: &crate::AppState) -> Vec<String> {
    let open_dirs = state.pty.open_dirs();
    open_dirs
        .iter()
        .filter_map(|(_, cwd)| state.git.info(cwd, GIT_MAX_AGE, &open_dirs).repo_root)
        .collect()
}

/// Collects the inputs from the app state and runs `TriggerService::refresh`.
/// May run `git`, so call it from a blocking thread.
pub fn refresh_now(state: &crate::AppState) -> TriggersStatus {
    let config = state.state.config().triggers;
    // Nobody pays for git lookups while the feature is off.
    let roots = if config.enabled {
        repo_roots(state)
    } else {
        Vec::new()
    };
    let ctx = Context::detect(state.notes.root(), roots);
    state.triggers.refresh(&config, &ctx)
}

/// `refresh_now` on a blocking thread, for callers that must not wait: pane
/// created or closed, working directory changed, config saved.
pub fn refresh_app(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<crate::AppState>();
        refresh_now(&state);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::mpsc::channel;
    use std::time::SystemTime;
    use trigger_core::Event;

    const DEFINITION: &str = r#"{"schemaVersion":1,"name":"Tidy","triggers":[
        {"id":"saved","kind":"fs.file-saved","when":{"path":"/somewhere"}}]}"#;

    fn ctx() -> Context {
        Context {
            home: PathBuf::from("/home/me"),
            downloads: None,
            notes_root: PathBuf::from("/notes"),
            repo_roots: Vec::new(),
            screenshots: || None,
        }
    }

    fn config(armed: &[&str]) -> TriggersConfig {
        TriggersConfig {
            enabled: true,
            armed: armed.iter().map(|a| a.to_string()).collect(),
        }
    }

    fn service() -> (TriggerService, PathBuf) {
        let dir = std::env::temp_dir().join(format!("tg-triggers-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tidy.json"), DEFINITION).unwrap();
        (
            TriggerService::with_hub(dir.clone(), Hub::new(Vec::new())),
            dir,
        )
    }

    fn cleanup(dir: &Path) {
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn refresh_applies_only_when_the_rules_change() {
        let (service, dir) = service();
        service.refresh(&config(&["tidy"]), &ctx());
        service.refresh(&config(&["tidy"]), &ctx());
        assert_eq!(service.inner.lock().unwrap().applies, 1);

        service.refresh(&config(&[]), &ctx());
        assert_eq!(service.inner.lock().unwrap().applies, 2);
        cleanup(&dir);
    }

    #[test]
    fn refresh_reports_the_hub_answer() {
        // A hub with no sources rejects every kind, which shows the answer
        // reaches the status.
        let (service, dir) = service();
        let status = service.refresh(&config(&["tidy"]), &ctx());
        assert!(status.enabled);
        assert_eq!(status.processes.len(), 1);
        assert_eq!(status.processes[0].name, "Tidy");
        assert!(status.processes[0].armed);
        assert_eq!(status.processes[0].rules.len(), 1);
        assert!(matches!(
            status.processes[0].rules[0].state,
            status::RuleStatusState::Rejected { .. }
        ));
        cleanup(&dir);
    }

    #[test]
    fn switched_off_lists_nothing_and_applies_no_rules() {
        let (service, dir) = service();
        let off = TriggersConfig {
            enabled: false,
            armed: vec!["tidy".into()],
        };
        let status = service.refresh(&off, &ctx());
        assert!(!status.enabled);
        assert!(status.processes.is_empty());
        assert_eq!(service.inner.lock().unwrap().applies, 1);
        cleanup(&dir);
    }

    #[test]
    fn a_published_event_is_forwarded_with_the_process_name() {
        let (service, dir) = service();
        service.refresh(&config(&["tidy"]), &ctx());
        let (tx, rx) = channel();
        service.start_forwarding(move |payload| {
            let _ = tx.send(payload);
        });
        service.hub.publish(Event {
            rule: "tidy/saved".into(),
            kind: "fs.file-saved".into(),
            at: SystemTime::now(),
            payload: BTreeMap::new(),
        });
        let payload = rx.recv_timeout(Duration::from_secs(5)).expect("a payload");
        assert_eq!(payload.process_id, "tidy");
        assert_eq!(payload.process_name, "Tidy");
        assert_eq!(payload.rule_id, "tidy/saved");
        assert_eq!(payload.coalesced, 0);
        cleanup(&dir);
    }
}
```

3. In `src-tauri/src/lib.rs`, add the field to `AppState`. Find:

```rust
    pub state: state::StateService,
}
```

and replace it with:

```rust
    pub state: state::StateService,
    pub triggers: triggers::TriggerService,
}
```

4. In the same file, in the `setup` closure, find the line `let pty = pty::PtyManager::new(config_dir);` and add this line directly above it (the next line moves `config_dir`, so the new line must come first):

```rust
            let triggers = triggers::TriggerService::new(config_dir.join("processes"));
```

5. In the same file find `                state: state_service,` inside the `AppState {` literal and add a line after it:

```rust
                triggers,
```

6. In the same file find the line `            start_git_sweep(handle.clone());` and add a line after it:

```rust
            triggers::start(&handle);
```

7. Run `cd src-tauri && cargo fmt` and then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::tests` shows 4 tests, all ok
- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::` shows 35 tests, all ok
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-core -p trigger-hub -p trigger-fs --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- The four commands, the calls to `refresh_app` from the terminal and config code, and `types.ts`. T36 does them.
- Giving the service a `publish` method for triggers from inside the app. Epic 002 adds it.
- Changing the cool down or the hub thread.
- Running real file watchers in a test. The tests use a hub with no sources on purpose.
- Fixing existing clippy warnings.

## If stuck

- If `Hub::new` does not take `Vec<Box<dyn Source>>`, or `FsSource` is not a unit struct, open `src-tauri/crates/trigger-hub/src/lib.rs` or `src-tauri/crates/trigger-fs/src/lib.rs`, set status blocked and quote the real signature. Do not change the crates.
- If clippy reports `vec_init_then_push` or `unused_mut` for `sources()`, the two `allow` lines in the code above were dropped; put them back as written.
- If the test `a_published_event_is_forwarded_with_the_process_name` times out, the hub may filter events by rule. Set status blocked and quote the hub's `publish` code.
- If `cargo check --no-default-features` fails with an unresolved `trigger_fs`, the `#[cfg(feature = "trigger-fs")]` line above `list.push` is missing.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
