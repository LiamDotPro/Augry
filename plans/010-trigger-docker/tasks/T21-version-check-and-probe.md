---
id: T21
epic: 010-trigger-docker
title: Add the version check and probe to os.rs
status: done
depends_on: [T20]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-docker/src/os.rs
  - src-tauri/crates/trigger-docker/src/os/tests.rs
---

## Goal

`os.rs` can ask an engine whether it is up. `probe(engine)` finds the tool and runs `<tool> version --format '{{.Server.Version}}'` with a five second limit, polling `try_wait`, killing and reaping the child on timeout, and with no console window on Windows. It returns `Probe::Missing`, `Probe::Silent(path)` or `Probe::Ready(path)`. Six tests cover it, four of them unix only.

## Read first

- `plans/010-trigger-docker/epic.md` Decision 8 (the version check)
- `plans/010-trigger-docker/research/R02-engine-binary.md` "Implications", items 5 and 6, and the evidence lines where `DOCKER_HOST` points at a missing socket (binary there, engine down, empty stdout, exit 1)
- `src-tauri/crates/trigger-docker/src/os.rs` the file as T20 left it
- `src-tauri/crates/trigger-docker/src/os/tests.rs` the tests so far
- `src-tauri/src/git/cli.rs:11-14` how the app polls a child with `try_wait` and sets `CREATE_NO_WINDOW` (read only, a different crate)

## Steps

1. In `src-tauri/crates/trigger-docker/src/os.rs` find these two lines and replace them with the five below. (`Read` is for the child's stdout, `Command` and `Stdio` run it, `Duration` and `Instant` time it, `Path` is for `command`.)

```rust
use std::path::PathBuf;
use std::sync::OnceLock;
```

becomes

```rust
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
```

2. In the same file find the last two lines

```rust
#[cfg(test)]
mod tests;
```

and insert this block directly above them, with one blank line between. `command` is the one place a `Command` is made, so the Windows flag is never forgotten; the same function builds the events child in T22. `answer` returns `None` for every failure (cannot start, nonzero exit, empty output, too slow), because the caller only needs "did it answer".

```rust
/// How long the engine gets to answer `version`.
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// How often a running child is checked.
const POLL: Duration = Duration::from_millis(10);

/// A command that does not open a console window on Windows.
fn command(program: &Path) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new(program);
        // CREATE_NO_WINDOW
        command.creation_flags(0x0800_0000);
        command
    }
    #[cfg(not(windows))]
    {
        Command::new(program)
    }
}

/// Runs the command and returns what it printed to stdout, trimmed, when it
/// exited with success and printed something within `limit`. A command that
/// takes longer is killed and reaped. Only for commands with short output.
fn answer(mut command: Command, limit: Duration) -> Option<String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < limit => std::thread::sleep(POLL),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    let out = out.trim().to_string();
    (!out.is_empty()).then_some(out)
}

/// What asking an engine found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    /// No tool for this engine was found.
    Missing,
    /// The tool is there but the engine did not answer `version` in time.
    Silent(PathBuf),
    /// The engine answered.
    Ready(PathBuf),
}

/// Finds the engine's tool and runs `version` against it. Takes up to five
/// seconds, so never call it from the UI thread.
pub fn probe(engine: Engine) -> Probe {
    let Some(path) = find_engine(engine) else {
        return Probe::Missing;
    };
    let mut version = command(&path);
    version.args(["version", "--format", "{{.Server.Version}}"]);
    match answer(version, VERSION_TIMEOUT) {
        Some(_) => Probe::Ready(path),
        None => Probe::Silent(path),
    }
}

```

3. Append this to the end of `src-tauri/crates/trigger-docker/src/os/tests.rs`, after a blank line. The `sh` helper and the four tests that use it are `#[cfg(unix)]`; the script ends with `exec sleep 30` so that killing the shell kills the sleep too.

```rust
/// A command that runs a shell script. The script ends with `exec` when it
/// should stay alive, so killing the shell also ends the sleep.
#[cfg(unix)]
fn sh(script: &str) -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", script]);
    command
}

#[cfg(unix)]
#[test]
fn answer_returns_trimmed_stdout_on_success() {
    let got = answer(sh("echo '  29.4.3  '"), Duration::from_secs(5));
    assert_eq!(got, Some("29.4.3".to_string()));
}

#[cfg(unix)]
#[test]
fn answer_is_none_when_the_command_fails() {
    assert_eq!(answer(sh("exit 1"), Duration::from_secs(5)), None);
    assert_eq!(answer(sh("echo hi; exit 1"), Duration::from_secs(5)), None);
}

#[cfg(unix)]
#[test]
fn answer_is_none_when_the_command_prints_nothing() {
    assert_eq!(answer(sh("exit 0"), Duration::from_secs(5)), None);
    assert_eq!(answer(sh("echo '   '"), Duration::from_secs(5)), None);
}

#[cfg(unix)]
#[test]
fn answer_kills_a_command_that_is_too_slow() {
    let started = Instant::now();
    let got = answer(sh("exec sleep 30"), Duration::from_millis(200));
    assert_eq!(got, None);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn answer_is_none_when_the_program_does_not_exist() {
    let missing = Command::new("/nonexistent/trigger-docker-test-program");
    assert_eq!(answer(missing, Duration::from_secs(5)), None);
}

#[test]
fn probe_is_consistent_with_find_engine() {
    for engine in [Engine::Docker, Engine::Podman] {
        match (find_engine(engine), probe(engine)) {
            (None, Probe::Missing) => {}
            (Some(found), Probe::Silent(path) | Probe::Ready(path)) => assert_eq!(found, path),
            (found, probe) => panic!("{found:?} does not fit {probe:?}"),
        }
    }
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-docker`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker os::` prints `16 passed` for the library on macOS and Linux (on Windows 11)
- [ ] `cd src-tauri && cargo test -p trigger-docker` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (it may print warnings for template text in `README.md` and `src/lib.rs`; T24 and T27 remove them)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/010-trigger-docker/tasks/`
- [ ] `cd src-tauri && cargo test -p trigger-docker os::tests::answer_kills` finishes in under two seconds

## Out of scope

- Running `events` or reading lines from a child (T22).
- Using `probe` from `lib.rs` (T23 and T24).
- Making the five second limit or the poll interval settings. They are the constants in the card.
- Passing the login shell environment, `DOCKER_HOST` or a docker context to the child.
- Windows tests: the Windows build is checked by CI running these same tests, of which only the unix ones are skipped.
- The README. Internal code only; the card that writes the README is T27, which describes this work.
- Removing the `#![allow(dead_code)]` line in `os.rs`.

## If stuck

- If `answer_kills_a_command_that_is_too_slow` takes more than five seconds, the shell was not killed or the pipe stayed open: check the script in the test still starts with `exec`. Do not raise the limit.
- If clippy says `unused_mut` in `command` on your OS, the `#[cfg(windows)]` block was written outside its braces: compare with the card.
- If `os.rs` has no line `use std::sync::OnceLock;`, T20 did not run. Set status blocked.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
