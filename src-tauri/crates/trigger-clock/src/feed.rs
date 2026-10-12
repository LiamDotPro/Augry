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
