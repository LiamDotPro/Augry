//! Calendar feeds: turns the text of an `.ics` file into the occurrences of
//! its events inside a window of time, and defines the `Fetcher` the feed
//! thread uses to get the text. Pure: no network, no files, no clock. The
//! design, and the two calcard traps worked around here, are in
//! `plans/007-trigger-clock/research/R02-ics-and-http.md`.

use std::time::SystemTime;

use calcard::common::timezone::Tz as CalTz;
use calcard::icalendar::dates::TimeOrDelta;
use calcard::icalendar::{ICalendar, ICalendarProperty, ICalendarValue};

use crate::logic::Occurrence;

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
