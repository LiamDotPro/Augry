---
id: T13
epic: 010-trigger-docker
title: Test restart, backoff, reset and stop during a wait
status: todo
depends_on: [T12]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-docker/src/supervisor/tests.rs
---

## Goal

Four tests prove the supervisor restarts a child that exits at once with waits 1, 2, 4, caps the wait at 60 s, starts the waits over after a child that gave lines, and cuts a long wait short on stop. They use the fake process and a pause that records the delay instead of sleeping, so they run in milliseconds.

## Read first

- `src-tauri/crates/trigger-docker/src/supervisor.rs` the `run` function and `Pause`
- `src-tauri/crates/trigger-docker/src/supervisor/fakes.rs` `FakeProcess`, `Script`, `recording_pause`, `wait_until`
- `src-tauri/crates/trigger-docker/src/supervisor/tests.rs` the file you append to

## Steps

1. Append this to the end of `src-tauri/crates/trigger-docker/src/supervisor/tests.rs` after a blank line. In the tests, `Script::Fail` is a spawn that errors, `Script::Lines(vec![])` is a child that exits at once with no output, and after the scripts run out the fake child stays alive until killed.

```rust

mod restart_tests {
    use super::super::fakes::*;
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn secs(delays: &Delays) -> Vec<u64> {
        delays.lock().unwrap().iter().map(|d| d.as_secs()).collect()
    }

    fn spawns(process: &FakeProcess) -> usize {
        process.spawns.load(Ordering::SeqCst)
    }

    #[test]
    fn a_child_that_exits_at_once_is_restarted_with_growing_waits() {
        let process = FakeProcess::new(
            Engine::Docker,
            vec![Script::Fail, Script::Lines(vec![]), Script::Lines(vec![])],
        );
        let (_seen, emit) = recorder();
        let (delays, pause) = recording_pause();
        let sup = Supervisor::start(process.clone(), vec![], emit, pause).unwrap();
        wait_until("three waits", || delays.lock().unwrap().len() == 3);
        wait_until("the fourth spawn", || spawns(&process) == 4);
        sup.stop();
        assert_eq!(secs(&delays), vec![1, 2, 4]);
        assert_eq!(spawns(&process), 4);
    }

    #[test]
    fn the_wait_stops_growing_at_sixty_seconds() {
        let scripts = (0..9).map(|_| Script::Fail).collect();
        let process = FakeProcess::new(Engine::Docker, scripts);
        let (_seen, emit) = recorder();
        let (delays, pause) = recording_pause();
        let sup = Supervisor::start(process, vec![], emit, pause).unwrap();
        wait_until("nine waits", || delays.lock().unwrap().len() == 9);
        sup.stop();
        assert_eq!(secs(&delays), vec![1, 2, 4, 8, 16, 32, 60, 60, 60]);
    }

    #[test]
    fn a_child_that_gave_lines_starts_the_waits_over() {
        let process = FakeProcess::new(
            Engine::Docker,
            vec![
                Script::Fail,
                Script::Fail,
                Script::Lines(vec![START]),
                Script::Fail,
            ],
        );
        let (_seen, emit) = recorder();
        let (delays, pause) = recording_pause();
        let sup = Supervisor::start(process, vec![], emit, pause).unwrap();
        wait_until("four waits", || delays.lock().unwrap().len() == 4);
        sup.stop();
        // Two failures, then a healthy run (wait 1, counter back to zero),
        // then one failure (wait 1 again).
        assert_eq!(secs(&delays), vec![1, 2, 1, 1]);
    }

    #[test]
    fn stop_cuts_a_long_wait_short() {
        let process = FakeProcess::new(Engine::Docker, vec![Script::Lines(vec![])]);
        let (_seen, emit) = recorder();
        let waiting = Arc::new(AtomicBool::new(false));
        let flag = waiting.clone();
        let pause: Pause = Arc::new(move |_delay, stop| {
            flag.store(true, Ordering::SeqCst);
            stop.wait(Duration::from_secs(60));
        });
        let sup = Supervisor::start(process.clone(), vec![], emit, pause).unwrap();
        wait_until("the wait to begin", || waiting.load(Ordering::SeqCst));
        let started = std::time::Instant::now();
        sup.stop();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(spawns(&process), 1);
    }
}
```

2. Run `cd src-tauri && cargo fmt -p trigger-docker`, then the acceptance commands. Run `cd src-tauri && cargo test -p trigger-docker supervisor` three times; it must pass every time.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker` passes: 12 tests in the library under `supervisor::tests` (8 before, 4 new `restart_tests`) on top of slice A's
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (it may print warnings for template text in `README.md`, `src/lib.rs` and `src/os.rs`; those stay until slice C)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/010-trigger-docker/tasks/`

## Out of scope

- Any change to `supervisor.rs` or `fakes.rs`.
- Real sleeping in a test. The pause closures never sleep for the backoff.
- Changing an expected delay to make a test pass.
- Kill tests. T14 does those.

## If stuck

- If a test fails with different delays, compare `run` in `supervisor.rs` with the card T12 character by character, and set status blocked quoting the numbers you got. Do not change the test or `run`.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
