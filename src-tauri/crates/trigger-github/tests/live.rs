//! Tests against the real GitHub, through the user's own `gh` login. Not run by
//! `cargo test`; run them on a machine where `gh auth login` was done with:
//!
//!     cargo test -p trigger-github -- --ignored
//!
//! Each test arms a rule, waits for the event with `RecordingSink::wait_for`,
//! and reads GitHub only. Keep them few and generous with time.

use std::time::Duration;

use trigger_core::testkit::{rule, RecordingSink};
use trigger_core::Source;

#[test]
#[ignore]
fn fires_for_the_real_thing() {
    let sink = RecordingSink::new();
    let source = trigger_github::GithubSource;
    let _handle = source
        .start(
            vec![rule(
                "p/t",
                "github.issue-assigned",
                serde_json::json!({ "repo": "cli/cli" }),
            )],
            sink.tx.clone(),
        )
        .expect("start");
    assert!(sink
        .wait_for("github.issue-assigned", Duration::from_secs(5))
        .is_some());
}
