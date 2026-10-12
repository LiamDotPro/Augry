//! What `gh api -i` printed, as a value. Pure.
//!
//! The output is a status line, headers, a blank line and the body. It is read
//! from stdout only: `gh` exits 1 for every HTTP error and for 304, and its
//! stderr text is not a contract. The exit code only matters when there is no
//! status line at all.

/// `gh` exits 4 when there is no login. `Backoff` turns this message into
/// `Verdict::Unauthorized`, which stops the poller.
pub const NOT_LOGGED_IN: &str = "gh is not logged in";

#[derive(Debug, Clone, PartialEq)]
pub enum GhResponse {
    /// A 2xx answer.
    Ok {
        status: u16,
        etag: Option<String>,
        headers: Vec<(String, String)>,
        body: String,
    },
    /// 304: nothing changed since the etag we sent.
    NotModified { headers: Vec<(String, String)> },
    /// Any other HTTP status.
    Http {
        status: u16,
        headers: Vec<(String, String)>,
        body: String,
    },
    /// No HTTP status line: `gh` did not run, had no network, or has no login.
    Transport(String),
}

impl GhResponse {
    /// All headers, names in lower case. Empty for `Transport`.
    pub fn headers(&self) -> &[(String, String)] {
        match self {
            GhResponse::Ok { headers, .. }
            | GhResponse::NotModified { headers }
            | GhResponse::Http { headers, .. } => headers,
            GhResponse::Transport(_) => &[],
        }
    }

    /// One header by name, ignoring case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers()
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// `X-Poll-Interval` in seconds, when GitHub sent it.
    pub fn poll_interval_s(&self) -> Option<u64> {
        self.header("x-poll-interval")?.trim().parse().ok()
    }
}

/// Reads the stdout of `gh api -i` and the exit code (`None` when the process
/// was killed). A 304 is `NotModified` whatever the exit code was.
pub fn parse_gh_output(stdout: &[u8], exit: Option<i32>) -> GhResponse {
    let text = String::from_utf8_lossy(stdout);
    let mut lines = text.split('\n');
    let status = lines
        .next()
        .map(|l| l.trim_end_matches('\r'))
        .and_then(status_of);
    let Some(status) = status else {
        return GhResponse::Transport(match exit {
            Some(4) => NOT_LOGGED_IN.to_string(),
            Some(code) => format!("gh exited with code {code} and gave no HTTP status"),
            None => "gh was stopped before it answered".to_string(),
        });
    };
    let mut headers = Vec::new();
    for line in lines.by_ref() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    let body = lines.collect::<Vec<_>>().join("\n");
    if status == 304 {
        return GhResponse::NotModified { headers };
    }
    if (200..300).contains(&status) {
        let etag = headers
            .iter()
            .find(|(n, _)| n == "etag")
            .map(|(_, v)| v.clone());
        return GhResponse::Ok {
            status,
            etag,
            headers,
            body,
        };
    }
    GhResponse::Http {
        status,
        headers,
        body,
    }
}

/// `HTTP/2.0 304 Not Modified` gives 304. Anything that is not a status line
/// gives None.
fn status_of(line: &str) -> Option<u16> {
    let mut parts = line.split_whitespace();
    if !parts.next()?.starts_with("HTTP/") {
        return None;
    }
    parts.next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = "HTTP/2.0 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nEtag: W/\"093a24\"\r\nX-Ratelimit-Remaining: 4995\r\nX-Poll-Interval: 60\r\n\r\n[{\"id\":1}]\n";

    #[test]
    fn a_200_gives_the_etag_headers_and_body() {
        let got = parse_gh_output(OK.as_bytes(), Some(0));
        let GhResponse::Ok {
            status,
            etag,
            headers,
            body,
        } = &got
        else {
            panic!("expected Ok, got {got:?}");
        };
        assert_eq!(*status, 200);
        assert_eq!(etag.as_deref(), Some("W/\"093a24\""));
        assert_eq!(headers.len(), 4);
        assert_eq!(body, "[{\"id\":1}]\n");
        assert_eq!(got.header("X-RATELIMIT-REMAINING"), Some("4995"));
        assert_eq!(got.poll_interval_s(), Some(60));
    }

    #[test]
    fn header_names_are_lower_cased_and_line_feeds_alone_work() {
        let out = "HTTP/2.0 200 OK\nEtag: \"abc\"\n\n{}";
        let got = parse_gh_output(out.as_bytes(), Some(0));
        assert_eq!(
            got.headers(),
            &[("etag".to_string(), "\"abc\"".to_string())]
        );
        match got {
            GhResponse::Ok { etag, body, .. } => {
                assert_eq!(etag.as_deref(), Some("\"abc\""));
                assert_eq!(body, "{}");
            }
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    #[test]
    fn a_304_with_exit_1_is_not_modified() {
        let out = "HTTP/2.0 304 Not Modified\r\nEtag: W/\"093a24\"\r\nX-Ratelimit-Remaining: 4995\r\n\r\n";
        let got = parse_gh_output(out.as_bytes(), Some(1));
        assert!(matches!(got, GhResponse::NotModified { .. }));
        assert_eq!(got.header("etag"), Some("W/\"093a24\""));
    }

    #[test]
    fn a_404_keeps_the_body_and_headers() {
        let out = "HTTP/2.0 404 Not Found\r\nX-Ratelimit-Remaining: 4990\r\n\r\n{\"message\":\"Not Found\",\"status\":\"404\"}";
        match parse_gh_output(out.as_bytes(), Some(1)) {
            GhResponse::Http {
                status,
                headers,
                body,
            } => {
                assert_eq!(status, 404);
                assert_eq!(headers.len(), 1);
                assert!(body.contains("Not Found"));
            }
            other => panic!("expected Http, got {other:?}"),
        }
    }

    #[test]
    fn a_403_rate_limit_is_http_with_its_retry_after() {
        let out = "HTTP/2.0 403 Forbidden\r\nRetry-After: 60\r\n\r\n{\"message\":\"You have exceeded a secondary rate limit\"}";
        let got = parse_gh_output(out.as_bytes(), Some(1));
        assert!(matches!(got, GhResponse::Http { status: 403, .. }));
        assert_eq!(got.header("retry-after"), Some("60"));
    }

    #[test]
    fn no_status_line_is_a_transport_error() {
        let got = parse_gh_output(b"", Some(1));
        assert_eq!(
            got,
            GhResponse::Transport("gh exited with code 1 and gave no HTTP status".into())
        );
        assert!(got.headers().is_empty());
        assert_eq!(got.header("etag"), None);
        let garbage = parse_gh_output(b"Get \"https://api.github.com\": dial tcp", Some(1));
        assert!(matches!(garbage, GhResponse::Transport(_)));
    }

    #[test]
    fn exit_4_means_not_logged_in_and_a_killed_process_says_so() {
        assert_eq!(
            parse_gh_output(b"", Some(4)),
            GhResponse::Transport(NOT_LOGGED_IN.into())
        );
        assert_eq!(
            parse_gh_output(b"", None),
            GhResponse::Transport("gh was stopped before it answered".into())
        );
    }

    #[test]
    fn a_status_line_with_no_number_is_transport() {
        let got = parse_gh_output(b"HTTP/2.0 abc\r\n\r\n", Some(1));
        assert!(matches!(got, GhResponse::Transport(_)));
    }

    #[test]
    fn invalid_utf8_in_the_body_does_not_panic() {
        let mut out = b"HTTP/2.0 200 OK\r\n\r\n".to_vec();
        out.extend_from_slice(&[0xff, 0xfe, b'x']);
        assert!(matches!(
            parse_gh_output(&out, Some(0)),
            GhResponse::Ok { .. }
        ));
    }
}
