---
id: T14
epic: 007-trigger-clock
title: Add UreqFetcher to os.rs with a local server test
status: done
depends_on: [T11]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-clock/src/os.rs
  - src-tauri/crates/trigger-clock/src/os/tests.rs
---

## Goal

`os.rs` has `UreqFetcher`, an implementation of `Fetcher` over `ureq` that sends `If-None-Match` and `If-Modified-Since`, returns `Fetched::NotModified` for a 304, times out after 15 seconds and refuses a body over 10 MiB. Its tests talk to a throwaway server on `127.0.0.1`, never to the internet.

## Read first

- `plans/007-trigger-clock/epic.md` Decision 9 (timeout, size cap, headers)
- `plans/007-trigger-clock/research/R02-ics-and-http.md` the HTTP part of Evidence and "Fetcher trait"
- `src-tauri/crates/trigger-clock/src/feed.rs:1-40` `Fetched` and `Fetcher`, the contract this card implements
- `src-tauri/crates/trigger-clock/src/os.rs` the file you replace (it has `availability()` only)

## Steps

1. Replace the whole of `src-tauri/crates/trigger-clock/src/os.rs` with this. `availability()` is unchanged. `webcal://` is mapped to `https://` by `feed::feed_url` before a URL reaches the fetcher, so the fetcher does not handle it. Errors are short fixed sentences on purpose: a feed URL often holds a secret token, so no error text may contain it.

```rust
//! The only file that talks to the operating system: reading a feed over HTTP
//! or from a file, and asking for the machine's time zone. The clock loop
//! itself needs no OS calls (it reads the wall clock through `trigger_core`'s
//! `Clock`). Everything decided about feeds is pure and lives in `feed.rs`.

use std::time::Duration;

use trigger_core::Availability;
use ureq::http::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};

use crate::feed::{Fetched, Fetcher};

/// A feed larger than this is refused (10 MiB).
pub const MAX_FEED_BYTES: u64 = 10 * 1024 * 1024;

/// How long one request may take, from connecting to the last byte.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

pub fn availability() -> Availability {
    Availability::Available
}

/// Fetches a feed with `ureq`, a blocking client with no async runtime. Call
/// it from the feed thread only.
pub struct UreqFetcher {
    agent: ureq::Agent,
    max_bytes: u64,
}

impl UreqFetcher {
    pub fn new() -> UreqFetcher {
        UreqFetcher::with_limit(MAX_FEED_BYTES)
    }

    /// A smaller cap, for tests.
    pub fn with_limit(max_bytes: u64) -> UreqFetcher {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(FETCH_TIMEOUT))
            // Statuses are handled below, so a 304 is not an error.
            .http_status_as_error(false)
            .build();
        UreqFetcher {
            agent: ureq::Agent::new_with_config(config),
            max_bytes,
        }
    }
}

impl Default for UreqFetcher {
    fn default() -> Self {
        UreqFetcher::new()
    }
}

/// A short reason for a failed request. It never includes the URL or the
/// library's own text, because a feed URL often carries a secret token.
fn reason(err: &ureq::Error) -> &'static str {
    match err {
        ureq::Error::Timeout(_) => "the request timed out",
        ureq::Error::HostNotFound => "the host was not found",
        ureq::Error::ConnectionFailed => "the connection failed",
        ureq::Error::BodyExceedsLimit(_) => "the feed is too large",
        ureq::Error::BadUri(_) => "the feed address is not valid",
        _ => "the request failed",
    }
}

impl Fetcher for UreqFetcher {
    fn fetch(
        &self,
        url: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> Result<Fetched, String> {
        let mut request = self.agent.get(url);
        if let Some(etag) = etag {
            request = request.header(IF_NONE_MATCH, etag);
        }
        if let Some(last_modified) = last_modified {
            request = request.header(IF_MODIFIED_SINCE, last_modified);
        }
        let mut response = request.call().map_err(|e| reason(&e).to_string())?;
        let status = response.status().as_u16();
        if status == 304 {
            return Ok(Fetched::NotModified);
        }
        if status != 200 {
            return Err(format!("the server answered {status}"));
        }
        let header = |name| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let etag = header(ETAG);
        let last_modified = header(LAST_MODIFIED);
        let text = response
            .body_mut()
            .with_config()
            .limit(self.max_bytes)
            .read_to_string()
            .map_err(|e| reason(&e).to_string())?;
        Ok(Fetched::Body {
            text,
            etag,
            last_modified,
        })
    }
}

#[cfg(test)]
mod tests;
```

2. Create the folder `src-tauri/crates/trigger-clock/src/os/` and the file `src-tauri/crates/trigger-clock/src/os/tests.rs` with exactly this content. Card T15 appends to it.

```rust
use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

/// Serves the given responses, one per connection, on a local port. Sends
/// each request's text (lower-cased) down the channel.
fn serve(responses: Vec<String>) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/cal.ics", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = stream.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
            }
            tx.send(String::from_utf8_lossy(&request).to_lowercase())
                .unwrap();
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (url, rx)
}

fn ok(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nLast-Modified: Tue, 13 Oct 2026 08:00:00 GMT\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

const NOT_MODIFIED: &str = "HTTP/1.1 304 Not Modified\r\nConnection: close\r\n\r\n";

#[test]
fn a_fetch_returns_the_body_and_its_validators() {
    let (url, _requests) = serve(vec![ok("BEGIN:VCALENDAR")]);
    let got = UreqFetcher::new().fetch(&url, None, None).unwrap();
    assert_eq!(
        got,
        Fetched::Body {
            text: "BEGIN:VCALENDAR".into(),
            etag: Some("\"v1\"".into()),
            last_modified: Some("Tue, 13 Oct 2026 08:00:00 GMT".into()),
        }
    );
}

#[test]
fn a_second_fetch_sends_the_validators_and_a_304_is_not_modified() {
    let (url, requests) = serve(vec![ok("x"), NOT_MODIFIED.to_string()]);
    let fetcher = UreqFetcher::new();
    fetcher.fetch(&url, None, None).unwrap();
    let first = requests.recv().unwrap();
    assert!(!first.contains("if-none-match"));
    let again = fetcher
        .fetch(&url, Some("\"v1\""), Some("Tue, 13 Oct 2026 08:00:00 GMT"))
        .unwrap();
    assert_eq!(again, Fetched::NotModified);
    let second = requests.recv().unwrap();
    assert!(second.contains("if-none-match: \"v1\""));
    assert!(second.contains("if-modified-since: tue, 13 oct 2026 08:00:00 gmt"));
}

#[test]
fn other_statuses_are_errors_that_do_not_name_the_url() {
    let (url, _requests) = serve(vec![
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
    ]);
    let err = UreqFetcher::new().fetch(&url, None, None).unwrap_err();
    assert_eq!(err, "the server answered 404");
    assert!(!err.contains("127.0.0.1"));
}

#[test]
fn a_feed_over_the_cap_is_refused() {
    let (url, _requests) = serve(vec![ok("0123456789ABCDEF")]);
    let err = UreqFetcher::with_limit(10)
        .fetch(&url, None, None)
        .unwrap_err();
    assert_eq!(err, "the feed is too large");
}

#[test]
fn a_connection_that_is_refused_is_an_error() {
    // Bind then drop, so the port is closed.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let url = format!("http://127.0.0.1:{port}/cal.ics");
    assert!(UreqFetcher::new().fetch(&url, None, None).is_err());
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-clock`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock os::` passes: 5 tests
- [ ] `cd src-tauri && cargo test -p trigger-clock` passes (all earlier tests too; `live` shows 1 ignored)
- [ ] `grep -c "example.com\|https://" src-tauri/crates/trigger-clock/src/os/tests.rs` prints `0` (no test may reach the internet)
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (the `still has template code` warning for `src/lib.rs` is expected until slice C)
- [ ] `git status --porcelain` lists only paths under `files:` (ignore other card files under `plans/007-trigger-clock/tasks/`)

## Out of scope

- Reading files and the local time zone: card T15.
- Retrying, polling every 5 minutes, remembering the validators: slice C, the feed thread.
- A `reqwest` fetcher, proxies, certificate settings, `platform-verifier`.
- Fetching from a real URL in any test.

## If stuck

- If pasted code does not compile or clippy objects, check that you pasted it exactly and in the place the step names. If it still fails, set `status: blocked` and quote the compiler error. Do not change the code.
- If the compiler says `ureq::http::header` or `config_builder` does not exist, the pinned `ureq` is not 3.4. Run `cd src-tauri && cargo tree -p trigger-clock -i ureq`, write the version in the Log, set `status: blocked` and stop.
- If a test fails with "the request failed" while the others pass, a proxy variable (`HTTP_PROXY`, `http_proxy`) may be set on this machine. Run the tests with `env -u HTTP_PROXY -u http_proxy -u HTTPS_PROXY -u https_proxy cargo test -p trigger-clock os::` and write that in the Log; if they pass, carry on.
- If `cargo fmt -p trigger-clock --check` fails, run `cd src-tauri && cargo fmt -p trigger-clock` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
