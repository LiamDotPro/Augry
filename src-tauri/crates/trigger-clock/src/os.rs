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
