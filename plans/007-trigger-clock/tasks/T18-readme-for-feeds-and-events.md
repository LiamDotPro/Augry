---
id: T18
epic: 007-trigger-clock
title: Document feeds and event kinds in the clock README
status: todo
depends_on: [T15, T17]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-clock/README.md
---

## Goal

The README describes the calendar feed code that slice B added: how a feed is read, the new files in the Develop table, and the known limits of event kinds. The Develop table has a `feed.rs` row.

## Read first

- `src-tauri/crates/trigger-clock/README.md` the file you edit; slice A's card T09 wrote it
- `plans/007-trigger-clock/epic.md` Decisions 9, 10 and 11
- `plans/007-trigger-clock/research/R02-ics-and-http.md` "Doubts" (the source of the limits below)

## Steps

1. In `src-tauri/crates/trigger-clock/README.md`, under the `## Rule fields` heading, find the line

```
`clock.daily-first-use` takes no fields.
```

and replace it with this text. It adds no new `##` heading: the README must keep exactly eight.

```
`clock.daily-first-use` takes no fields.

A `feed` is read like this. A path is read from disk. A URL is fetched with
`If-None-Match` and `If-Modified-Since`, `webcal://` counts as `https://`, a
request gives up after 15 seconds, and a feed over 10 MiB is refused. Each
distinct feed is read at most every 5 minutes, and only the events in the next
24 hours are kept. Event times written without a time zone are read in this
machine's zone. Each event fires once, however often the feed is read again.
```

2. In the Develop table, find the row

```
| `src/os.rs` | The only file that talks to the OS: `availability()` |
```

and replace that one row with these rows:

```
| `src/feed.rs` | Pure: `Fetcher`, `Fetched`, `feed_url`, `normalise`, `split_by_uid` and `expand`, which turns calendar text into the occurrences in a window. Uses `calcard` |
| `src/feed/tests.rs` | Unit tests for URLs, line unfolding, the UTC rewrite and splitting by UID |
| `src/feed/fixture_tests.rs` | `expand` against the files in `tests/fixtures/` |
| `tests/fixtures/*.ics` | Handwritten calendar files: weekly Europe/London, an excluded date, a moved occurrence, all day, UTC with a `Z`, floating, Outlook `GMT Standard Time`, and two old endless series |
| `src/logic/events.rs` | Pure: `fire_events`, called by `step` for `clock.event-starting` and `clock.event-ended` |
| `src/logic/event_tests.rs` | Unit tests for the two event kinds |
| `src/os.rs` | The only file that talks to the OS: `availability()`, `UreqFetcher` (HTTP with `ureq`), `read_feed_file` and `local_zone` |
```

3. In the Known limits section, find these two lines

```
- Not finished yet: the threads that run the logic, so `start` still returns
  unavailable, and the calendar feed code behind the two event kinds.
```

and replace them with:

```
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
```

4. Run `cd src-tauri && cargo test -p trigger-clock --test readme` to confirm the Kinds table is untouched.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock --test readme` passes (1 test)
- [ ] `grep -c "^## " src-tauri/crates/trigger-clock/README.md` prints `8`
- [ ] `grep -c "src/feed.rs" src-tauri/crates/trigger-clock/README.md` prints `1`
- [ ] `cd src-tauri && cargo test -p trigger-clock` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (the `still has template code` warning for `src/lib.rs` is expected until slice C)
- [ ] `git status --porcelain` lists only paths under `files:` (ignore other card files under `plans/007-trigger-clock/tasks/`)

## Out of scope

- Editing code or the Kinds table.
- Describing the feed thread or the live tests: slice C removes the "Not finished yet" bullet and adds them.
- Editing `plans/_context/repo-map.md`: slice D.

## If stuck

- If a "find" text is not in the README, T09 was not built as written. Set `status: blocked` and say which text.
- If the readme test fails with "Kinds table is stale", you changed something between the `<!-- kinds -->` markers. Undo that edit.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
