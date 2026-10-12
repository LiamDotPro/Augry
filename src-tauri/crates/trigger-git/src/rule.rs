//! The `when` object of a rule, typed. Pure.

use serde::Deserialize;
use trigger_core::{Rule, RuleError};

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct When {
    pub example: String,
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
    use trigger_core::testkit::rule;

    #[test]
    fn parses_the_fields() {
        let r = rule("p/t", "git.example", serde_json::json!({ "example": "x" }));
        assert_eq!(
            parse(&r).unwrap(),
            When {
                example: "x".into()
            }
        );
    }

    #[test]
    fn rejects_unknown_fields_and_kinds() {
        let typo = rule("p/t", "git.example", serde_json::json!({ "exmaple": "x" }));
        assert!(parse(&typo).is_err());
        let other = rule(
            "p/t",
            "fs.file-saved",
            serde_json::json!({ "example": "x" }),
        );
        assert!(matches!(parse(&other), Err(RuleError::UnknownKind(_))));
    }
}
