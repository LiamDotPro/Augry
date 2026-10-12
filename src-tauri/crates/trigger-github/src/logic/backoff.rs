//! How long to stay away from GitHub after it pushes back. Pure: the caller
//! passes the time in, as whole seconds since the Unix epoch, so every rule
//! here is a unit test with plain numbers.
//!
//! The rules come from plans/011-trigger-github/research/R02. One `Backoff`
//! covers the whole crate: when GitHub says stop, no rule polls.

use super::response::{GhResponse, NOT_LOGGED_IN};

/// First wait after a failure. Each repeat doubles it.
pub const LADDER_START_S: u64 = 60;
/// The ladder stops here (15 minutes).
pub const LADDER_CAP_S: u64 = 900;
/// A wait that GitHub asked for in a header is honoured up to an hour, which
/// is as far away as a rate limit reset can be.
pub const HEADER_WAIT_CAP_S: u64 = 3600;
/// Below this many requests left, every interval is stretched until reset.
pub const LOW_REMAINING: u64 = 100;
pub const STRETCH_FACTOR: u64 = 4;
/// A repository that answers 404 is polled this much less often.
pub const MISSING_FACTOR: u64 = 5;

/// What one answer means for the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// 2xx or 304. The ladder is reset.
    Fine,
    /// 403 or 429 that is a rate limit. Nothing polls for `wait_s`.
    RateLimited { wait_s: u64 },
    /// No answer, or a 5xx. Same ladder, and quiet: do not tell the user.
    Transport { wait_s: u64 },
    /// 404: the repository is missing or not visible. Poll it less often.
    NotFound,
    /// 401, or `gh` has no login. The source is unavailable again.
    Unauthorized,
    /// Any other 4xx, such as a 403 for a repository the token may not see.
    /// Report it; it is not a rate limit, so there is no wait.
    Denied(u16),
}

#[derive(Debug, Default)]
pub struct Backoff {
    /// Failures in a row that count towards the ladder.
    failures: u32,
    /// No polling before this time.
    wait_until: u64,
    /// Intervals are stretched until this time.
    stretch_until: u64,
}

impl Backoff {
    pub fn new() -> Backoff {
        Backoff::default()
    }

    /// False while a wait is running.
    pub fn may_poll(&self, now: u64) -> bool {
        now >= self.wait_until
    }

    /// When the wait ends, if one is running.
    pub fn waiting_until(&self, now: u64) -> Option<u64> {
        (now < self.wait_until).then_some(self.wait_until)
    }

    /// The interval to use for one rule: its own, raised to `X-Poll-Interval`
    /// when GitHub asked for more, times 5 for a missing repository and times
    /// 4 while few requests are left.
    pub fn effective_interval_s(
        &self,
        now: u64,
        base_s: u64,
        hint_s: Option<u64>,
        not_found: bool,
    ) -> u64 {
        let mut s = base_s.max(hint_s.unwrap_or(0));
        if not_found {
            s = s.saturating_mul(MISSING_FACTOR);
        }
        if now < self.stretch_until {
            s = s.saturating_mul(STRETCH_FACTOR);
        }
        s
    }

    /// Takes one answer, updates the waits and says what it meant.
    pub fn record(&mut self, now: u64, response: &GhResponse) -> Verdict {
        self.watch_remaining(now, response);
        match response {
            GhResponse::Ok { .. } | GhResponse::NotModified { .. } => {
                self.failures = 0;
                self.wait_until = 0;
                Verdict::Fine
            }
            GhResponse::Transport(message) if message == NOT_LOGGED_IN => Verdict::Unauthorized,
            GhResponse::Transport(_) => Verdict::Transport {
                wait_s: self.fail(now, None),
            },
            GhResponse::Http { status, body, .. } => {
                if is_rate_limited(*status, response, body) {
                    let asked = asked_wait(now, response);
                    return Verdict::RateLimited {
                        wait_s: self.fail(now, asked),
                    };
                }
                match *status {
                    401 => Verdict::Unauthorized,
                    404 => Verdict::NotFound,
                    500..=599 => Verdict::Transport {
                        wait_s: self.fail(now, None),
                    },
                    other => Verdict::Denied(other),
                }
            }
        }
    }

    /// Starts a wait and returns its length: what GitHub asked for, or the
    /// next step of the ladder (60, 120, 240, 480, then 900 seconds).
    fn fail(&mut self, now: u64, asked: Option<u64>) -> u64 {
        let step = LADDER_START_S
            .saturating_mul(1u64.checked_shl(self.failures).unwrap_or(u64::MAX))
            .min(LADDER_CAP_S);
        self.failures = self.failures.saturating_add(1);
        let wait = asked.unwrap_or(step);
        self.wait_until = now.saturating_add(wait);
        wait
    }

    /// Under 100 requests left stretches intervals until the reset time.
    fn watch_remaining(&mut self, now: u64, response: &GhResponse) {
        let Some(left) = number(response, "x-ratelimit-remaining") else {
            return;
        };
        self.stretch_until = if left < LOW_REMAINING {
            match number(response, "x-ratelimit-reset") {
                Some(reset) if reset > now => reset,
                _ => now.saturating_add(LADDER_START_S),
            }
        } else {
            0
        };
    }
}

fn number(response: &GhResponse, header: &str) -> Option<u64> {
    response.header(header)?.trim().parse().ok()
}

/// 403 or 429, and either `Retry-After`, no requests left, or the words "rate
/// limit" in the body. A plain 403 is a permissions problem, not a limit.
fn is_rate_limited(status: u16, response: &GhResponse, body: &str) -> bool {
    (status == 403 || status == 429)
        && (response.header("retry-after").is_some()
            || number(response, "x-ratelimit-remaining") == Some(0)
            || body.to_ascii_lowercase().contains("rate limit"))
}

/// The wait GitHub asked for: `Retry-After`, else the time until the reset
/// when no requests are left.
fn asked_wait(now: u64, response: &GhResponse) -> Option<u64> {
    if let Some(s) = number(response, "retry-after") {
        return Some(s.clamp(1, HEADER_WAIT_CAP_S));
    }
    if number(response, "x-ratelimit-remaining") == Some(0) {
        let reset = number(response, "x-ratelimit-reset")?;
        return (reset > now).then(|| (reset - now).min(HEADER_WAIT_CAP_S));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect()
    }

    fn http(status: u16, pairs: &[(&str, &str)], body: &str) -> GhResponse {
        GhResponse::Http {
            status,
            headers: headers(pairs),
            body: body.into(),
        }
    }

    fn ok(pairs: &[(&str, &str)]) -> GhResponse {
        GhResponse::Ok {
            status: 200,
            etag: None,
            headers: headers(pairs),
            body: "[]".into(),
        }
    }

    fn limited() -> GhResponse {
        http(
            403,
            &[],
            "{\"message\":\"API rate limit exceeded for user\"}",
        )
    }

    #[test]
    fn the_ladder_is_60_120_240_480_then_15_minutes() {
        let mut b = Backoff::new();
        let mut waits = Vec::new();
        for _ in 0..7 {
            match b.record(1000, &GhResponse::Transport("offline".into())) {
                Verdict::Transport { wait_s } => waits.push(wait_s),
                other => panic!("expected Transport, got {other:?}"),
            }
        }
        assert_eq!(waits, [60, 120, 240, 480, 900, 900, 900]);
    }

    #[test]
    fn a_wait_blocks_polling_until_it_ends() {
        let mut b = Backoff::new();
        assert!(b.may_poll(1000));
        assert_eq!(b.waiting_until(1000), None);
        b.record(1000, &limited());
        assert!(!b.may_poll(1059));
        assert_eq!(b.waiting_until(1059), Some(1060));
        assert!(b.may_poll(1060));
        assert_eq!(b.waiting_until(1060), None);
    }

    #[test]
    fn a_200_or_a_304_resets_the_ladder_and_the_wait() {
        for good in [ok(&[]), GhResponse::NotModified { headers: vec![] }] {
            let mut b = Backoff::new();
            b.record(1000, &limited());
            b.record(1100, &limited());
            assert_eq!(b.record(1300, &good), Verdict::Fine);
            assert!(b.may_poll(1300));
            assert_eq!(
                b.record(2000, &limited()),
                Verdict::RateLimited { wait_s: 60 }
            );
        }
    }

    #[test]
    fn retry_after_sets_the_wait() {
        let mut b = Backoff::new();
        let got = b.record(1000, &http(429, &[("retry-after", "90")], "{}"));
        assert_eq!(got, Verdict::RateLimited { wait_s: 90 });
        assert!(!b.may_poll(1089));
        assert!(b.may_poll(1090));
    }

    #[test]
    fn retry_after_is_capped_at_an_hour_and_at_least_a_second() {
        let mut b = Backoff::new();
        let big = http(403, &[("retry-after", "999999")], "{}");
        assert_eq!(b.record(0, &big), Verdict::RateLimited { wait_s: 3600 });
        let zero = http(403, &[("retry-after", "0")], "{}");
        assert_eq!(b.record(0, &zero), Verdict::RateLimited { wait_s: 1 });
    }

    #[test]
    fn no_requests_left_waits_for_the_reset() {
        let mut b = Backoff::new();
        let r = http(
            403,
            &[
                ("x-ratelimit-remaining", "0"),
                ("x-ratelimit-reset", "1500"),
            ],
            "{}",
        );
        assert_eq!(b.record(1000, &r), Verdict::RateLimited { wait_s: 500 });
    }

    #[test]
    fn a_reset_in_the_past_falls_back_to_the_ladder() {
        let mut b = Backoff::new();
        let r = http(
            403,
            &[("x-ratelimit-remaining", "0"), ("x-ratelimit-reset", "900")],
            "{}",
        );
        assert_eq!(b.record(1000, &r), Verdict::RateLimited { wait_s: 60 });
    }

    #[test]
    fn the_words_rate_limit_in_the_body_count_for_403_and_429_only() {
        let mut b = Backoff::new();
        let secondary = http(403, &[], "You have exceeded a secondary Rate Limit");
        assert_eq!(b.record(0, &secondary), Verdict::RateLimited { wait_s: 60 });
        let mut b = Backoff::new();
        let other = http(422, &[], "rate limit");
        assert_eq!(b.record(0, &other), Verdict::Denied(422));
        assert!(b.may_poll(0));
    }

    #[test]
    fn a_plain_403_is_denied_with_no_wait() {
        let mut b = Backoff::new();
        let got = b.record(
            0,
            &http(403, &[], "{\"message\":\"Resource protected by SSO\"}"),
        );
        assert_eq!(got, Verdict::Denied(403));
        assert!(b.may_poll(0));
    }

    #[test]
    fn missing_unauthorized_and_server_errors() {
        let mut b = Backoff::new();
        assert_eq!(b.record(0, &http(404, &[], "{}")), Verdict::NotFound);
        assert_eq!(b.record(0, &http(401, &[], "{}")), Verdict::Unauthorized);
        assert!(b.may_poll(0));
        assert_eq!(
            b.record(0, &http(502, &[], "")),
            Verdict::Transport { wait_s: 60 }
        );
        assert!(!b.may_poll(59));
    }

    #[test]
    fn no_login_is_unauthorized_not_a_wait() {
        let mut b = Backoff::new();
        let got = b.record(0, &GhResponse::Transport(NOT_LOGGED_IN.into()));
        assert_eq!(got, Verdict::Unauthorized);
        assert!(b.may_poll(0));
    }

    #[test]
    fn few_requests_left_stretch_intervals_until_the_reset() {
        let mut b = Backoff::new();
        let low = ok(&[
            ("x-ratelimit-remaining", "99"),
            ("x-ratelimit-reset", "2000"),
        ]);
        b.record(1000, &low);
        assert_eq!(b.effective_interval_s(1999, 120, None, false), 480);
        assert_eq!(b.effective_interval_s(2000, 120, None, false), 120);
    }

    #[test]
    fn a_low_count_without_a_reset_stretches_for_a_minute() {
        let mut b = Backoff::new();
        b.record(1000, &ok(&[("x-ratelimit-remaining", "5")]));
        assert_eq!(b.effective_interval_s(1059, 60, None, false), 240);
        assert_eq!(b.effective_interval_s(1060, 60, None, false), 60);
    }

    #[test]
    fn enough_requests_left_clears_the_stretch() {
        let mut b = Backoff::new();
        b.record(
            1000,
            &ok(&[
                ("x-ratelimit-remaining", "10"),
                ("x-ratelimit-reset", "5000"),
            ]),
        );
        b.record(1001, &ok(&[("x-ratelimit-remaining", "4000")]));
        assert_eq!(b.effective_interval_s(1002, 120, None, false), 120);
    }

    #[test]
    fn a_304_with_few_requests_left_also_stretches() {
        let mut b = Backoff::new();
        let r = GhResponse::NotModified {
            headers: headers(&[
                ("x-ratelimit-remaining", "50"),
                ("x-ratelimit-reset", "3000"),
            ]),
        };
        assert_eq!(b.record(1000, &r), Verdict::Fine);
        assert_eq!(b.effective_interval_s(1001, 100, None, false), 400);
    }

    #[test]
    fn interval_uses_the_hint_and_the_factors() {
        let mut b = Backoff::new();
        assert_eq!(b.effective_interval_s(0, 120, None, false), 120);
        assert_eq!(b.effective_interval_s(0, 120, Some(60), false), 120);
        assert_eq!(b.effective_interval_s(0, 60, Some(300), false), 300);
        assert_eq!(b.effective_interval_s(0, 120, None, true), 600);
        b.record(
            0,
            &ok(&[("x-ratelimit-remaining", "1"), ("x-ratelimit-reset", "100")]),
        );
        assert_eq!(b.effective_interval_s(0, 120, None, true), 2400);
        assert_eq!(b.effective_interval_s(0, u64::MAX, None, true), u64::MAX);
    }
}
