---
id: T02
epic: 007-trigger-clock
title: Add the five clock kinds and stub the glue
status: todo
depends_on: [T01]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-clock/src/lib.rs
  - src-tauri/crates/trigger-clock/src/rule.rs
  - src-tauri/crates/trigger-clock/src/logic.rs
  - src-tauri/crates/trigger-clock/src/os.rs
  - src-tauri/crates/trigger-clock/README.md
---

## Goal

`trigger-clock` has its five real kinds, a `When` type with all six fields, a `Parsed` type, and a `ClockSource` whose `check` parses a rule. `start` does not run anything yet. The README shows the real Kinds table.

## Read first

- `plans/007-trigger-clock/epic.md` Decisions 2, 3, 10 and 11 (kinds, fields, payload)
- `src-tauri/crates/trigger-clock/src/lib.rs` the template file you replace
- `src-tauri/crates/trigger-fs/src/lib.rs:1-60` how `FieldInfo` consts are written
- `src-tauri/crates/trigger-core/src/lib.rs:100-125` `FieldInfo` and `KindInfo`

## Steps

1. Replace the whole of `src-tauri/crates/trigger-clock/src/lib.rs` with this. The doc strings in `KINDS` are what the README table is built from, so keep them as written. `#![allow(dead_code)]` is temporary: nothing calls the logic until slice C wires the thread, and that card removes it.

```rust
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
```

2. Replace the whole of `src-tauri/crates/trigger-clock/src/os.rs` with this. The clock needs no OS calls to run; later cards add the feed fetcher here.

```rust
//! The only file that talks to the operating system. The clock loop needs no
//! OS calls of its own (it reads the wall clock through `trigger_core`'s
//! `Clock`), so for now this only reports availability. Later cards add the
//! feed fetcher and the local zone name here.

use trigger_core::Availability;

pub fn availability() -> Availability {
    Availability::Available
}
```

3. Replace the whole of `src-tauri/crates/trigger-clock/src/logic.rs` with this one line:

```rust
//! Pure decisions. Filled in by the next cards.
```

4. Replace the whole of `src-tauri/crates/trigger-clock/src/rule.rs` with this. The six fields are all optional here because which ones a kind needs is checked in the next card. `cron` and `title` stay `None` until then.

```rust
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
```

5. Run `cd src-tauri && cargo fmt -p trigger-clock` once. Then run `cd src-tauri && cargo run -p trigger-clock --example readme`. It prints a table with a header row, a separator row and five rows. In `src-tauri/crates/trigger-clock/README.md` replace everything between the line `<!-- kinds -->` and the line `<!-- /kinds -->` (keep both marker lines) with that output. Leave the rest of the README as it is: card T09 writes it.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock` passes: 2 tests in the library, 1 in `readme`, and `live` shows 1 ignored
- [ ] `cd src-tauri && cargo run -q -p trigger-clock --example readme | wc -l` prints `7` (ignore leading spaces)
- [ ] `grep -c "^| clock\." src-tauri/crates/trigger-clock/README.md` prints `5`
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0. Warnings about template text (`still has template code`) are expected until the glue card in slice C replaces `start`; they do not fail the check. (`src/lib.rs` keeps the text "not implemented yet" in `start` until slice C.)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Per kind checks, cron parsing, the title glob: the next card.
- Any logic in `logic.rs`.
- Editing `tests/live.rs` (it still uses the template's `clock.example` kind but is `#[ignore]`d; slice C rewrites it).
- Writing README sections other than the Kinds table.
- Registering the crate in the app.

## If stuck

- If pasted code does not compile or clippy objects, check that you pasted it exactly and in the place the step names. If it still fails, set `status: blocked` and quote the compiler error. Do not change the code.
- If `cargo test` fails only in `readme` with "Kinds table is stale", redo step 5.
- If `cargo fmt -p trigger-clock --check` fails, run `cd src-tauri && cargo fmt -p trigger-clock` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
