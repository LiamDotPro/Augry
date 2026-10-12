---
id: 007-trigger-clock
title: trigger-clock
status: draft
depends_on: [001-trigger-core]
---

# 007 trigger-clock

## Goal

The crate `src-tauri/crates/trigger-clock` fires on a schedule, on the first
use of the day, on a plain interval, and before and after events in a calendar
feed (an ICS URL or file). It catches up after sleep when a rule asks for it.

## Non goals

- The system calendar on macOS: `trigger-macos`. Reminders: the same.

## Decisions

1. **Crate** `trigger-clock`, source id `clock`, depends on `trigger-core`,
   a cron expression parser and an ICS parser at the versions research finds,
   and `ureq` or the app's `reqwest` for feeds (research decides).
2. **Kinds**: `clock.schedule`, `clock.interval`, `clock.daily-first-use`,
   `clock.event-starting`, `clock.event-ended`.
3. **`when`**: `cron` (five fields, local time), `every_s`, `catch_up`
   (default false), `feed` (URL or path), `minutes_before`, `title` (glob).
4. **Time** comes from the `Clock` trait in the test kit, so every schedule is
   unit tested with a fixed clock. The thread sleeps until the next due time
   and re-plans on wake.
5. **`daily-first-use`** fires the first time the app sees the machine in use
   on a calendar day: the first tick after midnight local time at which the
   wall clock has moved normally for a minute (so a machine that slept through
   the night fires on wake). It reads no idle signal and needs no other crate;
   a process that wants "first time I unlock" uses `system.unlock` with a
   condition instead.

## Shape

Written during planning: `KINDS`, `When`, the pure `next_due(now, rule)` and
`catch_up` logic, the feed poller behind a trait.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A | kinds, `When`, cron and interval scheduling with fixed clock tests | |
| B | the ICS feed poller with a fake fetcher and fixture files | A |
| C | the thread, sleep and re-plan, live tests, README, feature flag, app registration, repo map | A, B |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | Current, maintained cron parsing crate with local time zone support and its version | Decision 1 | |
| R02 | Current ICS parsing crate that handles recurring events, and its version | Decision 1 | |
| R03 | Does a thread sleeping across a system sleep wake with the right elapsed time on each OS, or must the crate re-check the wall clock on a short tick | Decision 4 | |

## Risks

- Recurring events and time zones in ICS are where parsers disagree; test
  with real exports from Google Calendar, Outlook and Apple Calendar.
