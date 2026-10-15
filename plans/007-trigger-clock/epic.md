---
id: 007-trigger-clock
title: trigger-clock
status: done
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

6. **Dependencies** (versions from research), added to
   `[workspace.dependencies]` and used by this crate only: `croner = "4.0.1"`
   (parsed with `Seconds::Disallowed` and `Year::Disallowed`, so exactly five
   fields), `chrono = { version = "0.4", default-features = false, features =
   ["std", "clock"] }`, `chrono-tz = "0.10"`, `calcard = { version = "0.3.14",
   default-features = false }`, `ureq = "3.4"`, and `iana-time-zone` at the
   version the card writer finds in `Cargo.lock` (or the current one) for the
   local zone. `ureq` over the app's `reqwest`, so the crate never touches
   tokio. Local time is `chrono::Local`; the IANA name feeds calcard's
   `default_tz`, falling back to floating.
7. **The loop** is R03's design exactly: one thread `trigger-clock` blocks on
   its stop channel for `min(TICK, until next due)` with `TICK` 30 s, then
   calls the pure `step(&mut State, now, rules) -> Vec<Fire>`. A wall gap over
   `JUMP` (90 s) means the machine slept: `catch_up` rules fire once for the
   latest missed time with `caught_up: true` in the payload, others skip the
   missed slots. A backwards jump fires nothing. `daily-first-use` fires when
   the local date differs from the last fire and the clock has run normally
   for `NORMAL_FOR` (60 s). No `Instant` in any decision, no OS power hooks.
   `TICK` and `JUMP` come from a `ClockConfig` so tests run in milliseconds.
8. **Cron in local time** fires at each occurrence instant croner returns, so
   an hourly pattern fires twice in the repeated autumn hour and a skipped
   spring time fires at the next valid minute (R01's observed behaviour; the
   README says so).
9. **Feeds** follow R02: a second thread `trigger-clock-feed` fetches each
   distinct feed at most every 5 minutes (`webcal://` becomes `https://`,
   `If-None-Match` and `If-Modified-Since`, 15 s timeout, 10 MiB cap; a path
   is read with `std::fs`), normalises the text (R02 step 1, the `Z` to
   `TZID=UTC` workaround), splits by UID, expands each with its own limit of
   20000 over the next 24 h, and sends the occurrence list to the clock
   thread. The fetcher is behind `trait Fetcher: Send { fn fetch(&self, url:
   &str, etag: Option<&str>, last_modified: Option<&str>) -> Result<Fetched,
   String>; }` so tests use fixture files.
10. **`when` per kind**: `clock.schedule` needs `cron`; `clock.interval` needs
    `every_s` (at least 10); `clock.event-starting` and `clock.event-ended`
    need `feed`, with optional `title` (glob, globset) and `minutes_before`
    (default 5, starting only); `catch_up` (default false) is allowed on
    schedule and interval only; `daily-first-use` takes no fields. Anything
    else is a `RuleError`.
11. **Payload**: `due` (ms), `caught_up` (bool) for schedule and interval;
    `date` for daily-first-use; `title`, `start`, `end` (ms) and `feed` for
    events. Never the event description or attendees.
12. **Registration**: cargo feature `trigger-clock` (default on) and one line
    in `sources()`, exactly like `trigger-fs`.

## Shape

```
src-tauri/crates/trigger-clock/
  src/lib.rs        KINDS (5), ClockSource, start (two threads)
  src/rule.rs       When { cron, every_s, catch_up, feed, minutes_before, title }, parse
  src/logic.rs      State, Fire, step(&mut State, now, rules), next_due, ClockConfig
  src/logic/tests.rs
  src/feed.rs       normalise, split by UID, expand(text, tz, from, to) -> Vec<Occurrence> (pure)
  src/os.rs         UreqFetcher: Fetcher, file reads, local zone name
  tests/fixtures/*.ics   R02's handwritten fixtures, including the traps
  tests/live.rs     #[ignore] interval and feed-from-file tests with a short ClockConfig
```

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A (T01 to T09) | scaffold, KINDS, `rule.rs`, `step` with cron, interval, catch up and daily first use, FixedClock style tests | |
| B (T10 to T19) | `feed.rs` expansion with fixture ICS files, the `Fetcher` trait and `UreqFetcher`, event starting and ended in `step` | A |
| C (T20 to T29) | the two threads, live tests, README | A, B |
| D (T30 to T39) | app: feature and `sources()` line, repo map row | C |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | Current, maintained cron parsing crate with local time zone support and its version | Decision 1 | croner 4.0.1, five fields via the builder, chrono 0.4.45 already in the lock. Decision 6, 8. |
| R02 | Current ICS parsing crate that handles recurring events, and its version | Decision 1 | calcard 0.3.14 with two traps and workarounds; ureq 3.4. Decision 6, 9. |
| R03 | Does a thread sleeping across a system sleep wake with the right elapsed time on each OS, or must the crate re-check the wall clock on a short tick | Decision 4 | Waits do not count sleep on any OS: 30 s tick, 90 s jump. Decision 7. |

## Risks

- Recurring events and time zones in ICS are where parsers disagree; test
  with real exports from Google Calendar, Outlook and Apple Calendar.
