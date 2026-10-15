---
id: T27
epic: 001-trigger-core
title: Start the notify watcher and join it to the logic
status: done
depends_on: [T26]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-fs/src/lib.rs
  - src-tauri/crates/trigger-fs/src/os.rs
  - src-tauri/crates/trigger-fs/src/logic.rs
---

## Goal

`FsSource::start` really watches: it canonicalises each rule's folder, starts a `notify` watcher, and runs a named thread that feeds events and 100 ms ticks through `logic::step` into the sink. The crate now fires events. The temporary `allow(dead_code)` lines are gone.

## Read first

- `src-tauri/crates/trigger-fs/src/os.rs` the helpers from T26 and the stub `listen` you replace
- `src-tauri/crates/trigger-fs/src/lib.rs` the `start` stub from T21 you replace
- `src-tauri/crates/trigger-core/src/lib.rs` `Handle` (stops on drop), `Event`, `Sink`
- `plans/001-trigger-core/epic.md` Decisions 6 and 22

## Steps

1. In `src-tauri/crates/trigger-fs/src/os.rs`, delete the two lines `// Removed in T27, when lib.rs uses everything in this file.` and `#![allow(dead_code)]` and the blank line after them.

2. In the same file replace the block of `use` lines (from `use std::path::` down to and including `use crate::logic::{Armed, RawKind, Signal};`) with:

```rust
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use globset::GlobSet;
use notify::event::{EventKind, ModifyKind, RenameMode};
use notify::{RecursiveMode, Watcher};
use trigger_core::{Availability, SourceError};

use crate::logic::{Armed, RawKind, Signal};
```

3. In the same file replace the stub `listen` function, from its doc comment `/// Starts reporting signals for the armed rules. ...` through its closing `}`, with the text below. It creates one `notify` watcher for every distinct canonical root, recursive (Decision 6), and sends a `Signal::Raw` stamped with `now_ms()` for each event that can change a file. Watcher errors are dropped on purpose. The watcher is created and started here, so a failure to watch is returned from `start`; it is then moved to a named thread that waits for `stop` and drops it, which stops it.

Current text to replace:

```rust
/// Starts reporting signals for the armed rules. Returns at once; the work
/// happens on a named thread or an OS callback, and ends when `stop` is set.
pub fn listen(
    _armed: &[Armed],
    _stop: Arc<AtomicBool>,
    _tx: Sender<Signal>,
) -> Result<(), SourceError> {
    Err(SourceError::Unavailable("not implemented yet".into()))
}
```

New text:

```rust
/// Starts reporting signals for the armed rules: one `notify` watcher over
/// every distinct root, recursive. Returns at once. The watcher lives on a
/// named thread until `stop` is set, then it is dropped, which stops it.
pub fn listen(
    armed: &[Armed],
    stop: Arc<AtomicBool>,
    tx: Sender<Signal>,
) -> Result<(), SourceError> {
    let mut roots: Vec<PathBuf> = armed.iter().map(|a| a.root.clone()).collect();
    roots.sort();
    roots.dedup();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        // A watcher error carries no path to act on; the next event or the
        // next rescan puts things right.
        let Ok(event) = res else { return };
        let kind = raw_kind(&event);
        if kind == RawKind::Ignore {
            return;
        }
        let _ = tx.send(Signal::Raw {
            at: now_ms(),
            kind,
            paths: event.paths,
        });
    })
    .map_err(|e| SourceError::Os(e.to_string()))?;
    for root in &roots {
        watcher
            .watch(root, RecursiveMode::Recursive)
            .map_err(|e| SourceError::Os(format!("cannot watch {}: {e}", root.display())))?;
    }

    std::thread::Builder::new()
        .name("trigger-fs-watch".into())
        .spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(50));
            }
            drop(watcher);
        })
        .map_err(|e| SourceError::Os(e.to_string()))?;
    Ok(())
}
```

4. In the same file, inside `mod tests`, directly above its final closing `}`, add this test. It is `#[ignore]` because it needs a real watcher.

```rust
    /// Needs a real file system watcher. Run with:
    /// `cargo test -p trigger-fs -- --ignored --test-threads=1`
    #[test]
    #[ignore]
    fn listen_reports_a_created_file_and_stops() {
        use std::sync::mpsc::channel;

        let dir = TempDir::new();
        let parsed = crate::rule::parse(&trigger_core::testkit::rule(
            "p/t",
            "fs.file-created",
            serde_json::json!({ "path": dir.0 }),
        ))
        .unwrap();
        let armed = Armed {
            id: "p/t".into(),
            kind: "fs.file-created".into(),
            root: dir.0.clone(),
            when: parsed.when,
            glob: parsed.glob,
            ignore: parsed.ignore,
        };
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        listen(&[armed], stop.clone(), tx).unwrap();
        std::thread::sleep(Duration::from_millis(400));
        let file = dir.0.join("hello.txt");
        std::fs::write(&file, "hi").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut seen = false;
        while std::time::Instant::now() < deadline && !seen {
            if let Ok(Signal::Raw { paths, .. }) = rx.recv_timeout(Duration::from_millis(100)) {
                seen = paths.contains(&file);
            }
        }
        stop.store(true, Ordering::Relaxed);
        assert!(seen, "no raw event for the new file within 10 s");
    }
```

5. In `src-tauri/crates/trigger-fs/src/logic.rs`, delete the two lines `// Removed in T27, when lib.rs reads everything in this file.` and `#![allow(dead_code)]` and the blank line after them.

6. In `src-tauri/crates/trigger-fs/src/lib.rs`, replace the block `use trigger_core::{ ... };` (the one import block below the three `mod` lines) with:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use trigger_core::{
    Availability, Event, FieldInfo, Handle, KindInfo, Platforms, Rule, RuleError, Sink, Source,
    SourceError,
};
```

7. In the same file replace everything from the line `    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError> {` to the end of the file with the text below. `start` parses every rule, canonicalises its folder (a folder that does not exist is a `SourceError::Os`), starts the watcher, and starts the thread `trigger-fs`. `run` keeps one state per rule, scans the folder once to seed the known set (after the watcher is up, so `start` returns at once), and then loops: it waits up to 100 ms for a signal, adds a `Tick` after every signal and after every quiet 100 ms, feeds both through `logic::step`, and sends the events. A `Rescan` makes it scan again. Dropping the `Handle` sets `stop`, which ends both threads.

```rust
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
```

8. Run `cd src-tauri && cargo fmt -p trigger-fs`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-fs --lib` passes: 47 passed, 1 ignored
- [ ] `cd src-tauri && cargo test -p trigger-fs --lib -- --ignored --test-threads=1` passes: 1 test, `listen_reports_a_created_file_and_stops`
- [ ] `cd src-tauri && cargo test -p trigger-fs` passes (the template's live test stays ignored)
- [ ] `grep -rn "allow(dead_code)\|not implemented yet" src-tauri/crates/trigger-fs/src` prints nothing
- [ ] `cd src-tauri && cargo clippy -p trigger-fs --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-fs --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Running `cargo test -p trigger-fs -- --ignored` without `--lib`: the template `tests/live.rs` still starts the example kind and fails until T28 replaces it.
- The five live tests (T28) and the README sections (T29).
- A debouncer crate, more threads, or a channel other than `std::sync::mpsc`.
- Changing any value in `logic.rs` beyond deleting the two attribute lines.
- Registering the source in the app. That is slice D.

## If stuck

- If `cargo clippy` reports an unused import in `os.rs` or `lib.rs`, the import block of step 2 or 6 was not replaced as a whole. Re-copy it.
- If `cargo test -p trigger-fs --lib -- --ignored` fails with "no raw event for the new file within 10 s" on macOS, the folder may need a permission: run it again once, then set status blocked and say what you saw.
- If `watcher.watch` is reported as not found, the line `use notify::{RecursiveMode, Watcher};` is missing (the trait must be in scope).
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
