//! Helpers for the tests of `git.rs`. Tests that need git skip themselves with
//! a message when it is not installed, so `cargo test` passes on any machine.

use super::{run, TIMEOUT};

/// True when `git --version` runs. A test that needs git starts with
/// `if !git_available() { return; }`; the message goes to stderr.
pub fn git_available() -> bool {
    let ok = run(&std::env::temp_dir(), &["--version"], TIMEOUT)
        .map(|o| o.ok)
        .unwrap_or(false);
    if !ok {
        eprintln!("skipped: git is not installed");
    }
    ok
}
