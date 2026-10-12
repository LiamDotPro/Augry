//! Pure decisions: given what the OS reported, which rules fire and with what
//! payload. No OS calls, no threads, no clocks: time and "is this a file"
//! come in as arguments, so every case is a unit test.

// Removed in T27, when lib.rs reads everything in this file.
#![allow(dead_code)]

use std::collections::BTreeMap;
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

/// Everything remembered between signals for one rule.
#[derive(Debug, Default)]
pub struct State {}

pub type Payload = BTreeMap<String, Value>;

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

#[cfg(test)]
mod tests;
