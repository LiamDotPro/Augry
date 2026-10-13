//! Tests for `run`. They start `/bin/sh`, so they only exist on Unix.

#![cfg(unix)]

use super::*;

fn sh(script: &str, timeout: Duration) -> Result<Finished, RunError> {
    let _guard = spawn_lock();
    run(
        Path::new("/bin/sh"),
        &["-c".to_string(), script.to_string()],
        timeout,
    )
}

#[test]
fn stdout_and_the_exit_code_come_back() {
    let done = sh("printf hello; exit 3", Duration::from_secs(10)).unwrap();
    assert_eq!(done.stdout, b"hello");
    assert_eq!(done.exit, Some(3));
}

#[test]
fn stderr_is_not_kept() {
    let done = sh("printf secret >&2; printf out", Duration::from_secs(10)).unwrap();
    assert_eq!(done.stdout, b"out");
}

#[test]
fn gh_is_told_not_to_prompt_or_check_for_updates() {
    let done = sh(
        "printf '%s%s' \"$GH_PROMPT_DISABLED\" \"$GH_NO_UPDATE_NOTIFIER\"",
        Duration::from_secs(10),
    )
    .unwrap();
    assert_eq!(done.stdout, b"11");
}

#[test]
fn stdin_is_closed_so_a_program_that_reads_it_ends() {
    let done = sh("cat; printf done", Duration::from_secs(10)).unwrap();
    assert_eq!(done.stdout, b"done");
    assert_eq!(done.exit, Some(0));
}

#[test]
fn a_big_answer_does_not_stall_the_child() {
    let done = sh("head -c 1000000 /dev/zero", Duration::from_secs(10)).unwrap();
    assert_eq!(done.stdout.len(), 1_000_000);
}

#[test]
fn a_child_that_runs_too_long_is_killed() {
    let started = Instant::now();
    let got = sh("exec sleep 30", Duration::from_millis(300));
    assert_eq!(got.unwrap_err(), RunError::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_program_that_does_not_exist_is_a_spawn_error() {
    let _guard = spawn_lock();
    let missing = std::env::temp_dir().join("tg-github-no-such-gh");
    let got = run(&missing, &[], Duration::from_secs(5));
    assert!(matches!(got, Err(RunError::Spawn(_))), "{got:?}");
}

#[test]
fn a_signal_gives_no_exit_code() {
    let done = sh("kill -9 $$", Duration::from_secs(10)).unwrap();
    assert_eq!(done.exit, None);
}
