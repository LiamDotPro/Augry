---
id: T14
epic: 010-trigger-docker
title: Test that stop kills the child, and check slice B
status: done
depends_on: [T13]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-docker/src/supervisor.rs
  - src-tauri/crates/trigger-docker/src/supervisor/kill_tests.rs
---

## Goal

Stopping or dropping the supervisor kills the child and returns quickly. Two tests use the fake; one unix only test starts a real `sleep 1000`, stops the supervisor and checks the process is gone. It also closes slice B with its checks: file sizes, stable repeated runs and no stray process.

## Read first

- `plans/010-trigger-docker/research/R02-engine-binary.md` the part about killing the child (kill then wait)
- `src-tauri/crates/trigger-docker/src/supervisor.rs` the `Kill` trait, `Supervisor::shutdown`
- `src-tauri/crates/trigger-docker/src/supervisor/fakes.rs` the fake the first two tests use

## Steps

1. In `src-tauri/crates/trigger-docker/src/supervisor.rs` the file ends with

```rust
#[cfg(test)]
mod fakes;
#[cfg(test)]
mod tests;
```

Change those four lines so the file ends with exactly:

```rust
#[cfg(test)]
mod fakes;
#[cfg(test)]
mod kill_tests;
#[cfg(test)]
mod tests;
```

2. Create `src-tauri/crates/trigger-docker/src/supervisor/kill_tests.rs` with exactly this content. The `real_child` module is `#[cfg(unix)]` because it needs `sleep` and `kill -0`; on Windows only the two fake tests run. `SleepProcess` is test code only: the real engine process is slice C.

```rust
//! Stopping kills the child, whether it is a fake or a real process.

use std::sync::atomic::Ordering;

use super::fakes::*;
use super::*;

#[test]
fn stop_kills_a_child_that_is_still_running() {
    let process = FakeProcess::new(Engine::Docker, vec![]);
    let (_seen, emit) = recorder();
    let (_delays, pause) = recording_pause();
    let sup = Supervisor::start(process.clone(), vec![], emit, pause).unwrap();
    wait_until("the first spawn", || {
        process.spawns.load(Ordering::SeqCst) == 1
    });
    sup.stop();
    assert!(process.kills.load(Ordering::SeqCst) >= 1);
    assert_eq!(process.spawns.load(Ordering::SeqCst), 1);
}

#[test]
fn dropping_the_supervisor_kills_the_child_too() {
    let process = FakeProcess::new(Engine::Docker, vec![]);
    let (_seen, emit) = recorder();
    let (_delays, pause) = recording_pause();
    let sup = Supervisor::start(process.clone(), vec![], emit, pause).unwrap();
    wait_until("the first spawn", || {
        process.spawns.load(Ordering::SeqCst) == 1
    });
    drop(sup);
    assert!(process.kills.load(Ordering::SeqCst) >= 1);
}

#[cfg(unix)]
mod real_child {
    use std::io::{BufRead, BufReader};
    use std::process::{Child, ChildStdout, Command, Stdio};

    use super::*;

    /// A real `sleep 1000` that never prints a line. It stands in for
    /// `docker events`, which also blocks for ever.
    struct SleepProcess {
        pid: Arc<Mutex<Option<u32>>>,
    }

    struct SleepLines {
        out: BufReader<ChildStdout>,
        child: Arc<Mutex<Child>>,
    }

    struct SleepKill {
        child: Arc<Mutex<Child>>,
    }

    impl Kill for SleepKill {
        fn kill(&self) {
            let mut child = lock(&self.child);
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    impl Lines for SleepLines {
        fn next_line(&mut self) -> Option<String> {
            let mut line = String::new();
            match self.out.read_line(&mut line) {
                Ok(0) | Err(_) => None,
                Ok(_) => Some(line.trim_end().to_string()),
            }
        }

        fn killer(&self) -> Arc<dyn Kill> {
            Arc::new(SleepKill {
                child: self.child.clone(),
            })
        }
    }

    impl EngineProcess for SleepProcess {
        fn engine(&self) -> Engine {
            Engine::Docker
        }

        fn spawn(&self) -> io::Result<Box<dyn Lines>> {
            let mut child = Command::new("sleep")
                .arg("1000")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?;
            let out = BufReader::new(child.stdout.take().expect("stdout is piped"));
            *lock(&self.pid) = Some(child.id());
            Ok(Box::new(SleepLines {
                out,
                child: Arc::new(Mutex::new(child)),
            }))
        }
    }

    /// True if a process with this id exists (signal 0 only checks).
    fn alive(pid: u32) -> bool {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[test]
    fn stop_kills_and_reaps_a_real_long_running_child() {
        let pid = Arc::new(Mutex::new(None));
        let process = Arc::new(SleepProcess { pid: pid.clone() });
        let (_seen, emit) = recorder();
        let (_delays, pause) = recording_pause();
        let sup = Supervisor::start(process, vec![], emit, pause).unwrap();
        wait_until("the child to start", || lock(&pid).is_some());
        let pid = lock(&pid).unwrap();
        assert!(alive(pid), "sleep should be running before stop");
        let started = std::time::Instant::now();
        sup.stop();
        assert!(started.elapsed() < Duration::from_secs(5));
        // `wait` reaped it, so the id is gone and not a zombie.
        assert!(!alive(pid), "sleep should be gone after stop");
    }
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-docker`, then the acceptance commands. Run `cd src-tauri && cargo test -p trigger-docker supervisor` three times; it must pass every time.

4. Slice B checks. Every file is under 400 lines: `wc -l src-tauri/crates/trigger-docker/src/supervisor.rs src-tauri/crates/trigger-docker/src/supervisor/*.rs` (if one is not, use If stuck). The library tests are stable: run `cd src-tauri && for i in 1 2 3 4 5 6 7 8 9 10; do cargo test -p trigger-docker supervisor 2>&1 | grep "test result" | head -1; done` and every line says `ok`.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker` passes: 15 tests in the library under `supervisor::` (12 before, 3 new in `kill_tests`; on Windows 14) on top of slice A's
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (it may print warnings for template text in `README.md`, `src/lib.rs` and `src/os.rs`; those stay until slice C)
- [ ] `grep -n "unwrap()\|expect(" src-tauri/crates/trigger-docker/src/supervisor.rs` prints nothing (library code has no unwrap or expect)
- [ ] `pgrep -f "sleep 1000"` prints nothing (skip on Windows)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/010-trigger-docker/tasks/`

## Out of scope

- Any other change to `supervisor.rs`.
- A Windows version of the real child test.
- Leaving a `sleep 1000` running: if a test is interrupted, run `pkill -f "sleep 1000"` yourself.
- Using `docker` or `podman` in a test. Those are slice C.

## If stuck

- If a file is over 400 lines, set status blocked and say which file and how long. Do not split it.
- If a test fails in only some of the ten runs, set status blocked and name the test.
- If the real child test fails with `sleep should be gone after stop`, set status blocked, run `pkill -f "sleep 1000"`, and quote the failure. Do not weaken the assertion.
- If `sleep` or `kill` is not found, set status blocked and say which.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
