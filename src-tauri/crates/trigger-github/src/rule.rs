//! The `when` object of a rule, typed. Pure.

// Removed in slice C, when the poll loop reads everything in this file.
#![allow(dead_code)]

use serde::Deserialize;
use trigger_core::{Rule, RuleError};

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct When {
    pub repo: String,
}

/// A parsed rule: the typed fields plus anything compiled from them.
#[derive(Debug, Clone)]
pub struct Parsed {
    pub when: When,
}

/// Checks that the kind belongs to this crate and the fields parse. Used by
/// `check` (the editor) and by `start`.
pub fn parse(rule: &Rule) -> Result<Parsed, RuleError> {
    if !crate::KINDS.iter().any(|k| k.id == rule.kind) {
        return Err(RuleError::UnknownKind(rule.kind.clone()));
    }
    let value = serde_json::Value::Object(rule.when.clone().into_iter().collect());
    let when: When = serde_json::from_value(value).map_err(|e| RuleError::Field {
        field: "when".into(),
        problem: e.to_string(),
    })?;
    Ok(Parsed { when })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use trigger_core::testkit::rule;

    fn field_of(err: RuleError) -> String {
        match err {
            RuleError::Field { field, .. } => field,
            other => panic!("expected a field error, got {other:?}"),
        }
    }

    #[test]
    fn parses_the_repo() {
        let r = rule("p/t", "github.issue-assigned", json!({ "repo": "cli/cli" }));
        assert_eq!(parse(&r).unwrap().when.repo, "cli/cli");
    }

    #[test]
    fn every_kind_is_accepted() {
        for kind in crate::KINDS {
            let r = rule("p/t", kind.id, json!({ "repo": "cli/cli" }));
            assert!(parse(&r).is_ok(), "{} should parse", kind.id);
        }
    }

    #[test]
    fn rejects_unknown_fields_missing_repo_and_other_kinds() {
        let typo = rule(
            "p/t",
            "github.ci-failed",
            json!({ "repo": "a/b", "brach": "x" }),
        );
        assert_eq!(field_of(parse(&typo).unwrap_err()), "when");
        let missing = rule("p/t", "github.ci-failed", json!({}));
        assert_eq!(field_of(parse(&missing).unwrap_err()), "when");
        let other = rule("p/t", "fs.file-saved", json!({ "repo": "a/b" }));
        assert!(matches!(parse(&other), Err(RuleError::UnknownKind(_))));
    }
}
