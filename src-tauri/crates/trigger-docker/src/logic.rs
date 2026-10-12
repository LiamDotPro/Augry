//! Pure decisions: given an event the engine printed, which rules fire and with
//! what payload. No child processes, no threads, no clocks: every case is a
//! unit test.

// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]

use std::collections::BTreeMap;

use trigger_core::{RuleId, Value};

use crate::rule::Parsed;

/// A rule this crate is watching for.
#[derive(Debug, Clone)]
pub struct Armed {
    pub id: RuleId,
    pub kind: String,
    pub when: Parsed,
}

pub type Payload = BTreeMap<String, Value>;

#[cfg(test)]
mod tests;
