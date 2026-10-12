//! Pure decisions: given what `gh` answered, which rules fire and with what
//! payload. No child processes, no threads, no clocks: everything it needs
//! comes in as arguments, so every case is a unit test. Slice B fills it in.

// Removed in slice C, when the poll loop reads everything in this file.
#![allow(dead_code)]

pub mod backoff;
pub mod json;
pub mod response;
pub mod text;

use trigger_core::RuleId;

use crate::rule::Parsed;

/// A rule this crate is polling for.
#[derive(Debug, Clone)]
pub struct Armed {
    pub id: RuleId,
    pub kind: String,
    pub parsed: Parsed,
}

/// Everything remembered between polls (seen ids, etags). Filled in by slice B.
#[derive(Debug, Default)]
pub struct State {}
