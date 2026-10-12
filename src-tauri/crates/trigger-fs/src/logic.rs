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
