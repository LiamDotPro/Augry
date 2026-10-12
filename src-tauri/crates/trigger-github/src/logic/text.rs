//! Small text helpers for request paths and payloads. Pure.

/// Percent-encodes everything except letters, digits and `-._~`, so a branch
/// like `feature/x` or a time like `12:30` is safe inside a query string.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The first `max` characters (not bytes), so a cut never splits a letter.
pub fn cut(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// The number at the end of a URL path, such as 13923 for
/// `https://api.github.com/repos/cli/cli/issues/13923`.
pub fn last_number(url: &str) -> Option<u64> {
    url.split(['?', '#'])
        .next()?
        .rsplit('/')
        .next()?
        .parse()
        .ok()
}

/// Logins are not case sensitive on GitHub.
pub fn same_login(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Seconds since the Unix epoch as `2026-10-12T20:24:14Z`, the form GitHub
/// uses, so two of them compare correctly as plain text.
pub fn iso(epoch_s: u64) -> String {
    let days = (epoch_s / 86_400) as i64;
    let rest = epoch_s % 86_400;
    // Civil date from a day count (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_keeps_safe_characters_and_escapes_the_rest() {
        assert_eq!(encode("trunk"), "trunk");
        assert_eq!(encode("feature/x y"), "feature%2Fx%20y");
        assert_eq!(encode("dependabot[bot]"), "dependabot%5Bbot%5D");
        assert_eq!(encode("2026-10-12T20:24:14Z"), "2026-10-12T20%3A24%3A14Z");
        assert_eq!(encode("é"), "%C3%A9");
    }

    #[test]
    fn cut_counts_characters() {
        assert_eq!(cut("hello", 10), "hello");
        assert_eq!(cut("hello", 3), "hel");
        assert_eq!(cut("ééé", 2), "éé");
        assert_eq!(cut("", 5), "");
    }

    #[test]
    fn last_number_reads_the_end_of_a_url() {
        let issue = "https://api.github.com/repos/cli/cli/issues/13923";
        assert_eq!(last_number(issue), Some(13923));
        assert_eq!(last_number("https://x/pulls/14568?a=1"), Some(14568));
        assert_eq!(last_number("https://x/pulls/abc"), None);
        assert_eq!(last_number(""), None);
    }

    #[test]
    fn logins_ignore_case() {
        assert!(same_login("LiamDotPro", "liamdotpro"));
        assert!(!same_login("a", "b"));
    }

    #[test]
    fn iso_matches_known_times() {
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso(1_760_300_654), "2025-10-12T20:24:14Z");
        assert_eq!(iso(4_102_444_799), "2099-12-31T23:59:59Z");
    }

    #[test]
    fn iso_times_compare_as_text() {
        assert!(iso(1_000_000_000) < iso(1_000_000_001));
        assert!(iso(999_999_999) < iso(1_000_000_000));
    }
}
