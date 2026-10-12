//! Tests against the real OS. Not run by `cargo test`; run them on a machine
//! of each supported platform with:
//!
//!     cargo test -p trigger-github -- --ignored
//!
//! Each test arms a rule, causes the real thing to happen, and waits for the
//! event with `RecordingSink::wait_for`. Keep them few and generous with time.

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
                "github.example",
                serde_json::json!({ "example": "x" }),
            )],
            sink.tx.clone(),
        )
        .expect("start");
    // Cause the real thing here.
    assert!(sink
        .wait_for("github.example", Duration::from_secs(5))
        .is_some());
}
