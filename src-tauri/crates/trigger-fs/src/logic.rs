//! Pure decisions: given what the OS reported, which rules fire and with what
//! payload. No OS calls, no threads, no clocks: time and "is this a file"
//! come in as arguments, so every case is a unit test.
//!
//! The design is Decision 22 of plans/001-trigger-core/epic.md. Event kinds
//! are not trusted (on macOS a plain save arrives as `Create`). A raw event
//! only marks a path pending; once the path has been quiet for `settle_ms`
//! the answer comes from a stat, compared with the set of files known to
//! exist.

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

/// How long after its last event a partial download file still counts as the
/// start of a download.
const DOWNLOAD_WINDOW_MS: u64 = 10_000;

fn is_partial(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    PARTIAL_SUFFIXES.iter().any(|s| name.ends_with(s))
}

/// True when the path is below the root and passes the rule's `glob` and
/// `ignore`. Both are matched against the path relative to the root.
fn accepts(armed: &Armed, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(&armed.root) else {
        return false;
    };
    !rel.as_os_str().is_empty() && armed.glob.is_match(rel) && !armed.ignore.is_match(rel)
}

/// The events a signal fires for this rule: (kind, payload). `is_file` says
/// whether a path is an existing regular file; directories are never
/// reported.
pub fn step(
    state: &mut State,
    armed: &Armed,
    signal: Signal,
    is_file: &dyn Fn(&Path) -> bool,
) -> Vec<(String, Payload)> {
    observe(state, armed, signal, is_file)
        .iter()
        .filter(|o| o.kind_id() == armed.kind)
        .map(|o| (o.kind_id().to_string(), o.payload(armed)))
        .collect()
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
/// stat and the known set, and keeps the known set up to date. Paths that
/// the rule's glob and ignore reject are decided too (so the known set stays
/// right) but not returned.
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
    out.created.retain(|r| accepts(armed, &r.path));
    out.saved.retain(|r| accepts(armed, &r.path));
    out.removed.retain(|r| accepts(armed, &r.path));
    out
}

fn settle(state: &mut State, armed: &Armed, now: u64, is_file: &dyn Fn(&Path) -> bool) -> Vec<Obs> {
    state
        .partials
        .retain(|(_, at)| now.saturating_sub(*at) <= DOWNLOAD_WINDOW_MS);
    let mut resolved = resolve(state, armed, now, is_file);
    let mut obs = pair_renames(state, &mut resolved);
    let finished = finished_downloads(state, &resolved, is_file);
    obs.extend(resolved.removed.into_iter().map(|r| Obs::Removed(r.path)));
    obs.extend(resolved.created.into_iter().map(|r| Obs::Created(r.path)));
    obs.extend(resolved.saved.into_iter().map(|r| Obs::Saved(r.path)));
    obs.extend(finished);
    obs
}

/// A file that was just created or saved is a finished download when a
/// partial file with its name plus `.crdownload`, `.part` or `.download` had
/// events lately and is gone now. Each partial is used once.
fn finished_downloads(
    state: &mut State,
    resolved: &Resolved,
    is_file: &dyn Fn(&Path) -> bool,
) -> Vec<Obs> {
    let mut finished = Vec::new();
    let mut used: Vec<PathBuf> = Vec::new();
    for r in resolved.created.iter().chain(resolved.saved.iter()) {
        let name = r.path.to_string_lossy().to_lowercase();
        let partial = state.partials.iter().map(|(p, _)| p).find(|p| {
            let p = p.to_string_lossy().to_lowercase();
            PARTIAL_SUFFIXES.iter().any(|s| p == format!("{name}{s}"))
        });
        if let Some(partial) = partial {
            if !is_file(partial) {
                used.push(partial.clone());
                finished.push(Obs::DownloadFinished(r.path.clone()));
            }
        }
    }
    state.partials.retain(|(p, _)| !used.contains(p));
    finished
}

/// Takes renames out of `resolved`. First the `[from, to]` pairs the OS gave
/// us (Linux). Then, for macOS and Windows where the halves arrive unpaired:
/// exactly one removed and exactly one created in the batch, both touched by
/// a rename event. Anything else stays removed plus created.
fn pair_renames(state: &mut State, resolved: &mut Resolved) -> Vec<Obs> {
    let mut renamed = Vec::new();
    let mut keep = Vec::new();
    for (from, to) in std::mem::take(&mut state.pairs) {
        let removed = resolved.removed.iter().position(|r| r.path == from);
        let created = resolved.created.iter().position(|r| r.path == to);
        if let (Some(i), Some(j)) = (removed, created) {
            resolved.removed.remove(i);
            resolved.created.remove(j);
            renamed.push(Obs::Renamed { from, to });
        } else if state.pending.contains_key(&from) || state.pending.contains_key(&to) {
            // One half has not settled yet: keep the pair for the next batch.
            keep.push((from, to));
        }
    }
    state.pairs = keep;
    if let ([removed], [created]) = (resolved.removed.as_slice(), resolved.created.as_slice()) {
        if removed.name && created.name {
            renamed.push(Obs::Renamed {
                from: removed.path.clone(),
                to: created.path.clone(),
            });
            resolved.removed.clear();
            resolved.created.clear();
        }
    }
    renamed
}

impl Obs {
    pub fn kind_id(&self) -> &'static str {
        match self {
            Obs::Created(_) => "fs.file-created",
            Obs::Saved(_) => "fs.file-saved",
            Obs::Removed(_) => "fs.file-removed",
            Obs::Renamed { .. } => "fs.file-renamed",
            Obs::DownloadFinished(_) => "fs.download-finished",
        }
    }

    /// Decision 8: `path`, `name`, `relative`, `ext`, and `from` for a rename.
    pub fn payload(&self, armed: &Armed) -> Payload {
        let (path, from) = match self {
            Obs::Created(p) | Obs::Saved(p) | Obs::Removed(p) | Obs::DownloadFinished(p) => {
                (p, None)
            }
            Obs::Renamed { from, to } => (to, Some(from)),
        };
        let text = |s: &std::ffi::OsStr| Value::from(s.to_string_lossy().into_owned());
        let relative = path
            .strip_prefix(&armed.root)
            .map(|rel| {
                rel.components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .unwrap_or_default();
        let mut payload = Payload::new();
        payload.insert("path".into(), text(path.as_os_str()));
        payload.insert("name".into(), text(path.file_name().unwrap_or_default()));
        payload.insert("relative".into(), Value::from(relative));
        payload.insert("ext".into(), text(path.extension().unwrap_or_default()));
        if let Some(from) = from {
            payload.insert("from".into(), text(from.as_os_str()));
        }
        payload
    }
}

#[cfg(test)]
mod tests;
