---
id: T12
epic: 010-trigger-docker
title: Add the Supervisor thread and its replay tests
status: todo
depends_on: [T11]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-docker/src/supervisor.rs
  - src-tauri/crates/trigger-docker/src/supervisor/fakes.rs
  - src-tauri/crates/trigger-docker/src/supervisor/tests.rs
---

## Goal

`Supervisor::start` runs the engine on a named thread: spawn the child, read lines, parse each with `parse_line`, overwrite `event.engine` with the engine actually running, call `fire`, and hand every result to `emit`. When the child ends it waits and spawns again. `fakes.rs` holds the fake processes the tests use. Three tests prove replay, the engine overwrite and that unwanted lines emit nothing.

## Read first

- `src-tauri/crates/trigger-docker/src/supervisor.rs` the file as T11 left it
- `src-tauri/crates/trigger-docker/src/logic.rs` `parse_line`, `fire`, `Armed`, `Payload`, `EngineEvent` (its pub `engine` field) and `Engine`
- `src-tauri/crates/trigger-core/src/testkit.rs:60-76` the `rule` helper the fakes use
- `plans/010-trigger-docker/epic.md` Decision 9

## Steps

1. Replace the whole of `src-tauri/crates/trigger-docker/src/supervisor.rs` with this. New since T11: the imports for `JoinHandle`, `RuleId` and the `logic` items; the `Emit` and `Pause` types; `real_pause`; `Supervisor` with `start`, `stop` and drop; the private `run` loop; and `mod fakes;` at the end. Read `run`: a run that gave at least one line resets the backoff counter, a run that gave nothing raises it; `stop` publishes the child's `Kill` handle in `current` so another thread can kill it; the check of `stop.is_stopped()` after publishing closes the race where `stop` ran first. The thread is named `docker-events-docker` or `docker-events-podman`.

```rust
//! Keeps one engine child process running and feeds its lines to the rules.
//! The child is behind `EngineProcess`, so tests use fakes. Pure of the OS.

// Removed when lib.rs reads everything in this file (slice C).
#![allow(dead_code)]

use std::io;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use trigger_core::RuleId;

use crate::logic::{fire, parse_line, Armed, Engine, Payload};

/// How long to wait before starting the child again. `attempt` counts the
/// failed runs in a row, from 0: 1, 2, 4, 8, 16, 32, then 60 seconds for ever.
pub fn backoff(attempt: u32) -> Duration {
    Duration::from_secs(1u64 << attempt.min(6)).min(Duration::from_secs(60))
}

/// Kills a running child. Another thread may be blocked in `Lines::next_line`
/// when this is called, and that call must return `None` soon after.
pub trait Kill: Send + Sync {
    /// Kills the child, then waits for it so it does not stay a zombie. Safe to
    /// call twice and after the child has already exited.
    fn kill(&self);
}

/// The output of one running child.
pub trait Lines: Send {
    /// The next line of stdout without its line ending. Blocks. `None` means
    /// the child has exited or was killed.
    fn next_line(&mut self) -> Option<String>;
    /// A handle for killing this child from another thread.
    fn killer(&self) -> Arc<dyn Kill>;
}

/// Something that can start the engine's event stream, over and over.
pub trait EngineProcess: Send + Sync {
    /// The engine this process talks to. The supervisor stamps it on every
    /// event, so a rule that names an engine matches what really ran.
    fn engine(&self) -> Engine;
    /// Starts a new child.
    fn spawn(&self) -> io::Result<Box<dyn Lines>>;
}

/// Locks a mutex even if another thread panicked while holding it. The data
/// here is a flag or a handle, so it is never half written.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A flag that says "stop now" and wakes anyone sleeping on it. Shared by the
/// caller and the supervisor thread.
#[derive(Default)]
pub struct Stop {
    stopped: Mutex<bool>,
    changed: Condvar,
}

impl Stop {
    pub fn is_stopped(&self) -> bool {
        *lock(&self.stopped)
    }

    /// Sets the flag and wakes every `wait`.
    pub fn stop(&self) {
        *lock(&self.stopped) = true;
        self.changed.notify_all();
    }

    /// Sleeps for `delay`, or less if `stop` is called meanwhile. Returns true
    /// if the flag is set.
    pub fn wait(&self, delay: Duration) -> bool {
        let guard = lock(&self.stopped);
        let (guard, _) = self
            .changed
            .wait_timeout_while(guard, delay, |stopped| !*stopped)
            .unwrap_or_else(PoisonError::into_inner);
        *guard
    }
}

/// Called once for each rule an engine event fires: (rule id, kind, payload).
pub type Emit = Arc<dyn Fn(RuleId, String, Payload) + Send + Sync>;

/// Sleeps for the delay before a restart, or less when `Stop` is set. Tests
/// pass one that does not sleep.
pub type Pause = Arc<dyn Fn(Duration, &Stop) + Send + Sync>;

/// The pause the app uses: really sleeps, and wakes early on stop.
pub fn real_pause(delay: Duration, stop: &Stop) {
    stop.wait(delay);
}

/// The child that is running now, so `stop` can kill it.
type Current = Arc<Mutex<Option<Arc<dyn Kill>>>>;

/// Runs the engine child on its own thread, restarts it when it exits, and
/// kills it on `stop` or drop.
pub struct Supervisor {
    stop: Arc<Stop>,
    current: Current,
    thread: Option<JoinHandle<()>>,
}

impl Supervisor {
    /// Starts the thread. It spawns the child, reads lines, calls `emit` for
    /// every rule an event fires, and spawns again (after `pause`) when the
    /// child ends or fails to start.
    pub fn start(
        process: Arc<dyn EngineProcess>,
        rules: Vec<Armed>,
        emit: Emit,
        pause: Pause,
    ) -> io::Result<Supervisor> {
        let stop = Arc::new(Stop::default());
        let current: Current = Arc::new(Mutex::new(None));
        let name = format!("docker-events-{}", process.engine().as_str());
        let thread = {
            let (stop, current) = (stop.clone(), current.clone());
            std::thread::Builder::new()
                .name(name)
                .spawn(move || run(&*process, &rules, &*emit, &*pause, &stop, &current))?
        };
        Ok(Supervisor {
            stop,
            current,
            thread: Some(thread),
        })
    }

    /// Kills the child, waits for it, and joins the thread.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.stop();
        let running = lock(&self.current).take();
        if let Some(kill) = running {
            kill.kill();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The thread body. A run that delivered at least one line counts as healthy
/// and starts the backoff over; a run that gave nothing makes the next wait
/// longer.
fn run(
    process: &dyn EngineProcess,
    rules: &[Armed],
    emit: &(dyn Fn(RuleId, String, Payload) + Send + Sync),
    pause: &(dyn Fn(Duration, &Stop) + Send + Sync),
    stop: &Stop,
    current: &Current,
) {
    let mut failures: u32 = 0;
    while !stop.is_stopped() {
        let mut got_line = false;
        if let Ok(mut lines) = process.spawn() {
            let kill = lines.killer();
            *lock(current) = Some(kill.clone());
            // `stop` may have run just before the line above published the
            // handle, and then it had nothing to kill.
            if stop.is_stopped() {
                kill.kill();
            }
            while let Some(line) = lines.next_line() {
                got_line = true;
                if let Some(mut event) = parse_line(&line) {
                    event.engine = process.engine();
                    for (id, kind, payload) in fire(rules, &event) {
                        emit(id, kind, payload);
                    }
                }
            }
            // Reap the child if it ended by itself.
            kill.kill();
            *lock(current) = None;
        }
        if stop.is_stopped() {
            break;
        }
        if got_line {
            failures = 0;
        }
        pause(backoff(failures), stop);
        if !got_line {
            failures = failures.saturating_add(1);
        }
    }
}

#[cfg(test)]
mod fakes;
#[cfg(test)]
mod tests;
```

2. Create `src-tauri/crates/trigger-docker/src/supervisor/fakes.rs` with exactly this content. `FakeProcess` plays its scripts one per `spawn`; when they run out it returns a child that stays alive until killed (so a test never spins). A `Lines` script is a child that prints its lines then exits by itself.

```rust
//! Fake engine processes for the supervisor tests.

use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use trigger_core::testkit::rule;
use trigger_core::RuleId;

use super::*;
use crate::logic::{Armed, Engine, Payload};

/// A real Docker `start` line and a real `die` line (exit code 3), from R01.
pub const START: &str = r#"{"Type":"container","Action":"start","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837794,"timeNano":1791837794607003258}"#;
pub const DIE: &str = r#"{"Type":"container","Action":"die","Actor":{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Attributes":{"exitCode":"3","image":"alpine:3","name":"r01x"}},"scope":"local","time":1791837807,"timeNano":1791837807993939750}"#;

/// What one `spawn` call does.
pub enum Script {
    /// The spawn itself fails.
    Fail,
    /// The child prints these lines and exits.
    Lines(Vec<&'static str>),
}

/// Plays the scripts in order, one per `spawn`. When they run out, `spawn`
/// returns a child that prints nothing and stays alive until it is killed.
pub struct FakeProcess {
    pub engine: Engine,
    scripts: Mutex<VecDeque<Script>>,
    pub spawns: AtomicUsize,
    pub kills: Arc<AtomicUsize>,
}

impl FakeProcess {
    pub fn new(engine: Engine, scripts: Vec<Script>) -> Arc<FakeProcess> {
        Arc::new(FakeProcess {
            engine,
            scripts: Mutex::new(scripts.into()),
            spawns: AtomicUsize::new(0),
            kills: Arc::new(AtomicUsize::new(0)),
        })
    }
}

struct FakeKill {
    gate: Arc<Stop>,
    kills: Arc<AtomicUsize>,
}

impl Kill for FakeKill {
    fn kill(&self) {
        self.kills.fetch_add(1, Ordering::SeqCst);
        self.gate.stop();
    }
}

struct FakeLines {
    pending: VecDeque<&'static str>,
    gate: Arc<Stop>,
    kills: Arc<AtomicUsize>,
}

impl Lines for FakeLines {
    fn next_line(&mut self) -> Option<String> {
        if let Some(line) = self.pending.pop_front() {
            return Some(line.to_string());
        }
        // Out of lines. A child that exits by itself has its gate open, so
        // this returns at once. A child that stays alive waits here until
        // `kill` opens the gate.
        self.gate.wait(Duration::from_secs(30));
        None
    }

    fn killer(&self) -> Arc<dyn Kill> {
        Arc::new(FakeKill {
            gate: self.gate.clone(),
            kills: self.kills.clone(),
        })
    }
}

impl EngineProcess for FakeProcess {
    fn engine(&self) -> Engine {
        self.engine
    }

    fn spawn(&self) -> io::Result<Box<dyn Lines>> {
        self.spawns.fetch_add(1, Ordering::SeqCst);
        let script = self.scripts.lock().unwrap().pop_front();
        let gate = Arc::new(Stop::default());
        let pending = match script {
            Some(Script::Fail) => return Err(io::Error::other("no such program")),
            Some(Script::Lines(lines)) => {
                // This child exits by itself once its lines are read.
                gate.stop();
                lines.into()
            }
            // Scripts ran out: a child that stays alive until killed.
            None => VecDeque::new(),
        };
        Ok(Box::new(FakeLines {
            pending,
            gate,
            kills: self.kills.clone(),
        }))
    }
}

pub fn armed(kind: &str, when: serde_json::Value) -> Armed {
    let r = rule("p/t", kind, when);
    let parsed = crate::rule::parse(&r).unwrap();
    Armed {
        id: r.id,
        kind: r.kind,
        when: parsed,
    }
}

/// What `emit` was called with, in order.
pub type Seen = Arc<Mutex<Vec<(RuleId, String, Payload)>>>;

pub fn recorder() -> (Seen, Emit) {
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let emit: Emit = Arc::new(move |id, kind, payload| {
        sink.lock().unwrap().push((id, kind, payload));
    });
    (seen, emit)
}

/// The delays `pause` was asked for. The pause does not sleep.
pub type Delays = Arc<Mutex<Vec<Duration>>>;

pub fn recording_pause() -> (Delays, Pause) {
    let delays: Delays = Arc::new(Mutex::new(Vec::new()));
    let sink = delays.clone();
    let pause: Pause = Arc::new(move |delay, _stop| {
        sink.lock().unwrap().push(delay);
    });
    (delays, pause)
}

/// Polls until `done` is true. Panics after five seconds.
pub fn wait_until(what: &str, done: impl Fn() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
```

3. Append this to the end of `src-tauri/crates/trigger-docker/src/supervisor/tests.rs` after a blank line.

```rust

mod loop_tests {
    use super::super::fakes::*;
    use super::*;
    use serde_json::json;

    fn started_rule() -> Armed {
        armed("docker.container-started", json!({}))
    }

    #[test]
    fn lines_are_parsed_matched_and_emitted_in_order() {
        let process = FakeProcess::new(
            Engine::Docker,
            vec![Script::Lines(vec![START, "not json at all", DIE])],
        );
        let (seen, emit) = recorder();
        let (_delays, pause) = recording_pause();
        let rules = vec![started_rule(), armed("docker.container-stopped", json!({}))];
        let sup = Supervisor::start(process, rules, emit, pause).unwrap();
        wait_until("two events", || seen.lock().unwrap().len() == 2);
        sup.stop();
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].1, "docker.container-started");
        assert_eq!(seen[0].2["name"], json!("r01c"));
        assert_eq!(seen[1].1, "docker.container-stopped");
        assert_eq!(seen[1].2["name"], json!("r01x"));
    }

    #[test]
    fn the_running_engine_replaces_the_one_the_parser_guessed() {
        // The line is Docker shaped, but the process says it is Podman.
        let process = FakeProcess::new(Engine::Podman, vec![Script::Lines(vec![START])]);
        let (seen, emit) = recorder();
        let (_delays, pause) = recording_pause();
        let rules = vec![
            armed("docker.container-started", json!({ "engine": "podman" })),
            armed("docker.container-started", json!({ "engine": "docker" })),
        ];
        let sup = Supervisor::start(process, rules, emit, pause).unwrap();
        wait_until("one event", || !seen.lock().unwrap().is_empty());
        sup.stop();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].2["engine"], json!("podman"));
    }

    #[test]
    fn a_line_no_rule_wants_emits_nothing() {
        let process = FakeProcess::new(Engine::Docker, vec![Script::Lines(vec![DIE])]);
        let (seen, emit) = recorder();
        let (delays, pause) = recording_pause();
        let sup = Supervisor::start(process, vec![started_rule()], emit, pause).unwrap();
        wait_until("a restart", || !delays.lock().unwrap().is_empty());
        sup.stop();
        assert!(seen.lock().unwrap().is_empty());
    }
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-docker`, then the acceptance commands. Run `cd src-tauri && cargo test -p trigger-docker supervisor` three times; it must pass every time (the tests use threads).

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker` passes: 8 tests in the library under `supervisor::tests` (5 from T10 and T11, 3 new `loop_tests`) on top of slice A's
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (it may print warnings for template text in `README.md`, `src/lib.rs` and `src/os.rs`; those stay until slice C)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/010-trigger-docker/tasks/`

## Out of scope

- The README. The supervisor is internal and nothing reads it until slice C, which writes the README parts that mention it.
- Editing `src/logic.rs`, `src/rule.rs`, `src/os.rs` or any slice A test. Use their names as they are.
- Adding dependencies. The supervisor uses `std` only.
- Writing the real `std::process` engine, `find_engine` or the version check (slice C).
- Removing the `#![allow(dead_code)]` line in `supervisor.rs`. Slice C removes it.
- Registering the crate in `src-tauri/Cargo.toml` or `sources()` (slice D).
- Adding the restart tests or the kill tests. T13 and T14 do that.
- Logging the error when `spawn` fails. The supervisor just waits and tries again.
- Making `Supervisor::start` return a trigger-core `Handle`. Slice C does that.

## If stuck

- If `fire`, `parse_line` or `Armed` is not found in `crate::logic`, set status blocked and say which. Slice A (T03 to T07) defines them.
- If `EngineEvent` has no pub `engine` field, set status blocked. Do not edit `logic.rs`.
- If `trigger_core::testkit::rule` is not found, set status blocked and quote the error.
- If a test fails only sometimes, set status blocked and say which test. Do not add sleeps.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
