//! Fires when files change under a watched folder. See README.md.
//!
//! Glue only: the kinds this crate fires, the `Source` implementation, and the
//! thread that feeds OS signals through the pure logic into the sink. Rules
//! live in `rule.rs`, decisions in `logic.rs`, the OS in `os.rs`.

mod logic;
mod os;
mod rule;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use trigger_core::{
    Availability, Event, FieldInfo, Handle, KindInfo, Platforms, Rule, RuleError, Sink, Source,
    SourceError,
};

const WHEN: &[FieldInfo] = &[
    FieldInfo {
        name: "path",
        doc: "Folder to watch, absolute. Everything below it is watched.",
        required: true,
    },
    FieldInfo {
        name: "glob",
        doc: "Which files count, matched against the path below the folder. Default **/*",
        required: false,
    },
    FieldInfo {
        name: "ignore",
        doc: "Globs to skip, added to the built in ones (.git, node_modules, target, temp and partial download files)",
        required: false,
    },
    FieldInfo {
        name: "settle_ms",
        doc: "Milliseconds a file must be quiet before it counts as changed. Default 500",
        required: false,
    },
];

const PAYLOAD: &[FieldInfo] = &[
    FieldInfo {
        name: "path",
        doc: "Absolute path of the file",
        required: true,
    },
    FieldInfo {
        name: "name",
        doc: "File name",
        required: true,
    },
    FieldInfo {
        name: "relative",
        doc: "Path below the watched folder, with / separators",
        required: true,
    },
    FieldInfo {
        name: "ext",
        doc: "Extension without the dot, empty when there is none",
        required: true,
    },
];

const PAYLOAD_RENAMED: &[FieldInfo] = &[
    FieldInfo {
        name: "path",
        doc: "Absolute path of the file after the rename",
        required: true,
    },
    FieldInfo {
        name: "name",
        doc: "File name after the rename",
        required: true,
    },
    FieldInfo {
        name: "relative",
        doc: "Path below the watched folder, with / separators",
        required: true,
    },
    FieldInfo {
        name: "ext",
        doc: "Extension without the dot, empty when there is none",
        required: true,
    },
    FieldInfo {
        name: "from",
        doc: "Absolute path before the rename",
        required: true,
    },
];

/// Every kind this crate fires. The ids are a contract with saved process
/// definitions: add kinds, never rename one.
pub static KINDS: &[KindInfo] = &[
    KindInfo {
        id: "fs.file-saved",
        title: "File saved",
        summary: "A file that already existed was written to.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD,
    },
    KindInfo {
        id: "fs.file-created",
        title: "File created",
        summary: "A new file appeared.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD,
    },
    KindInfo {
        id: "fs.file-removed",
        title: "File removed",
        summary: "A file was deleted.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD,
    },
    KindInfo {
        id: "fs.file-renamed",
        title: "File renamed",
        summary: "A file was renamed or moved inside the watched folder.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD_RENAMED,
    },
    KindInfo {
        id: "fs.download-finished",
        title: "Download finished",
        summary: "A browser finished a download and the file got its final name.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD,
    },
];

pub struct FsSource;

impl Source for FsSource {
    fn id(&self) -> &'static str {
        "fs"
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
        let mut armed = Vec::new();
        for r in &rules {
            let parsed = rule::parse(r).map_err(|e| SourceError::Os(format!("{}: {e}", r.id)))?;
            armed.push(logic::Armed {
                id: r.id.clone(),
                kind: r.kind.clone(),
                root: os::canonical(&parsed.when.path)?,
                when: parsed.when,
                glob: parsed.glob,
                ignore: parsed.ignore,
            });
        }

        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<logic::Signal>();
        os::listen(&armed, stop.clone(), tx)?;

        let thread_stop = stop.clone();
        std::thread::Builder::new()
            .name("trigger-fs".into())
            .spawn(move || run(armed, rx, sink, &thread_stop))
            .map_err(|e| SourceError::Os(e.to_string()))?;

        Ok(Handle::new(move || stop.store(true, Ordering::Relaxed)))
    }
}

/// Joins the watcher to the logic. Each rule has its own state. The first
/// scan runs here, after the watcher is up, so `start` returns at once even
/// for a big tree. A tick follows every signal and every 100 ms of quiet, so
/// a path settles within 100 ms of its time.
fn run(armed: Vec<logic::Armed>, rx: Receiver<logic::Signal>, sink: Sink, stop: &AtomicBool) {
    let mut slots: Vec<(logic::Armed, logic::State)> = armed
        .into_iter()
        .map(|a| {
            let mut state = logic::State::default();
            state.seed(os::scan(&a.root, &a.ignore));
            (a, state)
        })
        .collect();

    while !stop.load(Ordering::Relaxed) {
        let mut signals = Vec::new();
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(signal) => signals.push(signal),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        signals.push(logic::Signal::Tick { now: os::now_ms() });

        for signal in signals {
            for (armed, state) in &mut slots {
                for (kind, payload) in logic::step(state, armed, signal.clone(), &os::is_file) {
                    let event = Event {
                        rule: armed.id.clone(),
                        kind,
                        at: SystemTime::now(),
                        payload,
                    };
                    if sink.send(event).is_err() {
                        return;
                    }
                }
                if state.take_reseed() {
                    state.seed(os::scan(&armed.root, &armed.ignore));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::mpsc::channel;
    use trigger_core::testkit::rule;

    fn saved(when: serde_json::Value) -> Rule {
        rule("p/t", "fs.file-saved", when)
    }

    #[test]
    fn the_source_describes_itself() {
        assert_eq!(FsSource.id(), "fs");
        let ids: Vec<&str> = FsSource.kinds().iter().map(|k| k.id).collect();
        assert_eq!(
            ids,
            [
                "fs.file-saved",
                "fs.file-created",
                "fs.file-removed",
                "fs.file-renamed",
                "fs.download-finished"
            ]
        );
        assert_eq!(FsSource.availability(), Availability::Available);
    }

    #[test]
    fn check_accepts_a_good_rule_and_names_the_bad_field() {
        let ok = saved(json!({ "path": std::env::temp_dir() }));
        assert!(FsSource.check(&ok).is_ok());
        let bad = saved(json!({ "path": std::env::temp_dir(), "glob": "[" }));
        match FsSource.check(&bad) {
            Err(RuleError::Field { field, .. }) => assert_eq!(field, "glob"),
            other => panic!("expected a glob error, got {other:?}"),
        }
    }

    #[test]
    fn start_fails_for_a_folder_that_does_not_exist() {
        let missing = std::env::temp_dir().join(format!("tg-fs-missing-{}", uuid::Uuid::new_v4()));
        let (tx, _rx) = channel();
        let result = FsSource.start(vec![saved(json!({ "path": missing }))], tx);
        assert!(matches!(result, Err(SourceError::Os(_))));
    }

    #[test]
    fn start_fails_for_a_bad_rule() {
        let (tx, _rx) = channel();
        let result = FsSource.start(vec![saved(json!({ "path": "relative/dir" }))], tx);
        assert!(matches!(result, Err(SourceError::Os(_))));
    }
}
