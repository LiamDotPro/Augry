---
id: T10
epic: 011-trigger-github
title: Parse the output of gh api -i into GhResponse
status: todo
depends_on: [T08]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-github/src/logic/response.rs
  - src-tauri/crates/trigger-github/src/logic.rs
---

## Goal

The crate has `logic/response.rs`: the `GhResponse` enum from the epic and `parse_gh_output(stdout, exit)`, a pure function that reads the status line, headers and body that `gh api -i` prints, including a 304 that `gh` reports with exit code 1 and a run that printed no status line at all. Nothing calls it yet.

## Read first

- `plans/011-trigger-github/epic.md` Decisions 8 and 9 (the `GhResponse` shape, why stdout and not stderr is parsed)
- `plans/011-trigger-github/research/R02-rate-limits-and-gh.md` Evidence, the 304, 404, bad token, not logged in and network down results (what `gh` prints and which exit code)
- `src-tauri/crates/trigger-github/src/logic.rs` the file that gets a new `pub mod` line

## Steps

1. Create `src-tauri/crates/trigger-github/src/logic/response.rs` with exactly this.

Header names are stored in lower case, so later code looks them up without caring about case (GitHub sends `Etag`, `X-Ratelimit-Remaining`). `NOT_LOGGED_IN` is the message for exit code 4; the backoff card (T11) and slice C compare against it. A 304 is `NotModified` whatever the exit code, because `gh` exits 1 for it. A 2xx is `Ok` and carries the `etag` header verbatim, including the `W/` prefix and the quotes. Any other status is `Http`. Output with no status line is `Transport`, and its message never contains stdout (so nothing the user has is ever echoed).

```rust
//! What `gh api -i` printed, as a value. Pure.
//!
//! The output is a status line, headers, a blank line and the body. It is read
//! from stdout only: `gh` exits 1 for every HTTP error and for 304, and its
//! stderr text is not a contract. The exit code only matters when there is no
//! status line at all.

/// `gh` exits 4 when there is no login. Slice C turns this message into
/// "unavailable, run gh auth login".
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
```

2. Edit `logic.rs` so it declares the module. The new line goes just above `use trigger_core::RuleId;`. Use exactly this edit in `src-tauri/crates/trigger-github/src/logic.rs`.

Find this text, which appears once:

```rust
use trigger_core::RuleId;
```

Replace it with:

```rust
pub mod response;

use trigger_core::RuleId;
```

3. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib` passes with 20 tests (9 new in this card)
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (the `source`, `readme` and `live` tests included)
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (warnings that `lib.rs` and `os.rs` still have template code are expected until slice C)
- [ ] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- Running `gh`, or any child process (slice C).
- Rate limit decisions: `Backoff` is the next card.
- Adding re-exports of the new items to `logic.rs`. Other code names them by path, `logic::response::parse_gh_output`.
- Removing `#![allow(dead_code)]` from `logic.rs`.

## If stuck

- If a test in `response.rs` fails, compare the file with step 1 character by character. Do not change a test.
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
