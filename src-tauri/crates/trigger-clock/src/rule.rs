//! The `when` object of a rule, typed, with its cron expression and title
//! glob compiled. Pure.

use croner::Cron;
use globset::GlobMatcher;
use serde::Deserialize;
use trigger_core::{Rule, RuleError};

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct When {
    pub cron: Option<String>,
    pub every_s: Option<u64>,
    pub catch_up: Option<bool>,
    pub feed: Option<String>,
    pub minutes_before: Option<u32>,
    pub title: Option<String>,
}

/// A parsed rule: the typed fields plus the compiled cron expression and
/// title matcher, present only for the kinds that use them.
#[derive(Debug, Clone)]
pub struct Parsed {
    pub when: When,
    pub cron: Option<Cron>,
    pub title: Option<GlobMatcher>,
}

impl Parsed {
    pub fn catch_up(&self) -> bool {
        self.when.catch_up.unwrap_or(false)
    }
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
    // Per kind checks, the cron expression and the title glob come in the
    // next card.
    Ok(Parsed {
        when,
        cron: None,
        title: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use trigger_core::testkit::rule;

    #[test]
    fn reads_every_field() {
        let r = rule(
            "p/t",
            "clock.schedule",
            json!({ "cron": "0 9 * * 1-5", "every_s": 60, "catch_up": true,
                    "feed": "/x.ics", "minutes_before": 10, "title": "Standup*" }),
        );
        let p = parse(&r).unwrap();
        assert_eq!(p.when.cron.as_deref(), Some("0 9 * * 1-5"));
        assert_eq!(p.when.every_s, Some(60));
        assert!(p.catch_up());
        assert_eq!(p.when.feed.as_deref(), Some("/x.ics"));
        assert_eq!(p.when.minutes_before, Some(10));
        assert_eq!(p.when.title.as_deref(), Some("Standup*"));
    }

    #[test]
    fn rejects_unknown_fields_and_kinds() {
        let typo = rule("p/t", "clock.schedule", json!({ "crn": "* * * * *" }));
        assert!(matches!(parse(&typo), Err(RuleError::Field { .. })));
        let other = rule("p/t", "fs.file-saved", json!({}));
        assert!(matches!(parse(&other), Err(RuleError::UnknownKind(_))));
    }
}
