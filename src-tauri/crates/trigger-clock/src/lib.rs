//! Fires on a schedule, on a plain interval, on the first use of the day, and
//! before and after calendar events. See README.md.
//!
//! Glue only: the kinds this crate fires, the `Source` implementation, and the
//! threads that feed the wall clock and calendar feeds through the pure logic
//! into the sink. Rules live in `rule.rs`, decisions in `logic.rs`, the OS in
//! `os.rs`.

// Temporary: nothing calls the logic until the glue card wires the thread.
// That card removes this line.
#![allow(dead_code)]

mod feed;
mod logic;
mod os;
mod rule;

use trigger_core::{
    Availability, FieldInfo, Handle, KindInfo, Platforms, Rule, RuleError, Sink, Source,
    SourceError,
};

const CRON: FieldInfo = FieldInfo {
    name: "cron",
    doc: "Five field cron expression in local time: minute hour day-of-month month day-of-week",
    required: true,
};

const EVERY_S: FieldInfo = FieldInfo {
    name: "every_s",
    doc: "Seconds between firings, at least 10",
    required: true,
};

const CATCH_UP: FieldInfo = FieldInfo {
    name: "catch_up",
    doc: "After the machine slept, fire once for the latest missed time. Default false",
    required: false,
};

const FEED: FieldInfo = FieldInfo {
    name: "feed",
    doc: "Calendar feed: an https:// or webcal:// URL, or an absolute path to an .ics file",
    required: true,
};

const TITLE: FieldInfo = FieldInfo {
    name: "title",
    doc: "Only events whose title matches this glob, case insensitive. Default every event",
    required: false,
};

const MINUTES_BEFORE: FieldInfo = FieldInfo {
    name: "minutes_before",
    doc: "How many minutes before the event starts to fire. Default 5",
    required: false,
};

const PAYLOAD_DUE: &[FieldInfo] = &[
    FieldInfo {
        name: "due",
        doc: "The time the rule was due, milliseconds since 1970",
        required: true,
    },
    FieldInfo {
        name: "caught_up",
        doc: "true when this firing stands for times missed while the machine slept",
        required: true,
    },
];

const PAYLOAD_DAILY: &[FieldInfo] = &[FieldInfo {
    name: "date",
    doc: "The local date, YYYY-MM-DD",
    required: true,
}];

const PAYLOAD_EVENT: &[FieldInfo] = &[
    FieldInfo {
        name: "title",
        doc: "The event title",
        required: true,
    },
    FieldInfo {
        name: "start",
        doc: "When the event starts, milliseconds since 1970",
        required: true,
    },
    FieldInfo {
        name: "end",
        doc: "When the event ends, milliseconds since 1970",
        required: true,
    },
    FieldInfo {
        name: "feed",
        doc: "The feed the event came from, as written in the rule",
        required: true,
    },
];

/// Every kind this crate fires. The ids are a contract with saved process
/// definitions: add kinds, never rename one.
pub static KINDS: &[KindInfo] = &[
    KindInfo {
        id: "clock.schedule",
        title: "Schedule",
        summary: "A cron schedule in local time is due.",
        platforms: Platforms::ALL,
        permission: None,
        when: &[CRON, CATCH_UP],
        payload: PAYLOAD_DUE,
    },
    KindInfo {
        id: "clock.interval",
        title: "Interval",
        summary: "A fixed number of seconds has passed since the last firing.",
        platforms: Platforms::ALL,
        permission: None,
        when: &[EVERY_S, CATCH_UP],
        payload: PAYLOAD_DUE,
    },
    KindInfo {
        id: "clock.daily-first-use",
        title: "First use of the day",
        summary: "The first time today the app sees the machine in use, including on wake.",
        platforms: Platforms::ALL,
        permission: None,
        when: &[],
        payload: PAYLOAD_DAILY,
    },
    KindInfo {
        id: "clock.event-starting",
        title: "Event starting",
        summary: "A calendar event starts in a few minutes.",
        platforms: Platforms::ALL,
        permission: None,
        when: &[FEED, TITLE, MINUTES_BEFORE],
        payload: PAYLOAD_EVENT,
    },
    KindInfo {
        id: "clock.event-ended",
        title: "Event ended",
        summary: "A calendar event has just ended.",
        platforms: Platforms::ALL,
        permission: None,
        when: &[FEED, TITLE],
        payload: PAYLOAD_EVENT,
    },
];

pub struct ClockSource;

impl Source for ClockSource {
    fn id(&self) -> &'static str {
        "clock"
    }

    fn kinds(&self) -> &'static [KindInfo] {
        KINDS
    }

    fn availability(&self) -> Availability {
        os::availability()
    }

    fn check(&self, rule: &Rule) -> Result<(), RuleError> {
        rule::parse(rule).map(|_| ())
    }

    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError> {
        for r in &rules {
            rule::parse(r).map_err(|e| SourceError::Os(e.to_string()))?;
        }
        // The thread is added by the glue card.
        let _ = sink;
        Err(SourceError::Unavailable("not implemented yet".into()))
    }
}
