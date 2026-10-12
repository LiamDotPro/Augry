---
id: T22
epic: 010-trigger-docker
title: Add the real engine process to os.rs
status: done
depends_on: [T21]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-docker/src/os.rs
  - src-tauri/crates/trigger-docker/src/os/tests.rs
---

## Goal

`os.rs` has the real engine: `RealEngine` implements `EngineProcess` over `std::process`, runs `<tool> events` with the command line from Decision 6 (Docker: `--format '{{json .}}'`, Podman: `--format json`, both with `--filter type=container --filter type=image`, no `--since`), reads its stdout line by line, and kills then waits it on request, from another thread, while the reader is blocked. Seven tests cover the command lines and the reading and killing.

## Read first

- `plans/010-trigger-docker/epic.md` Decisions 6 and 9
- `plans/010-trigger-docker/research/R02-engine-binary.md` "Implications", items 7 and 8 (spawn by absolute path, `kill` then `wait`, keep the `Child` out of the reader)
- `src-tauri/crates/trigger-docker/src/supervisor.rs:1-50` the traits `Kill`, `Lines` and `EngineProcess` that are implemented here. Their names and signatures are fixed.
- `src-tauri/crates/trigger-docker/src/os.rs` the file as T21 left it
- `src-tauri/crates/trigger-docker/src/os/tests.rs` the tests so far, including the `sh` helper

## Steps

1. In `src-tauri/crates/trigger-docker/src/os.rs` replace these five lines at the top of the imports

```rust
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
```

with

```rust
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};
```

and directly under the line `use crate::logic::Engine;` add the line

```rust
use crate::supervisor::{EngineProcess, Kill, Lines};
```

2. In the same file find the last two lines

```rust
#[cfg(test)]
mod tests;
```

and insert this block directly above them, with one blank line between. Read the design: `ChildLines` owns the stdout reader, and the `Child` sits in an `Arc<Mutex<Child>>` that only `ChildKill` locks, so `kill` never waits behind a blocked `next_line`. Killing closes the pipe, which makes `read_until` return 0. `kill` ignores errors because the child may already have exited, and `wait` reaps it so it is not left as a zombie.

```rust
/// The arguments for the engine's event stream (Decision 6 of the epic). Only
/// container and image events are asked for: the `exec_*` events of health
/// checks are most of the traffic otherwise. There is no `--since`, so a
/// restarted child does not replay old events.
pub fn events_args(engine: Engine) -> Vec<&'static str> {
    let format = match engine {
        Engine::Docker => "{{json .}}",
        Engine::Podman => "json",
    };
    vec![
        "events",
        "--format",
        format,
        "--filter",
        "type=container",
        "--filter",
        "type=image",
    ]
}

/// A running child and a reader on its stdout.
struct ChildLines {
    // Shared with `ChildKill`. The reader below owns stdout, so this lock is
    // never held while a thread waits for a line.
    child: Arc<Mutex<Child>>,
    out: BufReader<ChildStdout>,
}

impl ChildLines {
    /// Starts the command with stdin and stderr closed and stdout piped.
    fn spawn(mut command: Command) -> io::Result<ChildLines> {
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::other("the child has no stdout"));
        };
        Ok(ChildLines {
            child: Arc::new(Mutex::new(child)),
            out: BufReader::new(stdout),
        })
    }
}

impl Lines for ChildLines {
    fn next_line(&mut self) -> Option<String> {
        let mut bytes = Vec::new();
        match self.out.read_until(b'\n', &mut bytes) {
            Ok(0) | Err(_) => None,
            Ok(_) => {
                while matches!(bytes.last(), Some(b'\n' | b'\r')) {
                    bytes.pop();
                }
                Some(String::from_utf8_lossy(&bytes).into_owned())
            }
        }
    }

    fn killer(&self) -> Arc<dyn Kill> {
        Arc::new(ChildKill(self.child.clone()))
    }
}

struct ChildKill(Arc<Mutex<Child>>);

impl Kill for ChildKill {
    fn kill(&self) {
        let mut child = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        // Both fail when the child already exited, which is fine.
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// The real event stream: the engine's tool run as a child process.
pub struct RealEngine {
    engine: Engine,
    path: PathBuf,
}

impl RealEngine {
    pub fn new(engine: Engine, path: PathBuf) -> RealEngine {
        RealEngine { engine, path }
    }
}

impl EngineProcess for RealEngine {
    fn engine(&self) -> Engine {
        self.engine
    }

    fn spawn(&self) -> io::Result<Box<dyn Lines>> {
        let mut events = command(&self.path);
        events.args(events_args(self.engine));
        Ok(Box::new(ChildLines::spawn(events)?))
    }
}

```

3. Append this to the end of `src-tauri/crates/trigger-docker/src/os/tests.rs`, after a blank line.

```rust
#[test]
fn the_docker_command_line_is_the_one_in_the_epic() {
    assert_eq!(
        events_args(Engine::Docker),
        [
            "events",
            "--format",
            "{{json .}}",
            "--filter",
            "type=container",
            "--filter",
            "type=image"
        ]
    );
}

#[test]
fn the_podman_command_line_asks_for_json() {
    assert_eq!(
        events_args(Engine::Podman),
        [
            "events",
            "--format",
            "json",
            "--filter",
            "type=container",
            "--filter",
            "type=image"
        ]
    );
}

#[test]
fn no_command_line_uses_since() {
    for engine in [Engine::Docker, Engine::Podman] {
        assert!(!events_args(engine).contains(&"--since"));
    }
}

#[cfg(unix)]
#[test]
fn child_lines_reads_lines_then_none_when_the_child_exits() {
    let mut lines = ChildLines::spawn(sh("echo one; printf 'two\\r\\n'; printf three")).unwrap();
    assert_eq!(lines.next_line(), Some("one".to_string()));
    assert_eq!(lines.next_line(), Some("two".to_string()));
    assert_eq!(lines.next_line(), Some("three".to_string()));
    assert_eq!(lines.next_line(), None);
    // Reaps the child. Killing one that has exited is fine, twice too.
    let killer = lines.killer();
    killer.kill();
    killer.kill();
}

#[cfg(unix)]
#[test]
fn killing_ends_a_child_that_never_prints_again() {
    let mut lines = ChildLines::spawn(sh("echo up; exec sleep 30")).unwrap();
    assert_eq!(lines.next_line(), Some("up".to_string()));
    let killer = lines.killer();
    let started = Instant::now();
    let reader = std::thread::spawn(move || lines.next_line());
    std::thread::sleep(Duration::from_millis(100));
    killer.kill();
    assert_eq!(reader.join().unwrap(), None);
    assert!(started.elapsed() < Duration::from_secs(5));
    killer.kill();
}

#[cfg(unix)]
#[test]
fn killing_waits_so_the_child_is_gone() {
    let lines = ChildLines::spawn(sh("exec sleep 30")).unwrap();
    let child = lines.child.clone();
    lines.killer().kill();
    // `try_wait` on a child that was already waited for still reports it.
    let status = child.lock().unwrap().try_wait().unwrap();
    assert!(status.is_some());
}

#[test]
fn a_program_that_does_not_exist_fails_to_spawn() {
    let missing = Command::new("/nonexistent/trigger-docker-test-program");
    assert!(ChildLines::spawn(missing).is_err());
    let real = RealEngine::new(
        Engine::Docker,
        PathBuf::from("/nonexistent/trigger-docker-test-program"),
    );
    assert!(real.spawn().is_err());
    assert_eq!(real.engine(), Engine::Docker);
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-docker`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker os::` prints `23 passed` for the library on macOS and Linux (on Windows 15)
- [ ] `cd src-tauri && cargo test -p trigger-docker` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (it may print warnings for template text in `README.md` and `src/lib.rs`; T24 and T27 remove them)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/010-trigger-docker/tasks/`
- [ ] `cd src-tauri && cargo test -p trigger-docker os::tests::killing` passes three times in a row

## Out of scope

- Starting a `Supervisor` or calling `RealEngine` from `lib.rs` (T24).
- Adding `--since`, other filters or `--format` variants. The two command lines are fixed by Decision 6.
- Reading stderr. It is closed (`Stdio::null()`), so a chatty engine cannot block on a full pipe.
- Changing `supervisor.rs` or its traits.
- Windows tests for the child (the unix ones use `sh`). The live tests (T25, T26) cover the real child on every platform.
- The README. Internal code only; the card that writes the README is T27, which describes this work.
- Removing the `#![allow(dead_code)]` line in `os.rs`.

## If stuck

- If `crate::supervisor::{EngineProcess, Kill, Lines}` does not resolve, slice B is not built. Set status blocked.
- If `killing_ends_a_child_that_never_prints_again` hangs, do not add a timeout in the test. The cause is a `Child` lock held across `read_until`: compare `ChildLines` with the card.
- If clippy reports `dead_code` on `RealEngine` or `events_args`, the `#![allow(dead_code)]` line at the top of `os.rs` is missing. Put it back as it was after the `//!` lines (with its `// Removed when ...` comment above).
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
