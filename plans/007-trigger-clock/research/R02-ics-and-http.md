---
id: R02
epic: 007-trigger-clock
question: Current ICS parsing crate that handles recurring events (RRULE, EXDATE, RECURRENCE-ID) and time zones, and its version; and ureq vs reqwest blocking for fetching the feed
status: answered
---

## Answer

ICS: `calcard 0.3.14` alone (parse plus `ICalendar::expand_dates`) gives start and end instants for RRULE, EXDATE, RECURRENCE-ID, VTIMEZONE, TZID, UTC, floating and all day. No second crate needed. Two traps: the `limit` is a global cap across all series (one infinite RRULE eats it), and a recurring event with a `...Z` DTSTART is wrongly treated as local time. Both have tested workarounds below.
HTTP: `ureq 3.4.2` (blocking, rustls with ring, 42 packages total standalone). `reqwest` blocking would add only `futures-io` to the app lock and works in a plain thread, but it ties the crate to the app's version; recommend ureq.
ETag and Last-Modified: both work on both clients, a 304 comes back as `Ok` with status 304 in both.

## Evidence

All run on 2026-10-12 in a scratch dir (`mktemp -d`), Rust 1.99.0. Registry versions read from crates.io API: calcard 0.3.14 (2026-09-15), icalendar 0.17.14, rrule 0.14.0 (2025-04-20), ical 0.11.0 (2024-03-13, no expansion), ureq 3.4.2, reqwest 0.13.5 (app lock has 0.13.4 at `src-tauri/Cargo.lock:3307`; app dep `src-tauri/Cargo.toml:50`).

Why calcard: `icalendar` and `ical` only parse. `rrule` expands but you must hand-glue DTSTART, TZID, VTIMEZONE, EXDATE and RECURRENCE-ID. calcard 0.3.14 has `ICalendar::expand_dates(default_tz, limit) -> CalendarExpand { events, errors }` (source `calcard-0.3.14/src/icalendar/dates.rs:65`). Each `CalendarEvent { comp_id, start: DateTime<Tz>, end: TimeOrDelta<DateTime<Tz>, TimeDelta> }`. `end` is `Time(dt)` when DTEND is present and `Delta(d)` for DURATION, recurring events, or no end. It resolves VTIMEZONE and also Windows names (Outlook `TZID=GMT Standard Time` came out as Europe/London). Deps with `default-features = false`: chrono, chrono-tz, ahash, nom-style parser, mail-parser, mail-builder (about 20 packages in the scratch tree). `jmap` default feature adds serde, serde_json, uuid, jmap-tools; drop it.

Fixtures run (Google style VTIMEZONE Europe/London, now = 2026-10-13T22:00Z, window 48h):
- Weekly MO,TH 09:00 TZID=Europe/London from 2020: Thu 2026-10-15 08:00Z (BST, DST correct), 30 min.
- Daily 14:00 London with EXDATE for 2026-10-14: that day absent.
- RECURRENCE-ID override of 2026-10-15 14:00 moved to 16:00: occurs once at 15:00Z, original not duplicated.
- All day `VALUE=DATE`: start is local midnight in the default tz, end is next midnight. With default tz London, a recurring all day series gave 23:00Z for Oct 13, EXDATE;VALUE=DATE removed Oct 14 correctly.
- UTC `DTSTART:...Z` single event: correct instant.
- Floating DTSTART with DURATION: takes the `default_tz` passed in (London gave 10:00 London = 09:00Z).
- Outlook Windows TZID: correct.

Traps found by running:
1. `limit` is one counter shared by all recurring series in the call, and an infinite RRULE runs until it hits it. With two series, the first took all 9998 and the second produced zero events. Output is also unsorted and expands from DTSTART, so a daily series from 2010 needs about 6300 steps to reach today. Cost measured: limit 5000 is 8 ms, 100000 is 167 ms.
2. A recurring event with `DTSTART:20261001T090000Z` is expanded as if 09:00 were local time in `default_tz` (London gave 08:00Z, Tokyo 00:00Z). Non recurring Z events are fine. Rewriting to `DTSTART;TZID=UTC:20261001T090000` (and the same for DTEND, EXDATE, RECURRENCE-ID, dropping the Z) fixed it for all three default zones, including a two value EXDATE.

Workaround tested (text level, before parsing): split the feed into a header (everything outside VEVENT, including VTIMEZONE) and VEVENT groups keyed by UID (an override shares the UID of its series), then for each UID parse `header + group + END:VCALENDAR` and call `expand_dates(tz, 20000)`. Result matched expectations for two series in one feed.

HTTP, run against a local server that answers 200 with ETag and Last-Modified and 304 on `If-None-Match`:
- ureq 3.4.2: `agent.get(url).call()` gives 200, headers readable, `body_mut().with_config().limit(n).read_to_string()`, second call with `.header("If-None-Match", etag)` returned `Ok` with status 304. Default features are rustls 0.23 + ring + webpki-roots (bundled roots, no system store), `cargo tree -e normal` = 30 distinct crates for ureq alone (42 locked). No tokio, no openssl.
- reqwest blocking: adding `blocking` to `["json"]` against a copy of the app's Cargo.lock changed the lock by one package: `futures-io 0.3.34`. Everything else (hyper, tokio, rustls, aws-lc-rs) is already there. Used from `std::thread::spawn` inside `#[tokio::main]` it worked and returned 304 on revalidation. Calling `reqwest::blocking::Client::new()` on a tokio worker thread panics ("Cannot drop a runtime in a context where blocking is not allowed").

## Implications for the cards

Cargo.toml for `src-tauri/crates/trigger-clock` (verify the cron line against R01):
```toml
calcard = { version = "0.3.14", default-features = false }
chrono = { version = "0.4", default-features = false, features = ["std", "clock"] }
chrono-tz = "0.10"
ureq = "3.4"
```
Check at build time that `calcard::common::timezone::Tz` accepts the `chrono-tz` version calcard uses (0.10.x); keep chrono-tz on 0.10 so `Tz::from(chrono_tz::Tz)` type matches. Local zone for `default_tz`: `iana-time-zone` crate (or `TZ` env) to get a name, parse into `chrono_tz::Tz`, then `Tz::from(...)`; fall back to `Tz::Floating`.

Why ureq over reqwest: the crate stays independent of tokio and of the app's reqwest version, the calls are plain blocking calls on the clock thread, and ureq has no async panic trap. The cost is a second TLS stack (ring) beside the app's aws-lc-rs; if the cards prefer one stack, use `reqwest = { version = "0.13", default-features = true, features = ["blocking"] }` and only call it from the clock's own thread, never from a tokio task.

Fetcher trait: `fetch(url, etag: Option<&str>, last_modified: Option<&str>) -> Result<Fetched, Error>` where `Fetched` is `NotModified` or `Body { text, etag, last_modified }`. ureq: send `If-None-Match` and `If-Modified-Since`, treat status 304 as NotModified, cap body at 10 MiB, global timeout 15 s. Map `webcal://` to `https://` before fetching. A path in `feed` is read with `std::fs` and needs no fetcher.

Expansion procedure for "occurrences between now and now+24h":
1. Normalise text: unfold lines, for lines starting `DTSTART`, `DTEND`, `EXDATE`, `RECURRENCE-ID` whose values end in `Z` and that have no `TZID`, replace with `;TZID=UTC` and strip the `Z` from each value. Apply only inside events that have an RRULE (or to all, it is harmless).
2. Split by UID as in Evidence, parse each with `calcard::icalendar::ICalendar::parse(&text)` (returns `Result`), call `expand_dates(local_tz, 20000)`; log `errors`.
3. For each event compute end: `Time(t)` is the end, `Delta(d)` is `start + d`. Then keep events with `start` or `end` inside the window of interest. Sort by start.
4. Read the title from `ical.components[e.comp_id as usize].property(&ICalendarProperty::Summary)`, value `ICalendarValue::Text`. Note that an override component has its own SUMMARY.
5. Poll the feed no more than every 5 minutes, keep the parsed event list cached between polls, and ignore expansion when the body is a 304.

Fixture files for slice B should include: the weekly London event, the EXDATE daily, the RECURRENCE-ID move, all day, a UTC recurring event with Z (the trap), floating, and an Outlook `GMT Standard Time` TZID. The handwritten ones above are in the style of Google, Outlook and Apple but are not real exports.

## Doubts

- No real Google, Outlook or Apple exports were available; fixtures are handwritten. The epic Risk calls for real exports.
- Apple style `X-WR-TIMEZONE` and `TZID` names like `/mozilla.org/...` not tested.
- Per UID limit of 20000 still fails for hourly or finer series starting years ago; they would show no occurrences. Not measured how common.
- `THISANDFUTURE` ranges are handled in calcard source (`rid_this_and_future`) but not run.
- calcard is young (64k downloads, one maintainer team, Stalwart Labs); the UTC bug may be fixed upstream later, then step 1 of the procedure can go. Not checked if an issue exists.
- ureq has bundled web roots only, so a feed behind a corporate TLS inspection proxy would fail; ureq's `platform-verifier` feature would fix that, untested.
- `iana-time-zone` use for the local zone was not run.
