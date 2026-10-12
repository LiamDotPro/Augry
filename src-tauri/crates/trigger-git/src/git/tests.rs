use std::path::Path;
use std::time::Duration;

use super::testutil::git_available;
use super::*;

fn here() -> std::path::PathBuf {
    std::env::temp_dir()
}

#[test]
fn run_returns_the_output_of_a_command() {
    if !git_available() {
        return;
    }
    let out = run(&here(), &["--version"], TIMEOUT).unwrap();
    assert!(out.ok);
    assert!(out.stdout.starts_with("git version"), "{}", out.stdout);
    assert_eq!(out.stderr, "");
}

#[test]
fn a_non_zero_exit_is_data_not_an_error() {
    if !git_available() {
        return;
    }
    let out = run(&here(), &["no-such-subcommand"], TIMEOUT).unwrap();
    assert!(!out.ok);
    assert!(!out.stderr.is_empty());
}

#[test]
fn value_trims_and_maps_failure_and_empty_to_none() {
    if !git_available() {
        return;
    }
    let version = value(&here(), &["--version"]).unwrap();
    assert!(version.starts_with("git version"));
    assert!(!version.ends_with('\n'));
    assert_eq!(value(&here(), &["no-such-subcommand"]), None);
    // `git config --get` of an unset key exits 1 with no output.
    assert_eq!(
        value(&here(), &["config", "--get", "trigger-git.never-set"]),
        None
    );
}

#[test]
fn a_missing_program_falls_through_to_the_next_one() {
    if !git_available() {
        return;
    }
    let out = run_with(
        &["trigger-git-no-such-program", "git"],
        &here(),
        &["--version"],
        TIMEOUT,
    )
    .unwrap();
    assert!(out.ok);
}

#[test]
fn only_missing_programs_means_not_installed() {
    let err = run_with(
        &["trigger-git-no-such-program"],
        &here(),
        &["--version"],
        TIMEOUT,
    )
    .unwrap_err();
    assert_eq!(err, GitError::NotInstalled);
}

#[test]
fn an_unusable_working_directory_is_an_io_error() {
    if !git_available() {
        return;
    }
    let missing = Path::new("/trigger-git/no/such/dir");
    let err = run(missing, &["--version"], TIMEOUT).unwrap_err();
    assert!(matches!(err, GitError::Io(_)), "{err:?}");
}

#[cfg(unix)]
#[test]
fn a_slow_command_is_killed_at_the_timeout() {
    if !git_available() {
        return;
    }
    let started = std::time::Instant::now();
    let err = run(
        &here(),
        &["-c", "alias.slow=!sleep 5", "slow"],
        Duration::from_millis(200),
    )
    .unwrap_err();
    assert!(matches!(err, GitError::Timeout(_)), "{err:?}");
    assert!(started.elapsed() < Duration::from_secs(4));
}

#[test]
fn errors_read_as_plain_sentences() {
    assert_eq!(GitError::NotInstalled.to_string(), "git was not found");
    assert_eq!(
        GitError::Timeout("fetch".into()).to_string(),
        "git fetch timed out"
    );
}
