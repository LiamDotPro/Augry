# trigger-clock

Fires on a schedule, an interval, the first use of the day, and around calendar events.

Source id `clock`. One crate, one source: everything it fires starts with
`clock.`.

## Kinds

Generated from `KINDS` in `src/lib.rs`. Refresh with
`cargo run -p trigger-clock --example readme` and paste the output between
the markers; `tests/readme.rs` fails while this table is stale.

<!-- kinds -->
| Kind | Title | Platforms | Permission | When | Payload |
| --- | --- | --- | --- | --- | --- |
| clock.schedule | Schedule | macOS, Windows, Linux | none | cron*, catch_up | due, caught_up |
| clock.interval | Interval | macOS, Windows, Linux | none | every_s*, catch_up | due, caught_up |
| clock.daily-first-use | First use of the day | macOS, Windows, Linux | none | none | date |
| clock.event-starting | Event starting | macOS, Windows, Linux | none | feed*, title, minutes_before | title, start, end, feed |
| clock.event-ended | Event ended | macOS, Windows, Linux | none | feed*, title | title, start, end, feed |
<!-- /kinds -->

## Rule fields

The `when` object of a trigger, parsed by `src/rule.rs`. Unknown fields are an
error, so a typo shows up in the editor. A field a kind does not use is an
error too.

| Field | Used by | Required | Default | Meaning |
|---|---|---|---|---|
| `cron` | `clock.schedule` | yes | | Five field cron expression in local time: minute, hour, day of month, month, day of week. Six fields (with seconds) are refused |
| `every_s` | `clock.interval` | yes | | Seconds between firings, at least 10 |
| `catch_up` | `clock.schedule`, `clock.interval` | no | `false` | After the machine slept, fire once for the latest missed time. Without it, missed times are skipped |
| `feed` | `clock.event-starting`, `clock.event-ended` | yes | | An `https://` or `webcal://` URL, or an absolute path to an `.ics` file |
| `title` | `clock.event-starting`, `clock.event-ended` | no | every event | Glob matched against the event title, case insensitive; `*` also matches `/` |
| `minutes_before` | `clock.event-starting` | no | `5` | Minutes before the start that the rule fires |

`clock.daily-first-use` takes no fields.

A `feed` is read like this. A path is read from disk. A URL is fetched with
`If-None-Match` and `If-Modified-Since`, `webcal://` counts as `https://`, a
request gives up after 15 seconds, and a feed over 10 MiB is refused. Each
distinct feed is read at most every 5 minutes, and only the events in the next
24 hours are kept. Event times written without a time zone are read in this
machine's zone. Each event fires once, however often the feed is read again.

## Payload

What an event carries, available to steps as `${trigger.<field>}`. Never an
event description or its attendees.

| Field | Kinds | Meaning |
|---|---|---|
| `due` | `clock.schedule`, `clock.interval` | The time the rule was due, milliseconds since 1970 |
| `caught_up` | `clock.schedule`, `clock.interval` | `true` when the firing stands for times missed while the machine slept |
| `date` | `clock.daily-first-use` | The local date, `YYYY-MM-DD` |
| `title` | event kinds | The event title |
| `start` | event kinds | When the event starts, milliseconds since 1970 |
| `end` | event kinds | When the event ends, milliseconds since 1970 |
| `feed` | event kinds | The feed the event came from, as written in the rule |

## Platforms

| Platform | How it is detected | Limits |
|---|---|---|
| macOS | The wall clock is read at least every 30 seconds; the local zone comes from `chrono::Local` | Same on every platform |
| Windows | Same | Same |
| Linux | Same | Same |

A gap of more than 90 seconds between two readings means the machine slept.
Waits do not count time spent asleep on any of the three systems, so the crate
never trusts the length of a wait, only the wall clock.

## Permissions

None. Reading a calendar file or feed needs no permission from the OS.

## Example

```json
{ "id": "morning", "kind": "clock.schedule", "when": { "cron": "0 9 * * 1-5", "catch_up": true } }
```

```json
{ "id": "poll", "kind": "clock.interval", "when": { "every_s": 300 } }
```

```json
{ "id": "hello", "kind": "clock.daily-first-use", "when": {} }
```

## Develop

| File | What it holds |
|---|---|
| `src/lib.rs` | `KINDS`, `ClockSource` and the thread that joins the wall clock to the logic. Glue only |
| `src/rule.rs` | The typed `When`, `Parsed` and `parse`, with the per kind checks. Pure, unit tested |
| `src/logic.rs` | Pure: `State`, `Fire`, `next_due`, `step` and `wait`. Takes `now` as an argument and never sleeps |
| `src/logic/tests.rs` | Unit tests for normal ticks, jumps, catch up, backwards jumps and daily first use |
| `src/logic/dst_tests.rs` | Unit tests for the daylight saving cases, in the America/New_York zone |
| `src/feed.rs` | Pure: `Fetcher`, `Fetched`, `feed_url`, `normalise`, `split_by_uid` and `expand`, which turns calendar text into the occurrences in a window. Uses `calcard` |
| `src/feed/tests.rs` | Unit tests for URLs, line unfolding, the UTC rewrite and splitting by UID |
| `src/feed/fixture_tests.rs` | `expand` against the files in `tests/fixtures/` |
| `tests/fixtures/*.ics` | Handwritten calendar files: weekly Europe/London, an excluded date, a moved occurrence, all day, UTC with a `Z`, floating, Outlook `GMT Standard Time`, and two old endless series |
| `src/logic/events.rs` | Pure: `fire_events`, called by `step` for `clock.event-starting` and `clock.event-ended` |
| `src/logic/event_tests.rs` | Unit tests for the two event kinds |
| `src/os.rs` | The only file that talks to the OS: `availability()`, `UreqFetcher` (HTTP with `ureq`), `read_feed_file` and `local_zone` |
| `tests/live.rs` | `#[ignore]` tests against the real clock |
| `tests/readme.rs` | Fails when the Kinds table above is stale |

```
cd src-tauri
cargo test -p trigger-clock                       # pure and readme tests, any OS, offline
cargo test -p trigger-clock -- --ignored          # live tests on this machine
cargo clippy -p trigger-clock --all-targets -- -D warnings
```

## Known limits

- Cron runs in local time, at each instant the cron library returns. A time
  that spring forward skips (such as `30 2 * * *`) runs at the first valid
  minute after the gap (03:00). A fixed time inside the repeated autumn hour
  (`30 1 * * *`) runs once. An hourly pattern (`0 * * * *`) runs twice in the
  repeated hour, once for each 01:00.
- A firing can be up to 30 seconds late, and later still right after the
  machine wakes: the first check after waking is when missed times are found.
- A process frozen for more than 90 seconds looks the same as a sleep.
- If the wall clock is set backwards, nothing fires for that check and every
  schedule is planned again from the new time, so a time can fire twice.
- `clock.daily-first-use` remembers the last day only in memory. After the app
  restarts it fires again on the same day, a minute after it starts.
- An event rule fires from its moment (the start minus `minutes_before`, or the
  end) until 10 minutes after. If the app only sees an event later than that,
  for example after a long sleep, that firing is skipped.
- A recurring event is expanded for at most 20000 occurrences from its first
  date. An hourly or finer series that began years ago can show no
  occurrences.
- A recurring event whose times are written in UTC (with a `Z`) is rewritten
  before it is parsed, because the calendar library reads those as local time.
- Feeds are checked with the certificates bundled in the app, not the system
  store, so a feed behind a company proxy that re-signs HTTPS can fail.
- Not finished yet: the threads that run the logic and fetch feeds, so `start`
  still returns unavailable.
