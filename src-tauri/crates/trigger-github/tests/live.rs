//! Tests against the real GitHub, through the user's own `gh` login. Not run by
//! `cargo test`; run them on a machine where `gh auth login` was done with:
//!
//!     cargo test -p trigger-github -- --ignored --nocapture
//!
//! They only read, and only a public repository (`cli/cli`). When `gh` is not
//! installed or not signed in they print why and pass without testing
//! anything, so a CI machine without a login stays green. The first poll of a
//! rule only records what exists and fires nothing, so there is no way to make
//! GitHub fire an event on demand; these tests check that arming rules against
//! the real API starts and stays quiet.

use std::time::Duration;

use serde_json::json;
use trigger_core::testkit::{rule, RecordingSink};
use trigger_core::{Availability, Rule, Source, SourceError};
use trigger_github::{GithubSource, KINDS};

/// One rule per kind, so one first poll makes all six requests.
fn every_kind_on_cli() -> Vec<Rule> {
    KINDS
        .iter()
        .map(|k| {
            rule(
                &format!("live/{}", k.id),
                k.id,
                json!({ "repo": "cli/cli", "interval_s": 60 }),
            )
        })
        .collect()
}

#[test]
#[ignore]
fn availability_says_whether_gh_is_installed() {
    match GithubSource.availability() {
        Availability::Available => println!("gh found"),
        Availability::Unsupported { reason } => {
            assert!(reason.contains("https://cli.github.com"), "{reason}");
            println!("skipped: {reason}");
        }
        other => panic!("unexpected availability {other:?}"),
    }
}

#[test]
#[ignore]
fn the_first_poll_of_the_real_api_seeds_and_fires_nothing() {
    let sink = RecordingSink::new();
    let handle = match GithubSource.start(every_kind_on_cli(), sink.tx.clone()) {
        Ok(handle) => handle,
        Err(SourceError::Unavailable(why)) => {
            println!("skipped: {why}");
            return;
        }
        Err(SourceError::Os(why)) => panic!("start failed: {why}"),
    };
    // Six requests, one after another. Ten seconds is generous; the next poll
    // is a minute away, so anything that arrives now came from the first one.
    assert!(sink
        .wait_for("github.release-published", Duration::from_secs(10))
        .is_none());
    assert!(sink.drain().is_empty(), "a first poll must not fire");
    drop(handle);
}
