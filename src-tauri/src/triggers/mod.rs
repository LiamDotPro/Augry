//! Process triggers in the app: the glue between the local process
//! definitions, the open panes and `trigger-hub`. The rules themselves are
//! worked out by pure code in `rules.rs`, `variables.rs` and `store.rs`.
//! Design: plans/001-trigger-core/epic.md Decisions 10 to 16.

pub mod feeds;
pub mod internal;
pub mod rules;
pub mod status;
pub mod store;
pub mod variables;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use internal::{InternalSignal, InternalSource};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use trigger_core::{Rule, RuleError, RuleId, Source};
use trigger_hub::{Hub, RuleState, SourceInfo};

use crate::error::{AppError, Result};
use crate::state::TriggersConfig;
use status::{FiredPayload, TriggersStatus};
use variables::Context;

pub const EVENT_FIRED: &str = "trigger://fired";

/// How stale a cached git answer may be when working out repository roots.
const GIT_MAX_AGE: Duration = Duration::from_secs(15);

/// The sources this build runs. A new source crate adds one line here, behind
/// its own cargo feature. The internal source is always on: it is not a crate.
#[allow(clippy::vec_init_then_push)]
fn sources(internal: &InternalSource) -> Vec<Box<dyn Source>> {
    let mut list: Vec<Box<dyn Source>> = Vec::new();
    list.push(Box::new(internal.clone()));
    #[cfg(feature = "trigger-fs")]
    list.push(Box::new(trigger_fs::FsSource));
    list
}

pub struct TriggerService {
    hub: Hub,
    /// The `tg` source. The hub runs one clone; this one is how the app feeds it.
    internal: InternalSource,
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
        let internal = InternalSource::default();
        let hub = Hub::new(sources(&internal));
        TriggerService::with_hub(dir, hub, internal)
    }

    fn with_hub(dir: PathBuf, hub: Hub, internal: InternalSource) -> TriggerService {
        TriggerService {
            hub,
            internal,
            dir,
            names: Arc::new(Mutex::new(BTreeMap::new())),
            inner: Mutex::new(Inner::default()),
        }
    }

    pub fn catalogue(&self) -> Vec<SourceInfo> {
        self.hub.catalogue()
    }

    /// Hands a signal from inside the app to the `tg` source, which fires the
    /// armed rules it matches. Cheap when no rule is armed. Call it after the
    /// work it reports has succeeded.
    pub fn publish_internal(&self, signal: InternalSignal) {
        self.internal.publish(&signal);
    }

    /// True while a `tg.output-matched` rule is armed, so the terminal reader
    /// only assembles lines when someone is listening.
    pub fn wants_output(&self) -> bool {
        self.internal.wants_output()
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

    /// True when a definition file with this id exists.
    pub fn has_definition(&self, id: &str) -> bool {
        store::load(&self.dir).iter().any(|def| def.id == id)
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
    // `tg.app-started` goes out only after the first refresh has armed the
    // rules; published sooner, nothing would be listening.
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<crate::AppState>();
        refresh_now(&state);
        state.triggers.publish_internal(InternalSignal::AppStarted);
    });
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

// Commands ----------------------------------------------------------------

/// The answer of `triggers_check`: `{ ok: true }` or `{ ok: false, reason }`.
#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[tauri::command]
pub fn triggers_catalogue(state: State<'_, crate::AppState>) -> Vec<SourceInfo> {
    state.triggers.catalogue()
}

#[tauri::command]
pub fn triggers_check(state: State<'_, crate::AppState>, rule: Rule) -> CheckResult {
    match state.triggers.check(&rule) {
        Ok(()) => CheckResult {
            ok: true,
            reason: None,
        },
        Err(error) => CheckResult {
            ok: false,
            reason: Some(error.to_string()),
        },
    }
}

/// Recomputes first, so a definition file added since the last refresh shows up.
#[tauri::command]
pub async fn triggers_status(app: AppHandle) -> Result<TriggersStatus> {
    crate::blocking(move || {
        let state = app.state::<crate::AppState>();
        refresh_now(&state)
    })
    .await
}

#[tauri::command]
pub async fn triggers_arm(app: AppHandle, process_id: String, armed: bool) -> Result<()> {
    crate::blocking(move || {
        let state = app.state::<crate::AppState>();
        if armed && !state.triggers.has_definition(&process_id) {
            return Err(AppError::NotFound {
                message: "there is no process definition with that id".into(),
            });
        }
        state.state.update_config(|config| {
            config.triggers.armed.retain(|id| id != &process_id);
            if armed {
                config.triggers.armed.push(process_id.clone());
            }
        })?;
        refresh_now(&state);
        Ok(())
    })
    .await?
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
            TriggerService::with_hub(dir.clone(), Hub::new(Vec::new()), InternalSource::default()),
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

    #[test]
    fn a_published_signal_fires_an_armed_tg_rule() {
        let dir = std::env::temp_dir().join(format!("tg-triggers-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("ring.json"),
            r#"{"schemaVersion":1,"name":"Ring","triggers":[
                {"id":"bell","kind":"tg.bell","when":{}}]}"#,
        )
        .unwrap();
        let service = TriggerService::new(dir.clone());
        let status = service.refresh(&config(&["ring"]), &ctx());
        assert!(matches!(
            status.processes[0].rules[0].state,
            status::RuleStatusState::Active
        ));
        let (tx, rx) = channel();
        service.start_forwarding(move |payload| {
            let _ = tx.send(payload);
        });
        service.publish_internal(InternalSignal::Bell { pane: "p1".into() });
        let payload = rx.recv_timeout(Duration::from_secs(5)).expect("a payload");
        assert_eq!(payload.process_id, "ring");
        assert_eq!(payload.rule_id, "ring/bell");
        assert!(!service.wants_output());
        cleanup(&dir);
    }

    #[test]
    fn app_started_fires_only_once_the_rules_are_armed() {
        let dir = std::env::temp_dir().join(format!("tg-triggers-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("hello.json"),
            r#"{"schemaVersion":1,"name":"Hello","triggers":[
                {"id":"started","kind":"tg.app-started","when":{}}]}"#,
        )
        .unwrap();
        let service = TriggerService::new(dir.clone());
        let (tx, rx) = channel();
        service.start_forwarding(move |payload| {
            let _ = tx.send(payload);
        });

        // Before the first refresh nothing is armed, so the signal goes nowhere.
        service.publish_internal(InternalSignal::AppStarted);
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());

        service.refresh(&config(&["hello"]), &ctx());
        service.publish_internal(InternalSignal::AppStarted);
        let payload = rx.recv_timeout(Duration::from_secs(5)).expect("a payload");
        assert_eq!(payload.rule_id, "hello/started");
        assert_eq!(payload.kind, "tg.app-started");
        assert!(payload.payload.is_empty());
        cleanup(&dir);
    }

    #[test]
    fn has_definition_looks_for_the_file() {
        let (service, dir) = service();
        assert!(service.has_definition("tidy"));
        assert!(!service.has_definition("nope"));
        cleanup(&dir);
    }

    #[test]
    fn check_results_have_the_shape_the_frontend_reads() {
        let ok = CheckResult {
            ok: true,
            reason: None,
        };
        assert_eq!(
            serde_json::to_value(ok).unwrap(),
            serde_json::json!({ "ok": true })
        );
        let bad = CheckResult {
            ok: false,
            reason: Some("unknown kind x.y".into()),
        };
        assert_eq!(
            serde_json::to_value(bad).unwrap(),
            serde_json::json!({ "ok": false, "reason": "unknown kind x.y" })
        );
    }
}
