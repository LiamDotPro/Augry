//! The `tg` source: triggers the app raises itself (an agent finished, a
//! command failed, a worktree created). The app has the events, so there is no
//! crate and no OS layer: the feeds call `TriggerService::publish_internal`.
//! It has the same split as a trigger crate: `kinds.rs` (the catalogue),
//! `rule.rs` and `logic.rs` (pure, tested) and this glue. See README.md.

mod kinds;
pub mod logic;
pub mod rule;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::SystemTime;

use trigger_core::{
    Availability, Event, Handle, KindInfo, Rule, RuleError, Sink, Source, SourceError,
};

pub use kinds::KINDS;
use logic::Armed;

/// Something the app saw. The feeds build one and hand it to
/// `TriggerService::publish_internal`.
#[derive(Debug, Clone, PartialEq)]
pub enum InternalSignal {
    AgentFinished {
        pane: String,
        cwd: String,
        agent: String,
    },
    CommandEnded {
        pane: String,
        cwd: String,
        exit_code: i32,
        duration_ms: u64,
    },
    OutputLine {
        pane: String,
        cwd: String,
        line: String,
    },
    Bell {
        pane: String,
    },
    TaskReported {
        pane: String,
        cwd: String,
    },
    PaneOpened {
        pane: String,
        cwd: String,
        repo_root: Option<String>,
    },
    WorktreeCreated {
        repo: String,
        path: String,
        branch: Option<String>,
    },
    WorktreeRemoved {
        repo: String,
        path: String,
    },
    ReviewStaged {
        repo: String,
        files: u32,
    },
    NoteSaved {
        path: String,
        relative: String,
    },
    FocusEntered {
        pane: String,
        cwd: String,
    },
    AppStarted,
}

#[derive(Default)]
struct Shared {
    /// The rules of the current run; empty when no run is live.
    armed: Mutex<Vec<Armed>>,
    sink: Mutex<Option<Sink>>,
    /// True while a `tg.output-matched` rule is armed.
    output: AtomicBool,
}

impl Shared {
    fn clear(&self) {
        lock(&self.armed).clear();
        *lock(&self.sink) = None;
        self.output.store(false, Ordering::Relaxed);
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Cheap to clone: every clone shares the same state. `sources()` boxes one
/// for the hub and `TriggerService` keeps another to feed it.
#[derive(Clone, Default)]
pub struct InternalSource {
    shared: Arc<Shared>,
}

impl InternalSource {
    /// True while a `tg.output-matched` rule is armed, so the terminal reader
    /// can skip assembling lines the rest of the time.
    pub fn wants_output(&self) -> bool {
        self.shared.output.load(Ordering::Relaxed)
    }

    /// Fires every armed rule the signal matches. Returns how many events were
    /// sent. Does nothing while no run is live.
    pub fn publish(&self, signal: &InternalSignal) -> usize {
        let fires = logic::step(&lock(&self.shared.armed), signal);
        if fires.is_empty() {
            return 0;
        }
        let Some(sink) = lock(&self.shared.sink).clone() else {
            return 0;
        };
        let at = SystemTime::now();
        let mut sent = 0;
        for (rule, kind, payload) in fires {
            let event = Event {
                rule,
                kind,
                at,
                payload,
            };
            if sink.send(event).is_ok() {
                sent += 1;
            }
        }
        sent
    }
}

impl Source for InternalSource {
    fn id(&self) -> &'static str {
        "tg"
    }

    fn kinds(&self) -> &'static [KindInfo] {
        KINDS
    }

    fn availability(&self) -> Availability {
        Availability::Available
    }

    fn check(&self, rule: &Rule) -> Result<(), RuleError> {
        rule::parse(rule).map(|_| ())
    }

    /// Stores the rules and the sink; the hub stops the run by dropping the
    /// handle, which clears them. Patterns are compiled here, once.
    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError> {
        let mut armed = Vec::new();
        for r in &rules {
            let parsed = rule::parse(r).map_err(|e| SourceError::Os(format!("{}: {e}", r.id)))?;
            armed.push(Armed {
                id: r.id.clone(),
                kind: r.kind.clone(),
                parsed,
            });
        }
        let output = armed.iter().any(|a| a.kind == "tg.output-matched");
        *lock(&self.shared.armed) = armed;
        *lock(&self.shared.sink) = Some(sink);
        self.shared.output.store(output, Ordering::Relaxed);
        let shared = Arc::clone(&self.shared);
        Ok(Handle::new(move || shared.clear()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::mpsc::{channel, Receiver};
    use trigger_core::testkit::rule;

    fn started(source: &InternalSource, rules: Vec<Rule>) -> (Handle, Receiver<Event>) {
        let (tx, rx) = channel();
        (source.start(rules, tx).unwrap(), rx)
    }

    fn bell() -> InternalSignal {
        InternalSignal::Bell { pane: "p1".into() }
    }

    #[test]
    fn it_is_the_tg_source_with_thirteen_kinds() {
        let source = InternalSource::default();
        assert_eq!(source.id(), "tg");
        assert_eq!(source.kinds().len(), 13);
        assert_eq!(source.availability(), Availability::Available);
    }

    #[test]
    fn check_accepts_good_rules_and_rejects_bad_ones() {
        let source = InternalSource::default();
        assert!(source.check(&rule("a/b", "tg.bell", json!({}))).is_ok());
        assert!(source
            .check(&rule(
                "a/b",
                "tg.output-matched",
                json!({ "pattern": "error" })
            ))
            .is_ok());
        assert!(matches!(
            source.check(&rule("a/b", "tg.nope", json!({}))),
            Err(RuleError::UnknownKind(_))
        ));
        assert!(source
            .check(&rule("a/b", "tg.output-matched", json!({ "pattern": "(" })))
            .is_err());
    }

    #[test]
    fn a_published_signal_reaches_the_sink_as_an_event() {
        let source = InternalSource::default();
        let (_handle, rx) = started(&source, vec![rule("a/b", "tg.bell", json!({}))]);
        assert_eq!(source.publish(&bell()), 1);
        let event = rx.try_recv().unwrap();
        assert_eq!(event.rule, "a/b");
        assert_eq!(event.kind, "tg.bell");
        assert_eq!(event.payload["pane"], json!("p1"));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn a_clone_feeds_the_same_run() {
        let source = InternalSource::default();
        let feeder = source.clone();
        let (_handle, rx) = started(&source, vec![rule("a/b", "tg.bell", json!({}))]);
        assert_eq!(feeder.publish(&bell()), 1);
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn dropping_the_handle_stops_the_run() {
        let source = InternalSource::default();
        let (handle, rx) = started(
            &source,
            vec![rule("a/b", "tg.output-matched", json!({ "pattern": "x" }))],
        );
        assert!(source.wants_output());
        drop(handle);
        assert!(!source.wants_output());
        assert_eq!(source.publish(&bell()), 0);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn wants_output_only_while_an_output_rule_is_armed() {
        let source = InternalSource::default();
        assert!(!source.wants_output());
        let (handle, _rx) = started(&source, vec![rule("a/b", "tg.bell", json!({}))]);
        assert!(!source.wants_output());
        drop(handle);
        let (_handle, _rx) = started(
            &source,
            vec![rule("a/b", "tg.output-matched", json!({ "pattern": "x" }))],
        );
        assert!(source.wants_output());
    }

    #[test]
    fn start_fails_when_a_rule_does_not_parse() {
        let source = InternalSource::default();
        let (tx, _rx) = channel();
        let result = source.start(vec![rule("a/b", "tg.command-slow", json!({}))], tx);
        assert!(matches!(result, Err(SourceError::Os(_))));
        assert!(!source.wants_output());
    }

    #[test]
    fn publishing_with_no_run_sends_nothing() {
        assert_eq!(InternalSource::default().publish(&bell()), 0);
    }
}
