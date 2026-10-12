---
id: T12
epic: 007-trigger-clock
title: Expand calendar events with calcard, with the first four fixtures
status: todo
depends_on: [T11]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-clock/src/feed.rs
  - src-tauri/crates/trigger-clock/src/feed/fixture_tests.rs
  - src-tauri/crates/trigger-clock/tests/fixtures/weekly_london.ics
  - src-tauri/crates/trigger-clock/tests/fixtures/exdate_daily.ics
  - src-tauri/crates/trigger-clock/tests/fixtures/recurrence_id_move.ics
  - src-tauri/crates/trigger-clock/tests/fixtures/all_day.ics
---

## Goal

`feed::expand(text, tz, from, to)` parses calendar text with `calcard`, expands each UID on its own with a limit of 20000, works out each end, reads each title (an override has its own), keeps the occurrences that overlap the window, and sorts them. Four fixture `.ics` files prove a weekly Europe/London series across daylight saving, an `EXDATE`, a `RECURRENCE-ID` move and an all day series.

## Read first

- `plans/007-trigger-clock/research/R02-ics-and-http.md` Evidence (what calcard 0.3.14 returns) and "Expansion procedure" steps 2 to 4
- `src-tauri/crates/trigger-clock/src/feed.rs` the file you add to; `split_by_uid` and `normalise` are what `expand` calls
- `src-tauri/crates/trigger-clock/src/logic.rs` `Occurrence` (`uid`, `title`, `start`, `end`, all public, times are `SystemTime`)
- `src-tauri/crates/trigger-clock/src/feed/tests.rs` how the existing tests are written

## Steps

1. Create the four fixture files. They are handwritten in the style of Google (a `VTIMEZONE` for Europe/London) and are not real exports. Each is a plain text file; copy the content exactly, including the final newline.

`src-tauri/crates/trigger-clock/tests/fixtures/weekly_london.ics` (Monday and Thursday 09:00 to 09:30 London, since 2020):

```
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Terminal Grid//Fixture//EN
CALSCALE:GREGORIAN
BEGIN:VTIMEZONE
TZID:Europe/London
BEGIN:DAYLIGHT
TZOFFSETFROM:+0000
TZOFFSETTO:+0100
TZNAME:BST
DTSTART:19700329T010000
RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU
END:DAYLIGHT
BEGIN:STANDARD
TZOFFSETFROM:+0100
TZOFFSETTO:+0000
TZNAME:GMT
DTSTART:19701025T020000
RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU
END:STANDARD
END:VTIMEZONE
BEGIN:VEVENT
UID:weekly-standup@fixture
DTSTAMP:20200101T000000Z
DTSTART;TZID=Europe/London:20200106T090000
DTEND;TZID=Europe/London:20200106T093000
RRULE:FREQ=WEEKLY;BYDAY=MO,TH
SUMMARY:Standup
END:VEVENT
END:VCALENDAR
```

`src-tauri/crates/trigger-clock/tests/fixtures/exdate_daily.ics` (daily 14:00 to 15:00 London, with 14 October 2026 excluded):

```
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Terminal Grid//Fixture//EN
CALSCALE:GREGORIAN
BEGIN:VTIMEZONE
TZID:Europe/London
BEGIN:DAYLIGHT
TZOFFSETFROM:+0000
TZOFFSETTO:+0100
TZNAME:BST
DTSTART:19700329T010000
RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU
END:DAYLIGHT
BEGIN:STANDARD
TZOFFSETFROM:+0100
TZOFFSETTO:+0000
TZNAME:GMT
DTSTART:19701025T020000
RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU
END:STANDARD
END:VTIMEZONE
BEGIN:VEVENT
UID:daily-review@fixture
DTSTAMP:20260101T000000Z
DTSTART;TZID=Europe/London:20260101T140000
DTEND;TZID=Europe/London:20260101T150000
RRULE:FREQ=DAILY
EXDATE;TZID=Europe/London:20261014T140000
SUMMARY:Review
END:VEVENT
END:VCALENDAR
```

`src-tauri/crates/trigger-clock/tests/fixtures/recurrence_id_move.ics` (a daily series where 15 October 2026 was moved from 14:00 to 16:00 and renamed):

```
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Terminal Grid//Fixture//EN
CALSCALE:GREGORIAN
BEGIN:VTIMEZONE
TZID:Europe/London
BEGIN:DAYLIGHT
TZOFFSETFROM:+0000
TZOFFSETTO:+0100
TZNAME:BST
DTSTART:19700329T010000
RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU
END:DAYLIGHT
BEGIN:STANDARD
TZOFFSETFROM:+0100
TZOFFSETTO:+0000
TZNAME:GMT
DTSTART:19701025T020000
RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU
END:STANDARD
END:VTIMEZONE
BEGIN:VEVENT
UID:daily-sync@fixture
DTSTAMP:20260101T000000Z
DTSTART;TZID=Europe/London:20260101T140000
DTEND;TZID=Europe/London:20260101T150000
RRULE:FREQ=DAILY
SUMMARY:Sync
END:VEVENT
BEGIN:VEVENT
UID:daily-sync@fixture
DTSTAMP:20260101T000000Z
RECURRENCE-ID;TZID=Europe/London:20261015T140000
DTSTART;TZID=Europe/London:20261015T160000
DTEND;TZID=Europe/London:20261015T170000
SUMMARY:Sync (moved)
END:VEVENT
END:VCALENDAR
```

`src-tauri/crates/trigger-clock/tests/fixtures/all_day.ics` (an all day series with 14 October 2026 excluded):

```
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Terminal Grid//Fixture//EN
CALSCALE:GREGORIAN
BEGIN:VEVENT
UID:holiday@fixture
DTSTAMP:20260101T000000Z
DTSTART;VALUE=DATE:20261001
DTEND;VALUE=DATE:20261002
RRULE:FREQ=DAILY
EXDATE;VALUE=DATE:20261014
SUMMARY:Out of office
END:VEVENT
END:VCALENDAR
```

2. In `src-tauri/crates/trigger-clock/src/feed.rs`, insert this directly below the five `//!` lines at the top, with a blank line between them and the first `/// What a fetch returned.` line:

```rust
use std::time::SystemTime;

use calcard::common::timezone::Tz as CalTz;
use calcard::icalendar::dates::TimeOrDelta;
use calcard::icalendar::{ICalendar, ICalendarProperty, ICalendarValue};

use crate::logic::Occurrence;
```

3. In the same file, find the last two lines

```rust
#[cfg(test)]
mod tests;
```

and replace them with this text (it adds `expand`, its types, and the declaration of the new test file):

```rust
/// Most occurrences expanded from one UID. calcard applies its limit across
/// everything it is given, so one endless series would use it all up; each UID
/// is expanded on its own instead.
pub const LIMIT_PER_UID: usize = 20_000;

/// The result of expanding a feed.
#[derive(Debug, Default, PartialEq)]
pub struct Expansion {
    /// Sorted by start, then UID.
    pub occurrences: Vec<Occurrence>,
    /// Events that could not be read, described without their content.
    pub problems: Vec<String>,
}

fn title_of(ical: &ICalendar, comp_id: u32) -> String {
    ical.components
        .get(comp_id as usize)
        .and_then(|c| c.property(&ICalendarProperty::Summary))
        .and_then(|e| e.values.first())
        .and_then(ICalendarValue::as_text)
        .unwrap_or_default()
        .to_string()
}

/// Every occurrence of every event in `text` that overlaps `from` to `to`
/// (both ends included), sorted by start. `tz` is the zone for times written
/// without one; pass `CalTz::Floating` when the local zone is unknown.
pub fn expand(text: &str, tz: CalTz, from: SystemTime, to: SystemTime) -> Expansion {
    let mut out = Expansion::default();
    for (n, (uid, part)) in split_by_uid(&normalise(text)).into_iter().enumerate() {
        let Ok(ical) = ICalendar::parse(part) else {
            out.problems.push(format!("event {n} could not be parsed"));
            continue;
        };
        let expanded = ical.expand_dates(tz, LIMIT_PER_UID);
        if !expanded.errors.is_empty() {
            out.problems.push(format!(
                "event {n} had {} expansion errors",
                expanded.errors.len()
            ));
        }
        for event in expanded.events {
            let start = SystemTime::from(event.start);
            let end = match event.end {
                TimeOrDelta::Time(t) => SystemTime::from(t),
                TimeOrDelta::Delta(d) => SystemTime::from(event.start + d),
            };
            if start <= to && end >= from {
                out.occurrences.push(Occurrence {
                    uid: uid.clone(),
                    title: title_of(&ical, event.comp_id),
                    start,
                    end,
                });
            }
        }
    }
    out.occurrences
        .sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.uid.cmp(&b.uid)));
    out
}

#[cfg(test)]
mod fixture_tests;
#[cfg(test)]
mod tests;
```

4. Create `src-tauri/crates/trigger-clock/src/feed/fixture_tests.rs` with exactly this content. Card T13 appends to it.

```rust
//! `expand` against the handwritten calendar files in `tests/fixtures/`.
//! Times in the expectations are UTC; the feeds are written in other zones.

use super::*;
use chrono::DateTime;
use chrono_tz::Europe::London;

fn t(iso: &str) -> SystemTime {
    DateTime::parse_from_rfc3339(iso).unwrap().into()
}

/// The occurrences as `(start, end)` in RFC 3339 form, for easy comparison.
fn spans(x: &Expansion) -> Vec<(String, String)> {
    let show = |s: SystemTime| DateTime::<chrono::Utc>::from(s).to_rfc3339();
    x.occurrences
        .iter()
        .map(|o| (show(o.start), show(o.end)))
        .collect()
}

fn span(start: &str, end: &str) -> (String, String) {
    (start.to_string(), end.to_string())
}

fn london(text: &str, from: &str, to: &str) -> Expansion {
    expand(text, CalTz::from(London), t(from), t(to))
}

const WEEKLY: &str = include_str!("../../tests/fixtures/weekly_london.ics");
const EXDATE: &str = include_str!("../../tests/fixtures/exdate_daily.ics");
const MOVED: &str = include_str!("../../tests/fixtures/recurrence_id_move.ics");
const ALL_DAY: &str = include_str!("../../tests/fixtures/all_day.ics");

#[test]
fn a_weekly_london_event_follows_daylight_saving() {
    // Thursday 15 October 2026 is British Summer Time: 09:00 is 08:00 UTC.
    let summer = london(WEEKLY, "2026-10-13T22:00:00Z", "2026-10-15T22:00:00Z");
    assert_eq!(
        spans(&summer),
        vec![span("2026-10-15T08:00:00+00:00", "2026-10-15T08:30:00+00:00")]
    );
    assert_eq!(summer.occurrences[0].title, "Standup");
    assert_eq!(summer.occurrences[0].uid, "weekly-standup@fixture");
    // Monday 26 October is after the clocks went back: 09:00 is 09:00 UTC.
    let winter = london(WEEKLY, "2026-10-25T22:00:00Z", "2026-10-26T22:00:00Z");
    assert_eq!(
        spans(&winter),
        vec![span("2026-10-26T09:00:00+00:00", "2026-10-26T09:30:00+00:00")]
    );
}

#[test]
fn an_excluded_date_is_missing_from_a_daily_series() {
    let x = london(EXDATE, "2026-10-13T00:00:00Z", "2026-10-16T00:00:00Z");
    assert_eq!(
        spans(&x),
        vec![
            span("2026-10-13T13:00:00+00:00", "2026-10-13T14:00:00+00:00"),
            span("2026-10-15T13:00:00+00:00", "2026-10-15T14:00:00+00:00"),
        ]
    );
}

#[test]
fn a_moved_occurrence_appears_once_with_its_own_title() {
    let x = london(MOVED, "2026-10-15T00:00:00Z", "2026-10-16T00:00:00Z");
    assert_eq!(
        spans(&x),
        vec![span("2026-10-15T15:00:00+00:00", "2026-10-15T16:00:00+00:00")]
    );
    assert_eq!(x.occurrences[0].title, "Sync (moved)");
    // The day before is not moved.
    let before = london(MOVED, "2026-10-14T00:00:00Z", "2026-10-15T00:00:00Z");
    assert_eq!(before.occurrences[0].title, "Sync");
}

#[test]
fn all_day_events_run_midnight_to_midnight_in_the_default_zone() {
    let x = london(ALL_DAY, "2026-10-13T00:00:00Z", "2026-10-15T22:00:00Z");
    // 14 October is excluded. British Summer Time: local midnight is 23:00 UTC.
    assert_eq!(
        spans(&x),
        vec![
            span("2026-10-12T23:00:00+00:00", "2026-10-13T23:00:00+00:00"),
            span("2026-10-14T23:00:00+00:00", "2026-10-15T23:00:00+00:00"),
        ]
    );
    assert_eq!(x.occurrences[0].title, "Out of office");
}

#[test]
fn the_window_keeps_events_that_overlap_it_and_drops_the_rest() {
    // The Review event on 15 October runs 13:00 to 14:00 UTC.
    let count = |from: &str, to: &str| london(EXDATE, from, to).occurrences.len();
    assert_eq!(count("2026-10-15T13:30:00Z", "2026-10-15T13:40:00Z"), 1);
    assert_eq!(count("2026-10-15T14:00:00Z", "2026-10-15T14:00:00Z"), 1);
    assert_eq!(count("2026-10-15T14:00:01Z", "2026-10-15T14:30:00Z"), 0);
    assert_eq!(count("2026-10-15T12:00:00Z", "2026-10-15T13:00:00Z"), 1);
    assert_eq!(count("2026-10-15T12:00:00Z", "2026-10-15T12:59:59Z"), 0);
}
```

5. Run `cd src-tauri && cargo fmt -p trigger-clock`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock feed::fixture_tests` passes: 5 tests
- [ ] `cd src-tauri && cargo test -p trigger-clock` passes (all earlier tests too; `live` shows 1 ignored)
- [ ] `ls src-tauri/crates/trigger-clock/tests/fixtures` lists `all_day.ics exdate_daily.ics recurrence_id_move.ics weekly_london.ics`
- [ ] `wc -l src-tauri/crates/trigger-clock/src/feed.rs` prints a number under 400
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (the `still has template code` warning for `src/lib.rs` is expected until slice C)
- [ ] `git status --porcelain` lists only paths under `files:` (ignore other card files under `plans/007-trigger-clock/tasks/`)

## Out of scope

- The other four fixtures (UTC recurring, floating, Outlook, two old series): card T13.
- Choosing the zone: `expand` takes it as an argument. Reading the machine's zone is card T15.
- Logging `problems`, caching, polling: slice C.
- Real calendar exports, extra tests, renaming anything.

## If stuck

- If pasted code does not compile or clippy objects, check that you pasted it exactly and in the place the step names. If it still fails, set `status: blocked` and quote the compiler error. Do not change the code.
- If the compiler says an item of `calcard` does not exist (`calcard::icalendar::dates::TimeOrDelta`, `ICalendarProperty::Summary`, `as_text`), the pinned version is not what the card was written against. Run `cd src-tauri && cargo tree -p trigger-clock -i calcard`, write the version in the Log, set `status: blocked` and stop.
- If a fixture test fails with times one hour off, a fixture was copied wrongly (a missing `VTIMEZONE` or `TZID` line). Compare the file with this card, character by character.
- If `cargo fmt -p trigger-clock --check` fails, run `cd src-tauri && cargo fmt -p trigger-clock` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
