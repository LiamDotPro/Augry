//! Helpers shared by the tests in this folder. Compiled for tests only.

use serde_json::Value;
use trigger_core::testkit::rule;

use super::Armed;

/// A rule as the poll loop holds it, parsed by the real `rule::parse`.
pub fn armed(id: &str, kind: &str, when: Value) -> Armed {
    let r = rule(id, kind, when);
    Armed {
        id: r.id.clone(),
        kind: r.kind.clone(),
        parsed: crate::rule::parse(&r).unwrap(),
    }
}
