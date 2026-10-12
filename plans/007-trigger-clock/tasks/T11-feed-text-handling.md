---
id: T11
epic: 007-trigger-clock
title: Add feed.rs with the Fetcher trait, normalise and split_by_uid
status: todo
depends_on: [T10]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-clock/src/lib.rs
  - src-tauri/crates/trigger-clock/src/feed.rs
  - src-tauri/crates/trigger-clock/src/feed/tests.rs
---

## Goal

`feed.rs` exists with the `Fetched` type and `Fetcher` trait (Decision 9), `feed_url` (a feed in a rule becomes a URL, or `None` for a file path), `normalise` (unfold lines, and the `Z` to `TZID=UTC` workaround for calcard's bug), and `split_by_uid` (one small calendar per UID). All pure and tested with inline text. Parsing and expanding come in the next card.

## Read first

- `plans/007-trigger-clock/epic.md` Decision 9 (the feed design and the `Fetcher` signature)
- `plans/007-trigger-clock/research/R02-ics-and-http.md` "Trap" 2 in Evidence and "Expansion procedure" steps 1 and 2
- `src-tauri/crates/trigger-clock/src/lib.rs:1-20` the `mod` lines you add one to
- `src-tauri/crates/trigger-fs/src/logic.rs:360-375` how a test module in a separate file is declared

## Steps

1. In `src-tauri/crates/trigger-clock/src/lib.rs`, find the three lines

```rust
mod logic;
mod os;
mod rule;
```

and replace them with these four, in this order:

```rust
mod feed;
mod logic;
mod os;
mod rule;
```

2. Create `src-tauri/crates/trigger-clock/src/feed.rs` with exactly this content. The first five lines are a module comment; card T12 adds imports directly below them.

```rust
//! Calendar feeds: turns the text of an `.ics` file into the occurrences of
//! its events inside a window of time, and defines the `Fetcher` the feed
//! thread uses to get the text. Pure: no network, no files, no clock. The
//! design, and the two calcard traps worked around here, are in
//! `plans/007-trigger-clock/research/R02-ics-and-http.md`.

/// What a fetch returned.
#[derive(Debug, Clone, PartialEq)]
pub enum Fetched {
    /// The server said the copy we hold is still current (HTTP 304).
    NotModified,
    Body {
        text: String,
        etag: Option<String>,
        last_modified: Option<String>,
    },
}

/// Gets a feed over the network. Behind a trait so tests answer from fixture
/// files. `etag` and `last_modified` are the values from the previous fetch.
pub trait Fetcher: Send {
    fn fetch(
        &self,
        url: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> Result<Fetched, String>;
}

/// The URL to fetch for a feed written in a rule, or `None` when the feed is
/// a file path. `webcal://` is HTTPS with another name.
pub fn feed_url(feed: &str) -> Option<String> {
    let feed = feed.trim();
    let lower = feed.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        Some(feed.to_string())
    } else if lower.starts_with("webcal://") {
        Some(format!("https://{}", &feed["webcal://".len()..]))
    } else {
        None
    }
}

const DATE_PROPERTIES: [&str; 4] = ["DTSTART", "DTEND", "EXDATE", "RECURRENCE-ID"];

/// Joins folded lines: a line that starts with a space or tab continues the
/// one before it. Blank lines are dropped.
fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.lines() {
        match lines.last_mut() {
            Some(last) if raw.starts_with([' ', '\t']) => last.push_str(&raw[1..]),
            _ => lines.push(raw.to_string()),
        }
    }
    lines.retain(|l| !l.trim().is_empty());
    lines
}

/// Byte index of the colon that ends the property name and parameters.
/// Colons inside a quoted parameter value do not count.
fn value_colon(line: &str) -> Option<usize> {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ':' if !quoted => return Some(i),
            _ => {}
        }
    }
    None
}

/// `DTSTART:20261001T090000Z` becomes `DTSTART;TZID=UTC:20261001T090000`.
/// calcard reads the time of a recurring event written with a `Z` as local
/// time, which is wrong; the second form it reads correctly. Returns `None`
/// when the line needs no change.
fn utc_as_tzid(line: &str) -> Option<String> {
    let colon = value_colon(line)?;
    let (head, value) = (&line[..colon], &line[colon + 1..]);
    let (name, params) = match head.split_once(';') {
        Some((name, params)) => (name, Some(params)),
        None => (head, None),
    };
    if !DATE_PROPERTIES.iter().any(|p| name.eq_ignore_ascii_case(p)) {
        return None;
    }
    if params.is_some_and(|p| p.to_ascii_uppercase().contains("TZID=")) {
        return None;
    }
    // YYYYMMDDTHHMMSSZ is 16 characters.
    let values: Vec<&str> = value.split(',').map(str::trim).collect();
    if !values.iter().all(|v| v.len() == 16 && v.ends_with('Z')) {
        return None;
    }
    let stripped: Vec<&str> = values.iter().map(|v| &v[..15]).collect();
    let params = params.map(|p| format!(";{p}")).unwrap_or_default();
    Some(format!("{name};TZID=UTC{params}:{}", stripped.join(",")))
}

/// The feed text with lines unfolded, `\r\n` line ends, and every time inside
/// a VEVENT that is written with a `Z` rewritten by `utc_as_tzid`. Lines
/// outside a VEVENT are left as they are.
pub fn normalise(text: &str) -> String {
    let mut out = String::new();
    let mut in_event = false;
    for line in unfold(text) {
        let upper = line.to_ascii_uppercase();
        if upper == "BEGIN:VEVENT" {
            in_event = true;
        } else if upper == "END:VEVENT" {
            in_event = false;
        }
        let line = if in_event {
            utc_as_tzid(&line).unwrap_or(line)
        } else {
            line
        };
        out.push_str(&line);
        out.push_str("\r\n");
    }
    out
}

/// Splits normalised feed text into one small calendar per UID. Each is the
/// feed's header (everything outside VEVENTs, so VTIMEZONE comes along), the
/// events with that UID (an override shares the UID of its series), and
/// `END:VCALENDAR`. Returns `(uid, text)` in order of first appearance. An
/// event with no UID is named `no-uid-<n>`.
pub fn split_by_uid(text: &str) -> Vec<(String, String)> {
    let mut header: Vec<String> = Vec::new();
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    let mut event: Option<Vec<String>> = None;
    for line in unfold(text) {
        let upper = line.to_ascii_uppercase();
        if let Some(lines) = event.as_mut() {
            let done = upper == "END:VEVENT";
            lines.push(line);
            if done {
                let lines = event.take().unwrap_or_default();
                let uid = lines
                    .iter()
                    .find(|l| l.to_ascii_uppercase().starts_with("UID"))
                    .and_then(|l| value_colon(l).map(|i| l[i + 1..].trim().to_string()))
                    .unwrap_or_else(|| format!("no-uid-{}", groups.len()));
                match groups.iter_mut().find(|(u, _)| *u == uid) {
                    Some((_, existing)) => existing.extend(lines),
                    None => groups.push((uid, lines)),
                }
            }
        } else if upper == "BEGIN:VEVENT" {
            event = Some(vec![line]);
        } else if upper != "END:VCALENDAR" {
            header.push(line);
        }
    }
    groups
        .into_iter()
        .map(|(uid, lines)| {
            let mut text = String::new();
            for l in header.iter().chain(lines.iter()) {
                text.push_str(l);
                text.push_str("\r\n");
            }
            text.push_str("END:VCALENDAR\r\n");
            (uid, text)
        })
        .collect()
}

#[cfg(test)]
mod tests;
```

3. Create the folder `src-tauri/crates/trigger-clock/src/feed/` and the file `src-tauri/crates/trigger-clock/src/feed/tests.rs` with exactly this content:

```rust
use super::*;

fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

#[test]
fn feed_url_maps_webcal_to_https_and_leaves_paths_alone() {
    assert_eq!(
        feed_url("https://x.test/a.ics").as_deref(),
        Some("https://x.test/a.ics")
    );
    assert_eq!(
        feed_url("http://x.test/a.ics").as_deref(),
        Some("http://x.test/a.ics")
    );
    assert_eq!(
        feed_url("webcal://x.test/a.ics").as_deref(),
        Some("https://x.test/a.ics")
    );
    assert_eq!(
        feed_url("  WEBCAL://x.test/a.ics ").as_deref(),
        Some("https://x.test/a.ics")
    );
    assert_eq!(feed_url("/home/me/cal.ics"), None);
    assert_eq!(feed_url("C:\\Users\\me\\cal.ics"), None);
}

#[test]
fn normalise_rewrites_utc_times_inside_events() {
    let text = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nDTSTART:20261001T090000Z\nDTEND:20261001T093000Z\nEXDATE:20261014T090000Z,20261015T090000Z\nRECURRENCE-ID:20261016T090000Z\nEND:VEVENT\nEND:VCALENDAR\n";
    assert_eq!(
        lines(&normalise(text)),
        vec![
            "BEGIN:VCALENDAR",
            "BEGIN:VEVENT",
            "DTSTART;TZID=UTC:20261001T090000",
            "DTEND;TZID=UTC:20261001T093000",
            "EXDATE;TZID=UTC:20261014T090000,20261015T090000",
            "RECURRENCE-ID;TZID=UTC:20261016T090000",
            "END:VEVENT",
            "END:VCALENDAR",
        ]
    );
}

#[test]
fn normalise_keeps_other_parameters_and_adds_the_zone_first() {
    let text = "BEGIN:VEVENT\nDTSTART;VALUE=DATE-TIME:20261001T090000Z\nEND:VEVENT\n";
    assert_eq!(
        lines(&normalise(text))[1],
        "DTSTART;TZID=UTC;VALUE=DATE-TIME:20261001T090000"
    );
}

#[test]
fn normalise_leaves_every_other_line_alone() {
    let text = "BEGIN:VCALENDAR\nDTSTART:20261001T090000Z\nBEGIN:VEVENT\nDTSTART;TZID=Europe/London:20261001T090000\nDTSTART:20261001T090000\nDTSTART;VALUE=DATE:20261001\nDTSTAMP:20260101T000000Z\nSUMMARY:Call at 9:00Z\nEND:VEVENT\nEND:VCALENDAR\n";
    // Same lines. The first DTSTART is outside any event.
    assert_eq!(lines(&normalise(text)), lines(text));
}

#[test]
fn normalise_unfolds_long_lines() {
    let text = "BEGIN:VEVENT\nSUMMARY:A very long\n  title\n  indeed\nEND:VEVENT\n";
    assert_eq!(
        lines(&normalise(text))[1],
        "SUMMARY:A very long title indeed"
    );
}

#[test]
fn split_by_uid_groups_an_override_with_its_series() {
    let text = normalise(
        "BEGIN:VCALENDAR\nVERSION:2.0\nBEGIN:VTIMEZONE\nTZID:Europe/London\nEND:VTIMEZONE\n\
         BEGIN:VEVENT\nUID:a\nSUMMARY:One\nEND:VEVENT\n\
         BEGIN:VEVENT\nUID:b\nSUMMARY:Two\nEND:VEVENT\n\
         BEGIN:VEVENT\nUID:a\nRECURRENCE-ID:20261015T140000\nSUMMARY:One moved\nEND:VEVENT\n\
         END:VCALENDAR\n",
    );
    let parts = split_by_uid(&text);
    let uids: Vec<&str> = parts.iter().map(|(u, _)| u.as_str()).collect();
    assert_eq!(uids, vec!["a", "b"]);
    assert_eq!(
        lines(&parts[0].1),
        vec![
            "BEGIN:VCALENDAR",
            "VERSION:2.0",
            "BEGIN:VTIMEZONE",
            "TZID:Europe/London",
            "END:VTIMEZONE",
            "BEGIN:VEVENT",
            "UID:a",
            "SUMMARY:One",
            "END:VEVENT",
            "BEGIN:VEVENT",
            "UID:a",
            "RECURRENCE-ID:20261015T140000",
            "SUMMARY:One moved",
            "END:VEVENT",
            "END:VCALENDAR",
        ]
    );
    assert!(parts[1].1.contains("SUMMARY:Two"));
    assert!(!parts[1].1.contains("SUMMARY:One"));
    assert!(parts[1].1.contains("TZID:Europe/London"));
}

#[test]
fn an_event_with_no_uid_gets_a_name_of_its_own() {
    let parts = split_by_uid("BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:x\nEND:VEVENT\nBEGIN:VEVENT\nSUMMARY:y\nEND:VEVENT\nEND:VCALENDAR\n");
    let uids: Vec<&str> = parts.iter().map(|(u, _)| u.as_str()).collect();
    assert_eq!(uids, vec!["no-uid-0", "no-uid-1"]);
}

#[test]
fn text_with_no_events_splits_into_nothing() {
    assert!(split_by_uid("").is_empty());
    assert!(split_by_uid("BEGIN:VCALENDAR\nEND:VCALENDAR\n").is_empty());
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-clock`, then the acceptance commands. `Fetched`, `Fetcher` and the functions are not called by anything yet; the crate-wide `#![allow(dead_code)]` in `lib.rs` keeps that quiet.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock feed::` passes: 8 tests
- [ ] `cd src-tauri && cargo test -p trigger-clock` passes (all earlier tests too; `live` shows 1 ignored)
- [ ] `wc -l src-tauri/crates/trigger-clock/src/feed.rs` prints a number under 400
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (the `still has template code` warning for `src/lib.rs` is expected until slice C)
- [ ] `git status --porcelain` lists only paths under `files:` (ignore other card files under `plans/007-trigger-clock/tasks/`)

## Out of scope

- Parsing with `calcard`, expanding, titles, `expand`: card T12.
- Any use of `Fetcher` or the network: cards T14 and later.
- Making `normalise` handle `Z` times outside VEVENTs, `X-WR-TIMEZONE`, or `VALUE=DATE` values.
- Extra tests, renames, a `log` dependency.

## If stuck

- If pasted code does not compile or clippy objects, check that you pasted it exactly and in the place the step names. If it still fails, set `status: blocked` and quote the compiler error. Do not change the code.
- If `cargo fmt -p trigger-clock --check` fails, run `cd src-tauri && cargo fmt -p trigger-clock` once and carry on.
- If clippy says a `use` or item is unused, the crate-wide `#![allow(dead_code)]` in `lib.rs` is missing (T02 puts it there). Set `status: blocked` and say so.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
