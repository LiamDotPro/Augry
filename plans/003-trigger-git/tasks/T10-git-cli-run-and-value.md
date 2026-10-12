---
id: T10
epic: 003-trigger-git
title: Add git.rs with run and value
status: todo
depends_on: [T09]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/git.rs
  - src-tauri/crates/trigger-git/src/git/tests.rs
  - src-tauri/crates/trigger-git/src/git/testutil.rs
  - src-tauri/crates/trigger-git/src/lib.rs
---

## Goal

`src/git.rs` exists: `run` and `value` call the git command line with the app's settings (no prompts, no pager, no console window on Windows, a timeout, non zero exit as data) and a macOS fallback to the Homebrew paths. It is std only, so the crate does not depend on the app.

## Read first

- `plans/003-trigger-git/epic.md` Decision 7
- `plans/003-trigger-git/research/R02-git-cli-pattern.md` "Answer" and "Implications for the cards"
- `src-tauri/src/git/cli.rs:1-140` the app's version this file copies
- `src-tauri/crates/trigger-git/src/lib.rs:1-12` where the `mod` lines are

## Steps

1. Check the file is new: `ls src-tauri/crates/trigger-git/src` lists `lib.rs logic.rs os.rs rule.rs` and no `git.rs`. If `git.rs` or a `git` folder exists, use If stuck.

2. Create `src-tauri/crates/trigger-git/src/git.rs` with exactly this content. Note the two things the app's version does not have: the program list (the macOS fallback) and the check that the folder exists (a missing folder also makes the spawn say `NotFound`, which must not look like a missing git).

```rust
//! The git command line, std only. This is the pattern of the app's
//! `git/cli.rs` copied here, because a trigger crate may not depend on the
//! app (research note R02). Together with `os.rs` it is the crate's OS layer.

// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]

use std::fmt;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The time most calls get before they are killed.
pub const TIMEOUT: Duration = Duration::from_secs(3);
/// `git fetch` talks to the network and gets longer.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(10);

/// Why git could not be run. A non zero exit is not an error: see `GitOutput`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitError {
    NotInstalled,
    /// The command took longer than its timeout and was killed. Holds the
    /// first argument, such as `fetch`.
    Timeout(String),
    Io(String),
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitError::NotInstalled => write!(f, "git was not found"),
            GitError::Timeout(what) => write!(f, "git {what} timed out"),
            GitError::Io(why) => write!(f, "git could not run: {why}"),
        }
    }
}

impl std::error::Error for GitError {}

/// What a finished git command printed. `ok` is false for a non zero exit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOutput {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// The programs tried in order. Bare `git` first, like the app. A macOS app
/// started from the Dock has a short PATH, so a missing `git` falls back to
/// the two usual Homebrew locations.
#[cfg(target_os = "macos")]
const PROGRAMS: &[&str] = &["git", "/opt/homebrew/bin/git", "/usr/local/bin/git"];
#[cfg(not(target_os = "macos"))]
const PROGRAMS: &[&str] = &["git"];

/// Runs `git <args>` in `cwd`. Err only when git cannot run or times out.
pub fn run(cwd: &Path, args: &[&str], timeout: Duration) -> Result<GitOutput, GitError> {
    run_with(PROGRAMS, cwd, args, timeout)
}

/// Like `run`, trying each program in turn while the spawn says NotFound.
pub(crate) fn run_with(
    programs: &[&str],
    cwd: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<GitOutput, GitError> {
    // A missing working directory also makes the spawn say NotFound, which
    // would look like a missing git, so rule it out first.
    if !cwd.is_dir() {
        return Err(GitError::Io(format!(
            "{} is not a directory",
            cwd.display()
        )));
    }
    for program in programs {
        match spawn(program, cwd, args) {
            Ok(child) => return wait(child, args.first().copied().unwrap_or(""), timeout),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(GitError::Io(e.to_string())),
        }
    }
    Err(GitError::NotInstalled)
}

fn spawn(program: &str, cwd: &Path, args: &[&str]) -> std::io::Result<Child> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn()
}

/// Reads a pipe on its own thread so a full pipe cannot stop git from exiting.
fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> Option<JoinHandle<Vec<u8>>> {
    let mut pipe = pipe?;
    std::thread::Builder::new()
        .name("trigger-git-pipe".into())
        .spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes);
            bytes
        })
        .ok()
}

fn collect(reader: Option<JoinHandle<Vec<u8>>>) -> String {
    let bytes = reader.and_then(|h| h.join().ok()).unwrap_or_default();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn wait(mut child: Child, what: &str, timeout: Duration) -> Result<GitOutput, GitError> {
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(GitError::Timeout(what.to_string()));
            }
            Ok(None) => std::thread::sleep(POLL),
            Err(e) => return Err(GitError::Io(e.to_string())),
        }
    };
    Ok(GitOutput {
        ok: status.success(),
        stdout: collect(stdout),
        stderr: collect(stderr),
    })
}

/// Stdout of a successful command with trailing line breaks removed. None when
/// git failed, timed out, or printed nothing: callers treat all three as
/// "not available".
pub fn value(cwd: &Path, args: &[&str]) -> Option<String> {
    let out = run(cwd, args, TIMEOUT).ok()?;
    if !out.ok {
        return None;
    }
    let text = out.stdout.trim_end_matches(['\r', '\n']);
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod testutil;
```

3. Create the folder `src-tauri/crates/trigger-git/src/git/` and in it the file `tests.rs` with exactly this content.

```rust
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
```

4. In the same folder create `testutil.rs` with exactly this content. Later cards add a temporary repository helper to it.

```rust
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
```

5. In `src-tauri/crates/trigger-git/src/lib.rs`, add the line `mod git;` directly above the line `mod logic;`. The three lines that follow it stay as they are, so the block reads `mod git;`, `mod logic;`, `mod os;`, `mod rule;`.

6. Run `cd src-tauri && cargo fmt -p trigger-git`. It may reorder the two `#[cfg(test)]` module lines at the end of `git.rs`; that is fine.

Note on `check-crates.mjs`: it prints warnings, not errors, about template text. Until slice C it may print `README.md still has template text`, `src/lib.rs still has template code`, `src/os.rs still has template code` and a length warning for `src/logic/tests.rs`. Warnings are fine; only errors fail the check. Do not try to remove them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes: 47 tests in the library, 1 in `readme`, `live` shows 1 ignored
- [ ] `cd src-tauri && cargo test -p trigger-git git::` runs 8 tests and they pass
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- `GitRepo`, snapshots and the `Repo` trait (T12, T13).
- Temporary repository helpers (T11).
- Calling git from `lib.rs`, `os.rs` or `logic.rs`.
- Adding `thiserror`, `which`, `dirs` or any other dependency. This file is std only.
- Reading the login shell PATH like `agent/installed.rs` does. Only the three program names in the file are tried.
- README changes (T16 and T19 write the README rows once the pieces they describe exist).
- Editing `src-tauri/src/git/cli.rs`.

## If stuck

- If `git.rs` or `src/git/` already exists, set status blocked and say so. Do not overwrite it.
- If git is not installed on this machine, the git tests print `skipped: git is not installed` and pass. That is fine; add a Log line saying so.
- If `a_slow_command_is_killed_at_the_timeout` fails, set status blocked and quote the output. Do not change the test.
- If clippy reports `dead_code` in `git.rs`, the line `#![allow(dead_code)]` near the top was lost. Put it back.
- If `cargo fmt` rewrites the files heavily, you mis-pasted; re-copy.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
