//! The `when` object of a rule, typed, with its cron expression and title
//! glob compiled. Pure.

use croner::parser::{CronParser, Seconds, Year};
use croner::Cron;
use globset::{GlobBuilder, GlobMatcher};
use serde::Deserialize;
use trigger_core::{Rule, RuleError};

/// The smallest `every_s`. Shorter intervals would outrun the 30 second tick.
pub const MIN_EVERY_S: u64 = 10;

/// Minutes before the start that `clock.event-starting` fires by default.
pub const DEFAULT_MINUTES_BEFORE: u32 = 5;

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

    pub fn minutes_before(&self) -> u32 {
        self.when.minutes_before.unwrap_or(DEFAULT_MINUTES_BEFORE)
    }
}

fn field_error(field: &str, problem: impl Into<String>) -> RuleError {
    RuleError::Field {
        field: field.into(),
        problem: problem.into(),
    }
}

/// Which `when` fields a kind accepts (Decision 10). Required fields are a
/// subset, checked separately.
fn allowed(kind: &str) -> &'static [&'static str] {
    match kind {
        "clock.schedule" => &["cron", "catch_up"],
        "clock.interval" => &["every_s", "catch_up"],
        "clock.event-starting" => &["feed", "title", "minutes_before"],
        "clock.event-ended" => &["feed", "title"],
        _ => &[],
    }
}

/// The fields that were written in the rule, by name.
fn present(when: &When) -> Vec<&'static str> {
    [
        ("cron", when.cron.is_some()),
        ("every_s", when.every_s.is_some()),
        ("catch_up", when.catch_up.is_some()),
        ("feed", when.feed.is_some()),
        ("minutes_before", when.minutes_before.is_some()),
        ("title", when.title.is_some()),
    ]
    .into_iter()
    .filter_map(|(name, there)| there.then_some(name))
    .collect()
}

/// Exactly five fields, local time. Seconds and year are refused so a
/// six field expression is an error, not a silently different schedule.
fn parse_cron(text: &str) -> Result<Cron, RuleError> {
    CronParser::builder()
        .seconds(Seconds::Disallowed)
        .year(Year::Disallowed)
        .build()
        .parse(text)
        .map_err(|e| field_error("cron", e.to_string()))
}

/// Case insensitive, and `*` crosses `/` because a title is not a path.
fn parse_title(text: &str) -> Result<GlobMatcher, RuleError> {
    GlobBuilder::new(text)
        .case_insensitive(true)
        .build()
        .map(|g| g.compile_matcher())
        .map_err(|e| field_error("title", e.to_string()))
}

/// Checks that the kind belongs to this crate and the fields parse. Used by
/// `check` (the editor) and by `start`.
pub fn parse(rule: &Rule) -> Result<Parsed, RuleError> {
    if !crate::KINDS.iter().any(|k| k.id == rule.kind) {
        return Err(RuleError::UnknownKind(rule.kind.clone()));
    }
    let value = serde_json::Value::Object(rule.when.clone().into_iter().collect());
    let when: When =
        serde_json::from_value(value).map_err(|e| field_error("when", e.to_string()))?;

    let allowed = allowed(&rule.kind);
    for name in present(&when) {
        if !allowed.contains(&name) {
            return Err(field_error(name, format!("is not used by {}", rule.kind)));
        }
    }
    let missing = |name: &str| field_error(name, "is required");
    let mut cron = None;
    let mut title = None;
    match rule.kind.as_str() {
        "clock.schedule" => {
            let text = when.cron.as_deref().ok_or_else(|| missing("cron"))?;
            cron = Some(parse_cron(text)?);
        }
        "clock.interval" => {
            let every = when.every_s.ok_or_else(|| missing("every_s"))?;
            if every < MIN_EVERY_S {
                return Err(field_error(
                    "every_s",
                    format!("must be at least {MIN_EVERY_S}"),
                ));
            }
        }
        "clock.event-starting" | "clock.event-ended" => {
            match when.feed.as_deref().map(str::trim) {
                None | Some("") => return Err(missing("feed")),
                Some(_) => {}
            }
            title = when.title.as_deref().map(parse_title).transpose()?;
        }
        _ => {}
    }
    Ok(Parsed { when, cron, title })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use trigger_core::testkit::rule;

    fn parsed(kind: &str, when: serde_json::Value) -> Result<Parsed, RuleError> {
        parse(&rule("p/t", kind, when))
    }

    fn field_of(err: RuleError) -> String {
        match err {
            RuleError::Field { field, .. } => field,
            other => panic!("expected a field error, got {other:?}"),
        }
    }

    #[test]
    fn schedule_needs_a_five_field_cron() {
        let p = parsed("clock.schedule", json!({ "cron": "0 9 * * 1-5" })).unwrap();
        assert!(p.cron.is_some());
        assert!(!p.catch_up());
        let missing = parsed("clock.schedule", json!({}));
        assert_eq!(field_of(missing.unwrap_err()), "cron");
        let six = parsed("clock.schedule", json!({ "cron": "0 0 9 * * 1-5" }));
        assert_eq!(field_of(six.unwrap_err()), "cron");
        let junk = parsed("clock.schedule", json!({ "cron": "every day" }));
        assert_eq!(field_of(junk.unwrap_err()), "cron");
    }

    #[test]
    fn catch_up_is_read_on_schedule_and_interval() {
        let s = parsed(
            "clock.schedule",
            json!({ "cron": "* * * * *", "catch_up": true }),
        );
        assert!(s.unwrap().catch_up());
        let i = parsed("clock.interval", json!({ "every_s": 60, "catch_up": true }));
        assert!(i.unwrap().catch_up());
    }

    #[test]
    fn interval_needs_every_s_of_at_least_ten() {
        assert!(parsed("clock.interval", json!({ "every_s": 10 })).is_ok());
        let short = parsed("clock.interval", json!({ "every_s": 9 }));
        assert_eq!(field_of(short.unwrap_err()), "every_s");
        let missing = parsed("clock.interval", json!({}));
        assert_eq!(field_of(missing.unwrap_err()), "every_s");
        let text = parsed("clock.interval", json!({ "every_s": "60" }));
        assert_eq!(field_of(text.unwrap_err()), "when");
    }

    #[test]
    fn daily_first_use_takes_no_fields() {
        assert!(parsed("clock.daily-first-use", json!({})).is_ok());
        let extra = parsed("clock.daily-first-use", json!({ "catch_up": true }));
        assert_eq!(field_of(extra.unwrap_err()), "catch_up");
    }

    #[test]
    fn event_kinds_need_a_feed() {
        let p = parsed(
            "clock.event-starting",
            json!({ "feed": "https://x.test/a.ics" }),
        )
        .unwrap();
        assert_eq!(p.minutes_before(), 5);
        assert!(p.title.is_none());
        let missing = parsed("clock.event-ended", json!({}));
        assert_eq!(field_of(missing.unwrap_err()), "feed");
        let blank = parsed("clock.event-ended", json!({ "feed": "  " }));
        assert_eq!(field_of(blank.unwrap_err()), "feed");
    }

    #[test]
    fn minutes_before_is_for_starting_only() {
        let starting = parsed(
            "clock.event-starting",
            json!({ "feed": "/a.ics", "minutes_before": 15 }),
        );
        assert_eq!(starting.unwrap().minutes_before(), 15);
        let ended = parsed(
            "clock.event-ended",
            json!({ "feed": "/a.ics", "minutes_before": 15 }),
        );
        assert_eq!(field_of(ended.unwrap_err()), "minutes_before");
    }

    #[test]
    fn title_is_a_case_insensitive_glob_that_crosses_slashes() {
        let p = parsed(
            "clock.event-ended",
            json!({ "feed": "/a.ics", "title": "stand*" }),
        )
        .unwrap();
        let title = p.title.unwrap();
        assert!(title.is_match("Standup"));
        assert!(title.is_match("STANDUP / team"));
        assert!(!title.is_match("Retro"));
        let bad = parsed(
            "clock.event-ended",
            json!({ "feed": "/a.ics", "title": "[" }),
        );
        assert_eq!(field_of(bad.unwrap_err()), "title");
    }

    #[test]
    fn fields_from_other_kinds_are_refused() {
        let cron_on_interval = parsed(
            "clock.interval",
            json!({ "every_s": 60, "cron": "* * * * *" }),
        );
        assert_eq!(field_of(cron_on_interval.unwrap_err()), "cron");
        let catch_up_on_event = parsed(
            "clock.event-starting",
            json!({ "feed": "/a.ics", "catch_up": true }),
        );
        assert_eq!(field_of(catch_up_on_event.unwrap_err()), "catch_up");
        let feed_on_schedule = parsed(
            "clock.schedule",
            json!({ "cron": "* * * * *", "feed": "/a.ics" }),
        );
        assert_eq!(field_of(feed_on_schedule.unwrap_err()), "feed");
    }

    #[test]
    fn rejects_unknown_fields_and_kinds() {
        let typo = parsed("clock.schedule", json!({ "crn": "* * * * *" }));
        assert_eq!(field_of(typo.unwrap_err()), "when");
        let other = parsed("fs.file-saved", json!({}));
        assert!(matches!(other, Err(RuleError::UnknownKind(_))));
    }
}
