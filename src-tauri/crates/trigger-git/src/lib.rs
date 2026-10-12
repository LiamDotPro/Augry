//! Fires when something happens in a git repository. See README.md.
//!
//! Glue only: the kinds this crate fires, the `Source` implementation, and the
//! thread that feeds OS signals through the pure logic into the sink. Rules
//! live in `rule.rs`, decisions in `logic.rs`, the OS in `os.rs`.

mod logic;
mod os;
mod rule;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use trigger_core::{
    Availability, Event, FieldInfo, Handle, KindInfo, Platforms, Rule, RuleError, Sink, Source,
    SourceError,
};

/// Every kind this crate fires. The ids are a contract with saved process
/// definitions: add kinds, never rename one.
pub static KINDS: &[KindInfo] = &[KindInfo {
    id: "git.example",
    title: "Example",
    summary: "Replace with what makes this kind fire.",
    platforms: Platforms::ALL,
    permission: None,
    when: &[FieldInfo {
        name: "example",
        doc: "Replace with the real fields",
        required: true,
    }],
    payload: &[FieldInfo {
        name: "example",
        doc: "Replace with the real fields",
        required: true,
    }],
}];

pub struct GitSource;

impl Source for GitSource {
    fn id(&self) -> &'static str {
        "git"
    }

    fn kinds(&self) -> &'static [KindInfo] {
        KINDS
    }

    fn availability(&self) -> Availability {
        os::availability()
    }

    fn check(&self, rule: &Rule) -> Result<(), RuleError> {
        rule::parse(rule).map(|_| ())
    }

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

        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<logic::Signal>();
        os::listen(&armed, stop.clone(), tx)?;

        let thread_stop = stop.clone();
        std::thread::Builder::new()
            .name("trigger-git".into())
            .spawn(move || {
                let mut state = logic::State::default();
                while !thread_stop.load(Ordering::Relaxed) {
                    let Ok(signal) = rx.recv_timeout(Duration::from_millis(250)) else {
                        continue;
                    };
                    for (rule, kind, payload) in logic::step(&mut state, &armed, signal) {
                        let event = Event {
                            rule,
                            kind,
                            at: SystemTime::now(),
                            payload,
                        };
                        if sink.send(event).is_err() {
                            return;
                        }
                    }
                }
            })
            .map_err(|e| SourceError::Os(e.to_string()))?;

        Ok(Handle::new(move || stop.store(true, Ordering::Relaxed)))
    }
}
