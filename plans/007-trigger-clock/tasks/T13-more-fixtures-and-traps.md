---
id: T13
epic: 007-trigger-clock
title: Add the UTC, floating, Outlook and many series fixtures
status: todo
depends_on: [T12]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-clock/src/feed/fixture_tests.rs
  - src-tauri/crates/trigger-clock/tests/fixtures/utc_recurring.ics
  - src-tauri/crates/trigger-clock/tests/fixtures/floating.ics
  - src-tauri/crates/trigger-clock/tests/fixtures/outlook_gmt.ics
  - src-tauri/crates/trigger-clock/tests/fixtures/two_old_series.ics
---

## Goal

Four more fixtures prove the two calcard traps are worked around (a recurring event written with a `Z`, and a limit shared between series) and that a floating time and an Outlook `GMT Standard Time` zone come out right. Bad input gives no occurrences and no panic.

## Read first

- `plans/007-trigger-clock/research/R02-ics-and-http.md` Evidence, "Traps found by running"
- `src-tauri/crates/trigger-clock/src/feed/fixture_tests.rs` the file you append to, and its helpers `t`, `spans`, `span` and `london`
- `src-tauri/crates/trigger-clock/src/feed.rs` `expand` and `normalise`

## Steps

1. Create the four fixture files. Copy each exactly, including the final newline.

`src-tauri/crates/trigger-clock/tests/fixtures/utc_recurring.ics` (daily 09:00 to 09:30 written in UTC with a `Z`, 14 October 2026 excluded: the first trap):

```
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Terminal Grid//Fixture//EN
CALSCALE:GREGORIAN
BEGIN:VEVENT
UID:utc-daily@fixture
DTSTAMP:20260101T000000Z
DTSTART:20261001T090000Z
DTEND:20261001T093000Z
RRULE:FREQ=DAILY
EXDATE:20261014T090000Z
SUMMARY:Planning
END:VEVENT
END:VCALENDAR
```

`src-tauri/crates/trigger-clock/tests/fixtures/floating.ics` (one event with no zone, 10:00 for an hour):

```
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Terminal Grid//Fixture//EN
CALSCALE:GREGORIAN
BEGIN:VEVENT
UID:floating@fixture
DTSTAMP:20260101T000000Z
DTSTART:20261015T100000
DURATION:PT1H
SUMMARY:Lunch and learn
END:VEVENT
END:VCALENDAR
```

`src-tauri/crates/trigger-clock/tests/fixtures/outlook_gmt.ics` (Outlook style: the zone is the Windows name `GMT Standard Time`, weekly on Thursdays 14:00):

```
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Terminal Grid//Fixture//EN
CALSCALE:GREGORIAN
BEGIN:VTIMEZONE
TZID:GMT Standard Time
BEGIN:STANDARD
DTSTART:16010101T020000
TZOFFSETFROM:+0100
TZOFFSETTO:+0000
RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=-1SU;BYMONTH=10
END:STANDARD
BEGIN:DAYLIGHT
DTSTART:16010101T010000
TZOFFSETFROM:+0000
TZOFFSETTO:+0100
RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=-1SU;BYMONTH=3
END:DAYLIGHT
END:VTIMEZONE
BEGIN:VEVENT
UID:outlook-weekly@fixture
DTSTAMP:20260101T000000Z
DTSTART;TZID=GMT Standard Time:20261001T140000
DTEND;TZID=GMT Standard Time:20261001T150000
RRULE:FREQ=WEEKLY;BYDAY=TH
SUMMARY:Outlook weekly
END:VEVENT
END:VCALENDAR
```

`src-tauri/crates/trigger-clock/tests/fixtures/two_old_series.ics` (two endless daily series that started in 1990: the second trap):

```
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Terminal Grid//Fixture//EN
CALSCALE:GREGORIAN
BEGIN:VEVENT
UID:old-a@fixture
DTSTAMP:20260101T000000Z
DTSTART:19900101T090000Z
DTEND:19900101T093000Z
RRULE:FREQ=DAILY
SUMMARY:Old A
END:VEVENT
BEGIN:VEVENT
UID:old-b@fixture
DTSTAMP:20260101T000000Z
DTSTART:19900101T100000Z
DTEND:19900101T103000Z
RRULE:FREQ=DAILY
SUMMARY:Old B
END:VEVENT
END:VCALENDAR
```

2. In `src-tauri/crates/trigger-clock/src/feed/fixture_tests.rs`, find the line

```rust
use chrono_tz::Europe::London;
```

and replace it with

```rust
use chrono_tz::{Asia::Tokyo, Europe::London};
```

3. Append this text to the end of the same file, after a blank line. Change nothing above it.

```rust

const UTC_DAILY: &str = include_str!("../../tests/fixtures/utc_recurring.ics");
const FLOATING: &str = include_str!("../../tests/fixtures/floating.ics");
const OUTLOOK: &str = include_str!("../../tests/fixtures/outlook_gmt.ics");
const TWO_OLD: &str = include_str!("../../tests/fixtures/two_old_series.ics");

#[test]
fn a_recurring_event_written_in_utc_is_not_read_as_local_time() {
    // calcard reads DTSTART:...Z of a recurring event as local time unless
    // `normalise` rewrites it. The default zone must not change the answer.
    let expected = vec![span("2026-10-13T09:00:00+00:00", "2026-10-13T09:30:00+00:00")];
    let from = t("2026-10-13T00:00:00Z");
    let to = t("2026-10-13T23:00:00Z");
    let in_london = expand(UTC_DAILY, CalTz::from(London), from, to);
    assert_eq!(spans(&in_london), expected);
    let in_tokyo = expand(UTC_DAILY, CalTz::from(Tokyo), from, to);
    assert_eq!(spans(&in_tokyo), expected);
    // The excluded date is excluded too.
    let from = t("2026-10-14T00:00:00Z");
    let to = t("2026-10-14T23:00:00Z");
    assert!(expand(UTC_DAILY, CalTz::from(London), from, to)
        .occurrences
        .is_empty());
}

#[test]
fn a_floating_event_takes_the_default_zone() {
    let from = t("2026-10-15T00:00:00Z");
    let to = t("2026-10-15T23:00:00Z");
    let in_london = expand(FLOATING, CalTz::from(London), from, to);
    assert_eq!(
        spans(&in_london),
        vec![span("2026-10-15T09:00:00+00:00", "2026-10-15T10:00:00+00:00")]
    );
    let in_tokyo = expand(FLOATING, CalTz::from(Tokyo), from, to);
    assert_eq!(
        spans(&in_tokyo),
        vec![span("2026-10-15T01:00:00+00:00", "2026-10-15T02:00:00+00:00")]
    );
}

#[test]
fn an_outlook_windows_zone_name_is_understood() {
    let x = london(OUTLOOK, "2026-10-15T00:00:00Z", "2026-10-15T23:00:00Z");
    assert_eq!(
        spans(&x),
        vec![span("2026-10-15T13:00:00+00:00", "2026-10-15T14:00:00+00:00")]
    );
    assert_eq!(x.occurrences[0].title, "Outlook weekly");
}

#[test]
fn every_uid_gets_its_own_limit_so_a_second_old_series_is_not_starved() {
    // Both series start in 1990, about 13 800 days before the window. A limit
    // shared between them (20 000) would run out inside the second one.
    let x = london(TWO_OLD, "2026-10-14T00:00:00Z", "2026-10-15T23:00:00Z");
    let titles: Vec<&str> = x.occurrences.iter().map(|o| o.title.as_str()).collect();
    assert_eq!(titles, vec!["Old A", "Old B", "Old A", "Old B"]);
    assert!(x.problems.is_empty());
    // Sorted by start across the two series.
    let starts: Vec<_> = x.occurrences.iter().map(|o| o.start).collect();
    assert!(starts.windows(2).all(|w| w[0] <= w[1]));
}

#[test]
fn text_that_is_not_a_calendar_gives_no_occurrences_and_no_panic() {
    let from = t("2026-10-15T00:00:00Z");
    let to = t("2026-10-16T00:00:00Z");
    for text in ["", "<html>login</html>", "BEGIN:VCALENDAR\nEND:VCALENDAR\n"] {
        let x = expand(text, CalTz::from(London), from, to);
        assert!(x.occurrences.is_empty());
    }
}

#[test]
fn a_broken_event_is_reported_without_its_content() {
    let broken = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:x\nDTSTART:soon\nSUMMARY:Secret title\nEND:VEVENT\nEND:VCALENDAR\n";
    let from = t("2026-10-15T00:00:00Z");
    let to = t("2026-10-16T00:00:00Z");
    let x = expand(broken, CalTz::from(London), from, to);
    assert!(x.occurrences.is_empty());
    assert_eq!(x.problems.len(), 1);
    assert!(!x.problems[0].contains("Secret"));
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-clock`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock feed::fixture_tests` passes: 11 tests
- [ ] `cd src-tauri && cargo test -p trigger-clock feed::fixture_tests::a_recurring_event_written_in_utc_is_not_read_as_local_time` passes
- [ ] `cd src-tauri && cargo test -p trigger-clock` passes (all earlier tests too; `live` shows 1 ignored)
- [ ] `ls src-tauri/crates/trigger-clock/tests/fixtures | wc -l` prints `8` (ignore leading spaces)
- [ ] `wc -l src-tauri/crates/trigger-clock/src/feed/fixture_tests.rs` prints a number under 400
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (the `still has template code` warning for `src/lib.rs` is expected until slice C)
- [ ] `git status --porcelain` lists only paths under `files:` (ignore other card files under `plans/007-trigger-clock/tasks/`)

## Out of scope

- Changing `normalise`, `split_by_uid` or `expand`. If a test here fails, the card is wrong, not the code: see If stuck.
- Real calendar exports from Google, Outlook or Apple (the epic's Risks list them; a later epic or a manual test does).
- `X-WR-TIMEZONE`, `THISANDFUTURE` ranges, hourly series that began years ago.
- Extra tests.

## If stuck

- If pasted code does not compile or clippy objects, check that you pasted it exactly and in the place the step names. If it still fails, set `status: blocked` and quote the compiler error. Do not change the code.
- If the UTC test gives 08:00 instead of 09:00, `normalise` is not being called by `expand`. Set `status: blocked` and say so; do not edit `feed.rs`.
- If the two old series test gives only one series, the limit is shared again. Set `status: blocked` and say so.
- If a fixture test fails with times one hour off, a fixture was copied wrongly. Compare the file with this card, character by character.
- If `cargo fmt -p trigger-clock --check` fails, run `cd src-tauri && cargo fmt -p trigger-clock` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
