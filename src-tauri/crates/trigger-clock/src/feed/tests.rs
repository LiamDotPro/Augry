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
