---
id: T21
epic: 011-trigger-github
title: Add the child process runner with a timeout
status: todo
depends_on: [T20]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-github/src/os.rs
  - src-tauri/crates/trigger-github/src/os/run_tests.rs
---

## Goal

`os.rs` has `run(exe, args, timeout)`: it starts a program the way Decision 8 says `gh` must be started (no stdin, `GH_PROMPT_DISABLED=1`, `GH_NO_UPDATE_NOTIFIER=1`, no console window on Windows, stderr thrown away), reads its stdout on its own thread, waits at most `timeout` and kills it when the time runs out. Eight tests prove it on macOS and Linux with `/bin/sh`. Nothing calls it yet.

## Read first

- `plans/011-trigger-github/epic.md` Decisions 7 and 8 (the environment variables, null stdin, `CREATE_NO_WINDOW`, the timeouts, one child at a time)
- `plans/011-trigger-github/research/R02-rate-limits-and-gh.md` point 5 of "Availability check" (why stdin is null and the two variables are set)
- `src-tauri/crates/trigger-github/src/os.rs` the file you replace (a stub)
- `src-tauri/crates/trigger-fs/src/os.rs` for how another trigger crate lays out its OS file (read only, copy nothing from it)

## Steps

1. Replace the whole of `src-tauri/crates/trigger-github/src/os.rs` with exactly this.

Points that matter:

- Stderr goes to `Stdio::null()`. `gh` prints login hints and token prefixes there, and nothing in this crate may read, log or keep them.
- `run` returns `Err(RunError::TimedOut)` after killing the child, `Err(RunError::Spawn(..))` when the program cannot start, and `Ok(Finished)` otherwise. `exit` is `None` when a signal ended the process.
- `no_window` is the only platform code. On Windows it sets `CREATE_NO_WINDOW` (0x08000000); elsewhere it does nothing.
- `availability()` is still the stub. The next card replaces it. `#![allow(dead_code)]` is there because nothing calls `run` yet; the last card of the slice removes it.
- `spawn_lock` exists only for tests. Tests that start children (and later tests that write a script file and run it) take it, so Linux never says "text file busy".

```rust
//! The only file that talks to the operating system: finds `gh` and runs it.
//! What to run and how to read the answer is decided in `logic/cli.rs` and
//! `logic/response.rs`; this file only starts children.

// Removed in the last card of slice C, when the poll loop uses everything here.
#![allow(dead_code)]

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use trigger_core::Availability;

pub fn availability() -> Availability {
    Availability::Unsupported {
        reason: "not implemented yet".into(),
    }
}

/// What a child that finished on its own left behind. Its stderr is thrown
/// away: `gh` prints the login and token hints there, and nothing in this
/// crate may log or keep them.
#[derive(Debug)]
pub struct Finished {
    pub stdout: Vec<u8>,
    /// None when the process was ended by a signal.
    pub exit: Option<i32>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RunError {
    /// The program could not be started. Carries the OS error text.
    Spawn(String),
    /// It was still running when the time ran out, and was killed.
    TimedOut,
}

/// Runs `exe` with `args` and waits at most `timeout`. The child gets no
/// stdin, so `gh` can never stop to ask a question, and the two environment
/// variables that keep it from prompting or printing update notices. On
/// Windows it gets no console window. Stdout is read on its own thread so a
/// big answer cannot fill the pipe and stall the child.
pub fn run(exe: &Path, args: &[String], timeout: Duration) -> Result<Finished, RunError> {
    let mut command = Command::new(exe);
    command
        .args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    no_window(&mut command);
    let mut child = command
        .spawn()
        .map_err(|e| RunError::Spawn(e.to_string()))?;
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(RunError::Spawn("no stdout pipe".into()));
    };
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::Builder::new()
        .name("trigger-github-read".into())
        .spawn(move || {
            let mut buf = Vec::new();
            let _ = stdout.read_to_end(&mut buf);
            let _ = tx.send(buf);
        });
    if let Err(e) = reader {
        let _ = child.kill();
        let _ = child.wait();
        return Err(RunError::Spawn(e.to_string()));
    }

    let deadline = Instant::now() + timeout;
    let exit = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::TimedOut);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::Spawn(e.to_string()));
            }
        }
    };
    // The reader ends when the pipe closes, which is when the child exits. The
    // wait is only a guard against a grandchild that kept the pipe open.
    let stdout = rx.recv_timeout(Duration::from_secs(5)).unwrap_or_default();
    Ok(Finished { stdout, exit })
}

#[cfg(windows)]
fn no_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn no_window(_command: &mut Command) {}

/// Tests that start children take this lock, so a test that writes a script
/// file never has it open while another test forks (which makes Linux refuse
/// to run the script with "text file busy").
#[cfg(all(test, unix))]
pub(crate) fn spawn_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod run_tests;
```

2. Create `src-tauri/crates/trigger-github/src/os/run_tests.rs` with exactly this. The tests only exist on Unix, because they start `/bin/sh`.

```rust
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
```

3. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib os::` passes (8 tests on macOS and Linux, none on Windows)
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (the lib reports 123 tests on macOS and Linux)
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (a warning that `lib.rs` still has template code is expected until a later card)
- [ ] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- Finding `gh`, `GhCli`, the auth check and `availability()` (next cards).
- A `Windows` test for `no_window`. It is four lines and Windows CI compiles it.
- Reading stderr "to give better errors". It is thrown away on purpose.
- Removing `#![allow(dead_code)]` from `os.rs`.
- README changes.

## If stuck

- If `a_child_that_runs_too_long_is_killed` fails by taking over ten seconds, `exec sleep 30` was not killed. Do not change the test; set status blocked and say what `sh` and `sleep` are on this machine.
- If any test fails with "text file busy", a test is missing `let _guard = spawn_lock();`. Compare `run_tests.rs` with step 2.
- If the build fails on Windows in `no_window`, set status blocked and paste the error. Do not change the cfg attributes.
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
