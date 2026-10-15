---
id: T22
epic: 001-trigger-core
title: Add the settle state machine for created, saved, removed
status: done
depends_on: [T21]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-fs/src/logic.rs
  - src-tauri/crates/trigger-fs/src/logic/tests.rs
---

## Goal

`logic.rs` turns raw file events into created, saved and removed decisions the way Decision 22 says: events only mark a path pending, and a stat after `settle_ms` of quiet decides. It does not match kinds, rename or download yet.

## Read first

- `plans/001-trigger-core/epic.md` Decision 22 the design you are implementing
- `plans/001-trigger-core/research/R03-notify-events.md` the event sequences the tests replay
- `src-tauri/crates/trigger-fs/src/logic.rs` the skeleton from T21 that you replace

## Steps

1. Replace the whole of `src-tauri/crates/trigger-fs/src/logic.rs` with this. It is Decision 22 minus rename pairing, download finished, the glob and ignore filter and the payload (T23 to T25 add those). How it works: `ingest` only marks a path pending with the time of its last event; `resolve`, called from `settle` on every tick, decides each path that has been quiet for `settle_ms` by asking `is_file` and the known set; the result is created, saved or removed, and a path that is not known and not there is dropped. Time is always an argument; nothing here reads a clock. Keep the `#![allow(dead_code)]` line, T27 removes it.

```rust
//! Pure decisions: given what the OS reported, which rules fire and with what
//! payload. No OS calls, no threads, no clocks: time and "is this a file"
//! come in as arguments, so every case is a unit test.
//!
//! The design is Decision 22 of plans/001-trigger-core/epic.md. Event kinds
//! are not trusted (on macOS a plain save arrives as `Create`). A raw event
//! only marks a path pending; once the path has been quiet for `settle_ms`
//! the answer comes from a stat, compared with the set of files known to
//! exist.

// Removed in T27, when lib.rs reads everything in this file.
#![allow(dead_code)]

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use globset::GlobSet;
use trigger_core::{RuleId, Value};

use crate::rule::When;

/// A rule this crate is watching for. `root` is `when.path` made canonical.
#[derive(Debug, Clone)]
pub struct Armed {
    pub id: RuleId,
    pub kind: String,
    pub when: When,
    pub root: PathBuf,
    pub glob: GlobSet,
    pub ignore: GlobSet,
}

/// What `os.rs` makes of a `notify` event kind, so this file does not depend
/// on `notify`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawKind {
    Create,
    Remove,
    /// Data, metadata or unspecified change.
    Modify,
    /// One half of a rename (`Name(From)`, `Name(To)`, `Name(Any)`).
    Name,
    /// Both halves of a rename in one event, `paths` is `[from, to]` (Linux).
    Both,
    /// The OS lost events: forget what is pending and scan again.
    Rescan,
    /// Access events and anything else that never changes a file.
    Ignore,
}

/// Time is a number of milliseconds on any clock that only goes forward. The
/// caller reads the clock; nothing in here does.
#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Raw {
        at: u64,
        kind: RawKind,
        paths: Vec<PathBuf>,
    },
    Tick {
        now: u64,
    },
}

/// What a rule can fire for, before the kind of the rule picks its own.
#[derive(Debug, Clone, PartialEq)]
pub enum Obs {
    Created(PathBuf),
    Saved(PathBuf),
    Removed(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
    DownloadFinished(PathBuf),
}

/// A path that has had events and is waiting to go quiet.
#[derive(Debug, Clone, Copy)]
struct Pending {
    last: u64,
    /// A rename event touched it.
    name: bool,
}

/// A pending path that has settled and been decided.
#[derive(Debug, Clone)]
struct Res {
    path: PathBuf,
    name: bool,
}

#[derive(Debug, Default)]
struct Resolved {
    created: Vec<Res>,
    saved: Vec<Res>,
    removed: Vec<Res>,
}

/// Everything remembered between signals for one rule.
#[derive(Debug, Default)]
pub struct State {
    /// Files known to exist. Seeded by a scan, updated after every decision.
    known: HashSet<PathBuf>,
    pending: BTreeMap<PathBuf, Pending>,
    /// `[from, to]` pairs from `RawKind::Both`.
    pairs: Vec<(PathBuf, PathBuf)>,
    /// Partial download files seen lately: (path, last event time).
    partials: Vec<(PathBuf, u64)>,
    reseed: bool,
}

impl State {
    /// Replaces the known set with the files found by a scan.
    pub fn seed(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        self.known = paths.into_iter().collect();
    }

    /// True once after a `Rescan`: the caller scans again and calls `seed`.
    pub fn take_reseed(&mut self) -> bool {
        std::mem::take(&mut self.reseed)
    }
}

pub type Payload = BTreeMap<String, Value>;

const PARTIAL_SUFFIXES: [&str; 3] = [".crdownload", ".part", ".download"];

fn is_partial(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    PARTIAL_SUFFIXES.iter().any(|s| name.ends_with(s))
}

/// The events a signal fires for this rule: (kind, payload). `is_file` says
/// whether a path is an existing regular file.
pub fn step(
    _state: &mut State,
    _armed: &Armed,
    _signal: Signal,
    _is_file: &dyn Fn(&Path) -> bool,
) -> Vec<(String, Payload)> {
    Vec::new()
}

/// Every observation a signal produces, whatever the rule's kind.
fn observe(
    state: &mut State,
    armed: &Armed,
    signal: Signal,
    is_file: &dyn Fn(&Path) -> bool,
) -> Vec<Obs> {
    match signal {
        Signal::Raw { at, kind, paths } => {
            ingest(state, armed, at, kind, paths);
            Vec::new()
        }
        Signal::Tick { now } => settle(state, armed, now, is_file),
    }
}

/// Marks paths pending. Ignore and glob are not applied here, so partial
/// download names still reach the state machine.
fn ingest(state: &mut State, armed: &Armed, at: u64, kind: RawKind, paths: Vec<PathBuf>) {
    match kind {
        RawKind::Ignore => return,
        RawKind::Rescan => {
            state.pending.clear();
            state.pairs.clear();
            state.partials.clear();
            state.reseed = true;
            return;
        }
        _ => {}
    }
    let inside: Vec<PathBuf> = paths
        .into_iter()
        .filter(|p| p.starts_with(&armed.root) && *p != armed.root)
        .collect();
    let name = matches!(kind, RawKind::Name | RawKind::Both);
    for path in &inside {
        let entry = state.pending.entry(path.clone()).or_insert(Pending {
            last: at,
            name: false,
        });
        entry.last = at;
        entry.name |= name;
        if is_partial(path) {
            state.partials.retain(|(p, _)| p != path);
            state.partials.push((path.clone(), at));
        }
    }
    if kind == RawKind::Both && inside.len() == 2 {
        state.pairs.push((inside[0].clone(), inside[1].clone()));
    }
}

/// Decides every pending path that has been quiet for `settle_ms`, using a
/// stat and the known set, and keeps the known set up to date.
fn resolve(
    state: &mut State,
    armed: &Armed,
    now: u64,
    is_file: &dyn Fn(&Path) -> bool,
) -> Resolved {
    let due: Vec<(PathBuf, Pending)> = state
        .pending
        .iter()
        .filter(|(_, p)| now.saturating_sub(p.last) >= armed.when.settle_ms)
        .map(|(path, p)| (path.clone(), *p))
        .collect();
    let mut out = Resolved::default();
    for (path, pending) in due {
        state.pending.remove(&path);
        let was_known = state.known.contains(&path);
        let res = Res {
            path: path.clone(),
            name: pending.name,
        };
        match (was_known, is_file(&path)) {
            (false, true) => {
                state.known.insert(path.clone());
                out.created.push(res);
            }
            (true, true) => out.saved.push(res),
            (true, false) => {
                state.known.remove(&path);
                out.removed.push(res);
            }
            // A temp file that came and went.
            (false, false) => {}
        }
    }
    out
}

fn settle(state: &mut State, armed: &Armed, now: u64, is_file: &dyn Fn(&Path) -> bool) -> Vec<Obs> {
    let resolved = resolve(state, armed, now, is_file);
    let mut obs = Vec::new();
    obs.extend(resolved.removed.into_iter().map(|r| Obs::Removed(r.path)));
    obs.extend(resolved.created.into_iter().map(|r| Obs::Created(r.path)));
    obs.extend(resolved.saved.into_iter().map(|r| Obs::Saved(r.path)));
    obs
}

#[cfg(test)]
mod tests;
```

2. Replace the whole of `src-tauri/crates/trigger-fs/src/logic/tests.rs` with this. `Fx` is a rule, its state and a fake disk (the set of files that exist). The tests replay the event sequences measured in `plans/001-trigger-core/research/R03-notify-events.md` with times as plain numbers, so none of them sleeps.

```rust
use super::*;
use crate::rule::parse;
use serde_json::json;
use std::cell::RefCell;
use std::rc::Rc;
use trigger_core::testkit::rule;

/// A rule, its state and a fake disk (the set of files that exist).
struct Fx {
    armed: Armed,
    state: State,
    disk: Rc<RefCell<HashSet<PathBuf>>>,
}

impl Fx {
    /// `existing` are paths relative to the root that exist and are known
    /// before the test starts, as after the start scan.
    fn new(kind: &str, extra_when: serde_json::Value, existing: &[&str]) -> Fx {
        let root = std::env::temp_dir().join("tg-fs-logic-root");
        let mut when = json!({ "path": root });
        when.as_object_mut()
            .unwrap()
            .extend(extra_when.as_object().unwrap().clone());
        let parsed = parse(&rule("p/t", kind, when)).unwrap();
        let armed = Armed {
            id: "p/t".into(),
            kind: kind.into(),
            root: root.clone(),
            when: parsed.when,
            glob: parsed.glob,
            ignore: parsed.ignore,
        };
        let files: HashSet<PathBuf> = existing.iter().map(|r| root.join(r)).collect();
        let mut state = State::default();
        state.seed(files.iter().cloned());
        Fx {
            armed,
            state,
            disk: Rc::new(RefCell::new(files)),
        }
    }

    fn saved() -> Fx {
        Fx::new("fs.file-saved", json!({}), &[])
    }

    fn p(&self, rel: &str) -> PathBuf {
        self.armed.root.join(rel)
    }

    fn put(&self, rel: &str) {
        self.disk.borrow_mut().insert(self.p(rel));
    }

    fn delete(&self, rel: &str) {
        self.disk.borrow_mut().remove(&self.p(rel));
    }

    fn raw(&mut self, at: u64, kind: RawKind, rels: &[&str]) {
        let paths = rels.iter().map(|r| self.p(r)).collect();
        let signal = Signal::Raw { at, kind, paths };
        assert!(self.observe(signal).is_empty());
    }

    fn tick(&mut self, now: u64) -> Vec<Obs> {
        self.observe(Signal::Tick { now })
    }

    fn observe(&mut self, signal: Signal) -> Vec<Obs> {
        let disk = self.disk.clone();
        let is_file = move |p: &Path| disk.borrow().contains(p);
        observe(&mut self.state, &self.armed, signal, &is_file)
    }
}

#[test]
fn macos_plain_save_of_a_known_file_is_one_saved() {
    // R03: Create, Metadata, Metadata, Data on a path that existed.
    let mut fx = Fx::new("fs.file-saved", json!({}), &["a.txt"]);
    fx.raw(1000, RawKind::Create, &["a.txt"]);
    fx.raw(1000, RawKind::Modify, &["a.txt"]);
    fx.raw(1001, RawKind::Modify, &["a.txt"]);
    fx.raw(1001, RawKind::Modify, &["a.txt"]);
    assert!(fx.tick(1100).is_empty(), "still inside the settle time");
    assert_eq!(fx.tick(1501), vec![Obs::Saved(fx.p("a.txt"))]);
    assert!(fx.tick(2000).is_empty(), "decided once");
}

#[test]
fn macos_atomic_save_is_one_saved_and_nothing_for_the_temp_file() {
    // R03: a.txt.tmp gets Create, Name, Metadata, Data; a.txt gets Name twice.
    let mut fx = Fx::new("fs.file-saved", json!({}), &["a.txt"]);
    fx.put("a.txt.tmp");
    fx.raw(1000, RawKind::Create, &["a.txt.tmp"]);
    fx.raw(1000, RawKind::Name, &["a.txt.tmp"]);
    fx.raw(1000, RawKind::Modify, &["a.txt.tmp"]);
    fx.raw(1001, RawKind::Name, &["a.txt"]);
    fx.raw(1001, RawKind::Name, &["a.txt"]);
    fx.delete("a.txt.tmp"); // renamed over the target
    assert_eq!(fx.tick(1600), vec![Obs::Saved(fx.p("a.txt"))]);
}

#[test]
fn a_new_file_is_created_even_after_several_quick_writes() {
    let mut fx = Fx::new("fs.file-created", json!({}), &[]);
    fx.put("new.txt");
    fx.raw(1000, RawKind::Create, &["new.txt"]);
    fx.raw(1100, RawKind::Modify, &["new.txt"]);
    fx.raw(1200, RawKind::Modify, &["new.txt"]);
    assert!(fx.tick(1600).is_empty(), "the last write was 400 ms ago");
    assert_eq!(fx.tick(1700), vec![Obs::Created(fx.p("new.txt"))]);
}

#[test]
fn a_file_that_existed_before_start_is_not_created() {
    let mut fx = Fx::new("fs.file-created", json!({}), &["old.txt"]);
    fx.raw(1000, RawKind::Create, &["old.txt"]);
    assert_eq!(fx.tick(2000), vec![Obs::Saved(fx.p("old.txt"))]);
}

#[test]
fn delete_then_recreate_is_one_saved() {
    let mut fx = Fx::new("fs.file-saved", json!({}), &["a.txt"]);
    fx.raw(1000, RawKind::Create, &["a.txt"]);
    fx.raw(1000, RawKind::Remove, &["a.txt"]);
    fx.raw(1001, RawKind::Create, &["a.txt"]);
    fx.raw(1001, RawKind::Modify, &["a.txt"]);
    assert_eq!(fx.tick(1600), vec![Obs::Saved(fx.p("a.txt"))]);
}

#[test]
fn a_deleted_file_is_removed_and_then_created_again_later() {
    let mut fx = Fx::new("fs.file-removed", json!({}), &["a.txt"]);
    fx.delete("a.txt");
    fx.raw(1000, RawKind::Remove, &["a.txt"]);
    fx.raw(1000, RawKind::Name, &["a.txt"]);
    assert_eq!(fx.tick(1600), vec![Obs::Removed(fx.p("a.txt"))]);
    fx.put("a.txt");
    fx.raw(3000, RawKind::Create, &["a.txt"]);
    assert_eq!(fx.tick(3600), vec![Obs::Created(fx.p("a.txt"))]);
}

#[test]
fn a_file_that_came_and_went_is_dropped() {
    let mut fx = Fx::saved();
    fx.raw(1000, RawKind::Create, &["swap.txt"]);
    assert!(fx.tick(2000).is_empty());
}

#[test]
fn directories_are_never_reported() {
    // The fake disk only holds files, so a directory is "not a file".
    let mut fx = Fx::saved();
    fx.raw(1000, RawKind::Create, &["subdir"]);
    assert!(fx.tick(2000).is_empty());
}

#[test]
fn access_events_and_paths_outside_the_root_are_ignored() {
    let mut fx = Fx::new("fs.file-saved", json!({}), &["a.txt"]);
    fx.raw(1000, RawKind::Ignore, &["a.txt"]);
    let outside = std::env::temp_dir().join("tg-fs-elsewhere").join("a.txt");
    let signal = Signal::Raw {
        at: 1000,
        kind: RawKind::Modify,
        paths: vec![outside, fx.armed.root.clone()],
    };
    assert!(fx.observe(signal).is_empty());
    assert!(fx.tick(2000).is_empty());
}

#[test]
fn settle_ms_is_per_rule() {
    let mut fx = Fx::new("fs.file-created", json!({ "settle_ms": 50 }), &[]);
    fx.put("a.txt");
    fx.raw(1000, RawKind::Create, &["a.txt"]);
    assert!(fx.tick(1049).is_empty());
    assert_eq!(fx.tick(1050), vec![Obs::Created(fx.p("a.txt"))]);
}

#[test]
fn a_rescan_forgets_pending_paths_and_asks_for_a_new_scan() {
    let mut fx = Fx::new("fs.file-saved", json!({}), &["a.txt"]);
    fx.raw(1000, RawKind::Modify, &["a.txt"]);
    fx.raw(1001, RawKind::Rescan, &[]);
    assert!(fx.tick(2000).is_empty());
    assert!(fx.state.take_reseed());
    assert!(!fx.state.take_reseed(), "only once");
    fx.state.seed([fx.p("b.txt")]);
    fx.put("b.txt");
    fx.raw(3000, RawKind::Modify, &["b.txt"]);
    assert_eq!(fx.tick(4000), vec![Obs::Saved(fx.p("b.txt"))]);
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-fs`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-fs --lib logic` passes: 11 tests
- [ ] `cd src-tauri && cargo test -p trigger-fs` passes (19 in the library)
- [ ] `cd src-tauri && cargo clippy -p trigger-fs --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-fs --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Rename pairing, download finished, the glob and ignore filter, the payload and the real `step` (T23 to T25). The `State` fields `pairs` and `partials` are filled in by `ingest` already and are read later.
- Reading a clock, sleeping, or touching the file system in `logic.rs`.
- Removing the `#![allow(dead_code)]` line.
- Changing the stub `step`, `rule.rs`, `lib.rs` or `os.rs`.
- Adding tests beyond the eleven given.
- Editing the README (T29 writes it; the crate fires nothing until T27).

## If stuck

- If a test fails, the code differs from step 1 or 2. Re-copy; never change an expected value in a test.
- If `cargo clippy` reports an unused import or a dead code warning, the `#![allow(dead_code)]` line is missing; restore it from step 1.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (logic 11/11, trigger-fs 19 lib, clippy, fmt, check-crates exit 0 with accepted warnings)
- 2026-10-12 checker: pass
