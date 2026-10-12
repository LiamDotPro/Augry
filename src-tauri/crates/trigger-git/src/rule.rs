//! The `when` object of a rule, typed. Pure.

// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]

use std::path::PathBuf;

use serde::Deserialize;
use trigger_core::{Rule, RuleError};

pub const MIN_INTERVAL_S: u64 = 30;

fn default_interval_s() -> u64 {
    300
}

fn default_older_than_min() -> u64 {
    60
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct When {
    #[serde(default)]
    pub repo: Option<PathBuf>,
    #[serde(default)]
    pub folder: Option<PathBuf>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default = "default_interval_s")]
    pub interval_s: u64,
    #[serde(default = "default_older_than_min")]
    pub older_than_min: u64,
    #[serde(default)]
    pub fetch: bool,
}

/// Checks that the kind belongs to this crate and the fields parse. Used by
/// `check` (the editor) and by `start`.
pub fn parse(rule: &Rule) -> Result<When, RuleError> {
    if !crate::KINDS.iter().any(|k| k.id == rule.kind) {
        return Err(RuleError::UnknownKind(rule.kind.clone()));
    }
    let value = serde_json::Value::Object(rule.when.clone().into_iter().collect());
    serde_json::from_value(value).map_err(|e| RuleError::Field {
        field: "when".into(),
        problem: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use trigger_core::testkit::rule;

    #[test]
    fn defaults_are_filled_in() {
        let r = rule("p/t", "git.commit", json!({ "repo": "/r" }));
        let when = parse(&r).unwrap();
        assert_eq!(when.repo, Some(PathBuf::from("/r")));
        assert_eq!(when.folder, None);
        assert_eq!(when.branch, None);
        assert_eq!(when.interval_s, 300);
        assert_eq!(when.older_than_min, 60);
        assert!(!when.fetch);
    }

    #[test]
    fn rejects_unknown_fields_and_kinds() {
        let typo = rule("p/t", "git.commit", json!({ "repo": "/r", "brnach": "x" }));
        assert!(parse(&typo).is_err());
        let other = rule("p/t", "fs.file-saved", json!({ "repo": "/r" }));
        assert!(matches!(parse(&other), Err(RuleError::UnknownKind(_))));
    }
}
