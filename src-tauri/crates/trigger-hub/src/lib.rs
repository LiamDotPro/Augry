//! The hub: owns the trigger sources, applies a rule set to them, cools down
//! noisy rules and passes fired events to subscribers. See
//! docs/process-triggers.md Part 2.

pub mod cool_down;

use trigger_core::{Availability, Event, KindInfo, Source};

#[cfg(test)]
mod tests;

/// Owns the sources. Every method takes `&self`, so the app can keep one in
/// Tauri state.
pub struct Hub {
    sources: Vec<Box<dyn Source>>,
}

/// One source as the frontend sees it.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub id: &'static str,
    pub kinds: Vec<KindInfo>,
    pub availability: Availability,
}

/// An event that passed the cool down, as sent to subscribers.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fired {
    pub event: Event,
    /// How many events for the same rule were dropped by the cool down since
    /// the last `Fired` for that rule.
    pub coalesced: u32,
}

/// What happened to one rule in `Hub::apply`.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum RuleState {
    Active,
    Rejected { reason: String },
    SourceFailed { reason: String },
}

impl Hub {
    pub fn new(sources: Vec<Box<dyn Source>>) -> Hub {
        Hub { sources }
    }

    /// Every source with its kinds and whether it can run here.
    pub fn catalogue(&self) -> Vec<SourceInfo> {
        self.sources
            .iter()
            .map(|s| SourceInfo {
                id: s.id(),
                kinds: s.kinds().to_vec(),
                availability: s.availability(),
            })
            .collect()
    }
}
