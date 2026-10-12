//! Pure decisions: given what `gh` answered, which rules fire and with what
//! payload. No child processes, no threads, no clocks: everything it needs
//! comes in as arguments, so every case is a unit test. Slice B fills it in.

// Removed in slice C, when the poll loop reads everything in this file.
#![allow(dead_code)]

pub mod backoff;
pub mod diff;
pub mod json;
pub mod payload;
pub mod pulls;
pub mod response;
pub mod runs;
pub mod simple;
pub mod text;

#[cfg(test)]
mod testutil;

use std::collections::BTreeMap;

use trigger_core::RuleId;

use crate::rule::Parsed;

/// A rule this crate is polling for.
#[derive(Debug, Clone)]
pub struct Armed {
    pub id: RuleId,
    pub kind: String,
    pub parsed: Parsed,
}

/// Everything remembered between polls: by repository (`owner/name`), the
/// seen ids and etags in `diff::RepoState`.
#[derive(Debug, Default)]
pub struct State {
    pub repos: BTreeMap<String, diff::RepoState>,
}

impl State {
    /// The memory of one repository, made empty on first use.
    pub fn repo(&mut self, repo: &str) -> &mut diff::RepoState {
        self.repos.entry(repo.to_string()).or_default()
    }
}
