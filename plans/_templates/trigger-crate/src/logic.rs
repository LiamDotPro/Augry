//! Pure decisions: given what the OS reported, which rules fire and with what
//! payload. No OS calls, no threads, no clocks: everything it needs comes in
//! as arguments, so every case is a unit test.

use std::collections::BTreeMap;

use trigger_core::{RuleId, Value};

use crate::rule::When;

/// A rule this crate is watching for.
#[derive(Debug, Clone)]
pub struct Armed {
    pub id: RuleId,
    pub kind: String,
    pub when: When,
}

/// What `os.rs` reports, already reduced to what this crate cares about.
// Template: remove this allow once os.rs sends signals.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Example { value: String },
}

/// Anything the logic must remember between signals (debounce, last state).
#[derive(Debug, Default)]
pub struct State {}

pub type Payload = BTreeMap<String, Value>;

/// The rules a signal fires: (rule id, kind, payload).
pub fn step(_state: &mut State, armed: &[Armed], signal: Signal) -> Vec<(RuleId, String, Payload)> {
    match signal {
        Signal::Example { value } => armed
            .iter()
            .filter(|a| a.when.example == value)
            .map(|a| {
                let payload = BTreeMap::from([("example".to_string(), Value::from(value.clone()))]);
                (a.id.clone(), a.kind.clone(), payload)
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn armed(value: &str) -> Armed {
        Armed { id: "p/t".into(), kind: "{{name}}.example".into(), when: When { example: value.into() } }
    }

    #[test]
    fn fires_for_a_matching_signal_only() {
        let mut state = State::default();
        let rules = [armed("a")];
        assert_eq!(step(&mut state, &rules, Signal::Example { value: "a".into() }).len(), 1);
        assert!(step(&mut state, &rules, Signal::Example { value: "b".into() }).is_empty());
    }
}
