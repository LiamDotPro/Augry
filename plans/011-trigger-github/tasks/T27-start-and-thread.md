---
id: T27
epic: 011-trigger-github
title: Start the trigger-github thread from Source::start
status: done
depends_on: [T26]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-github/src/run.rs
  - src-tauri/crates/trigger-github/src/run/tests.rs
  - src-tauri/crates/trigger-github/src/lib.rs
  - src-tauri/crates/trigger-github/src/os.rs
  - src-tauri/crates/trigger-github/src/logic.rs
  - src-tauri/crates/trigger-github/README.md
---

## Goal

`GithubSource::start` works: it parses the rules, checks that `gh` exists and is signed in (`SourceError::Unavailable("Run gh auth login in a terminal")` when it is not, `SourceError::Os` when the question cannot be answered), and starts the thread `trigger-github`, which wakes every second, runs `Poller::tick` and sends what fires to the sink. Dropping the handle stops it within a second. Four tests run the real thread against the fake `gh` and a clock the test moves by hand, and check the start rules.

## Read first

- `plans/011-trigger-github/epic.md` Decisions 5, 7 and 8, and the sentence under "Shape": "Dropping the handle stops it within a second"
- `src-tauri/crates/trigger-fs/src/lib.rs` `start` and `run` (how the sibling crate builds its `Handle` and thread; the same shape, read only)
- `src-tauri/crates/trigger-github/src/logic/poll.rs` `Poller::new`, `tick`, `is_stopped`, `Outcome`, `Note` (T24 to T26)
- `src-tauri/crates/trigger-github/src/lib.rs` `start` at the bottom of the file
- `src-tauri/crates/trigger-github/src/os.rs` `find_gh`, `auth_ok`, `GhCli`, `NOT_FOUND` (T22, T23)

## Steps

1. Create `src-tauri/crates/trigger-github/src/run.rs` with exactly this.

How it behaves:

- `check_login` is the start rule as a pure function, so the three outcomes are tested with closures instead of a real `gh`. `gh` not found is `Unavailable(NOT_FOUND)`, no login is `Unavailable("Run gh auth login in a terminal")`, and a failed question (timeout, bad exit code) is `Os`.
- `spawn` builds the `Poller` with the start time (that is `since` for the comment lists), then runs the thread named `trigger-github`. The `Handle` only sets a flag, so dropping it never blocks.
- The thread ticks, prints each note as one line on stderr (`trigger-github: <rule ids>: <text>`; rule ids and plain words only), sends each fire as an `Event`, and sleeps in ten steps of 100 ms so a stop is seen within a tenth of a second. It also ends when the sink is closed or when the poller stopped itself (`gh` lost its login).
- The clock is a `Box<dyn Fn() -> u64 + Send>` so tests move time by hand.

```rust
//! The thread behind `GithubSource::start`: joins `gh` to the pure poller and
//! the sink. Glue only. What to ask, when, and what fires is decided in
//! `logic/poll.rs`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use trigger_core::{Event, Handle, Sink, SourceError};

use crate::logic::cli::Gh;
use crate::logic::poll::Poller;
use crate::logic::Armed;
use crate::os;

/// Shown on every rule when `gh` is installed but not signed in.
pub const NOT_SIGNED_IN: &str = "Run gh auth login in a terminal";

/// The time in whole seconds since the Unix epoch. A test passes its own.
pub type Clock = Box<dyn Fn() -> u64 + Send>;

/// Decides whether the source can start. `found` is where `gh` is and `auth`
/// asks it whether it is signed in. `gh` missing or not signed in is
/// `Unavailable` with the thing to do; a failed question is `Os`.
pub fn check_login(
    found: Option<&Path>,
    auth: &dyn Fn(&Path) -> Result<bool, String>,
) -> Result<PathBuf, SourceError> {
    let exe = found.ok_or_else(|| SourceError::Unavailable(os::NOT_FOUND.to_string()))?;
    match auth(exe) {
        Ok(true) => Ok(exe.to_path_buf()),
        Ok(false) => Err(SourceError::Unavailable(NOT_SIGNED_IN.to_string())),
        Err(why) => Err(SourceError::Os(why)),
    }
}

/// Starts the thread `trigger-github`. It asks `gh` about whatever is due
/// every second and sends what fires to `sink`. Dropping the handle stops it:
/// at the latest after the request in flight, and at once if it is waiting.
pub fn spawn(
    armed: Vec<Armed>,
    gh: Box<dyn Gh>,
    sink: Sink,
    clock: Clock,
) -> Result<Handle, SourceError> {
    let poller = Poller::new(armed, clock());
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    std::thread::Builder::new()
        .name("trigger-github".into())
        .spawn(move || run(poller, gh, sink, clock, &thread_stop))
        .map_err(|e| SourceError::Os(e.to_string()))?;
    Ok(Handle::new(move || stop.store(true, Ordering::Relaxed)))
}

fn run(mut poller: Poller, gh: Box<dyn Gh>, sink: Sink, clock: Clock, stop: &AtomicBool) {
    let cancelled = || stop.load(Ordering::Relaxed);
    while !cancelled() {
        let outcome = poller.tick(clock(), gh.as_ref(), &cancelled);
        for note in &outcome.notes {
            eprintln!("trigger-github: {}: {}", note.rules.join(", "), note.text);
        }
        for fire in outcome.fires {
            if cancelled() {
                return;
            }
            let event = Event {
                rule: fire.rule,
                kind: fire.kind,
                at: SystemTime::now(),
                payload: fire.payload,
            };
            if sink.send(event).is_err() {
                return;
            }
        }
        if poller.is_stopped() {
            return;
        }
        // Wake every second, but look at the stop flag ten times in it.
        for _ in 0..10 {
            if cancelled() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests;
```

2. Create `src-tauri/crates/trigger-github/src/run/tests.rs` with exactly this.

The tests give the thread a `Sender` that nothing else holds, so `recv_timeout` reports `Disconnected` exactly when the thread has ended. That is how they prove the stop without sleeping for a guessed time.

```rust
//! Tests for the thread and for the start check, with a fake `gh` and a clock
//! the test moves by hand.

use std::sync::atomic::AtomicU64;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::Instant;

use serde_json::json;

use super::*;
use crate::logic::diff::{kind, path_for, Group};
use crate::logic::fake_gh::{http, ok, FakeGh};
use crate::logic::testutil::{armed, only, ME, RELEASES};

const REPO: &str = "cli/cli";
/// 2026-10-12T00:00:00Z.
const START: u64 = 1_791_763_200;
const WAIT: Duration = Duration::from_secs(5);

fn releases() -> String {
    path_for(REPO, Group::Releases, ME, None, None)
}

fn release_rule() -> Vec<Armed> {
    vec![armed(
        "p/rel",
        kind::RELEASE_PUBLISHED,
        json!({ "repo": REPO, "interval_s": 60 }),
    )]
}

/// Starts the thread with a clock the test owns. The test keeps `rx` and the
/// clock; the thread's only `Sender` is the one given to `spawn`, so `rx`
/// reports `Disconnected` when the thread has ended.
fn start(gh: &FakeGh) -> (Handle, Receiver<Event>, Arc<AtomicU64>) {
    let now = Arc::new(AtomicU64::new(START));
    let clock_now = now.clone();
    let (tx, rx) = channel();
    let handle = spawn(
        release_rule(),
        Box::new(gh.clone()),
        tx,
        Box::new(move || clock_now.load(Ordering::Relaxed)),
    )
    .unwrap();
    (handle, rx, now)
}

fn wait_until(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn ended(rx: &Receiver<Event>) -> bool {
    loop {
        match rx.recv_timeout(WAIT) {
            Err(RecvTimeoutError::Disconnected) => return true,
            Err(RecvTimeoutError::Timeout) => return false,
            Ok(_) => {}
        }
    }
}

#[test]
fn the_thread_seeds_at_once_fires_when_the_clock_moves_and_stops_on_drop() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), ok(&only(RELEASES, &[399674700]), "W/\"seed\""));
    gh.serve(&releases(), ok(RELEASES, "W/\"news\""));
    let (handle, rx, now) = start(&gh);

    wait_until("the first request", || !gh.calls().is_empty());
    assert!(rx.try_recv().is_err(), "the first poll only seeds");

    now.store(START + 60, Ordering::Relaxed);
    let event = rx.recv_timeout(WAIT).expect("an event after the interval");
    assert_eq!(event.rule, "p/rel");
    assert_eq!(event.kind, kind::RELEASE_PUBLISHED);
    assert!(event.payload.contains_key("tag"));

    drop(handle);
    assert!(ended(&rx), "the thread ends after the handle is dropped");
    let calls = gh.calls().len();
    now.store(START + 10_000, Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(gh.calls().len(), calls, "nothing is asked after the stop");
}

#[test]
fn the_thread_ends_by_itself_when_gh_loses_its_login() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), http(401, &[], "{}"));
    let (handle, rx, _now) = start(&gh);
    assert!(ended(&rx), "a 401 stops the thread");
    drop(handle);
}

#[test]
fn the_thread_ends_when_nobody_is_listening() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), ok(&only(RELEASES, &[399674700]), "W/\"seed\""));
    gh.serve(&releases(), ok(RELEASES, "W/\"news\""));
    let (handle, rx, now) = start(&gh);
    wait_until("the first request", || !gh.calls().is_empty());
    drop(rx);
    now.store(START + 60, Ordering::Relaxed);
    wait_until("the second request", || gh.calls().len() >= 2);
    drop(handle);
}

#[test]
fn start_needs_gh_and_a_login() {
    let never = |_: &Path| -> Result<bool, String> { panic!("must not ask without gh") };
    match check_login(None, &never) {
        Err(SourceError::Unavailable(text)) => assert_eq!(text, os::NOT_FOUND),
        other => panic!("expected Unavailable, got {other:?}"),
    }
    let exe = Path::new("/somewhere/gh");
    match check_login(Some(exe), &|_| Ok(false)) {
        Err(SourceError::Unavailable(text)) => {
            assert_eq!(text, "Run gh auth login in a terminal");
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
    match check_login(Some(exe), &|_| {
        Err("gh auth status exited with code 1".into())
    }) {
        Err(SourceError::Os(text)) => assert!(text.contains("code 1")),
        other => panic!("expected Os, got {other:?}"),
    }
    assert_eq!(check_login(Some(exe), &|_| Ok(true)).unwrap(), exe);
}
```

3. In `src-tauri/crates/trigger-github/src/lib.rs`, make this edit.

Find this text, which appears once:

```rust
mod os;
mod rule;
```

Replace it with:

```rust
mod os;
mod rule;
mod run;
```

4. In `src-tauri/crates/trigger-github/src/lib.rs`, make this edit.

Find this text, which appears once:

```rust
        let _armed = rules
```

Replace it with:

```rust
        let armed = rules
```

5. In `src-tauri/crates/trigger-github/src/lib.rs`, make this edit. It replaces the end of `start`.

Find this text, which appears once:

```rust
        // The poll thread is added by slice C.
        let _ = sink;
        Err(SourceError::Unavailable("not implemented yet".into()))
```

Replace it with:

```rust
        if armed.is_empty() {
            return Ok(Handle::new(|| {}));
        }
        let exe = run::check_login(os::find_gh(), &os::auth_ok)?;
        run::spawn(
            armed,
            Box::new(os::GhCli::at(&exe)),
            sink,
            Box::new(os::now_s),
        )
```

6. In `src-tauri/crates/trigger-github/src/os.rs`, make this edit. It adds the real clock.

Find this text, which appears once:

```rust
use std::time::{Duration, Instant};
```

Replace it with:

```rust
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
```

7. In `src-tauri/crates/trigger-github/src/os.rs`, make this edit.

Find this text, which appears once:

```rust
/// How long one request may take before it counts as no answer.
```

Replace it with:

```rust
/// The time in whole seconds since the Unix epoch.
pub fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// How long one request may take before it counts as no answer.
```

8. In `src-tauri/crates/trigger-github/src/logic.rs`, make this edit. The thread tests in `run/tests.rs` use the fixtures.

Find this text, which appears once:

```rust
#[cfg(test)]
mod testutil;
```

Replace it with:

```rust
#[cfg(test)]
pub(crate) mod testutil;
```

9. In `src-tauri/crates/trigger-github/README.md`, make this edit. It fills the Permissions section. Do not touch any other section.

Find this text, which appears once:

```
What the OS asks the user for, and when. "None" is a fine answer.
```

Replace it with:

```
None. The crate asks the OS for nothing. It uses the login that `gh auth login`
already stored, and never reads, asks for or keeps a token. To check the login
it runs `gh auth status --json hosts` (never `gh auth token`, never plain
`gh auth status`, and what they print is never logged). Every request is
`gh api`.

When `gh` is missing, or has no login, every rule shows the reason in Settings:
"GitHub CLI not found. Install it from https://cli.github.com" or "Run gh auth
login in a terminal".
```

10. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib run::` passes with 4 tests
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (the lib reports 167 tests on macOS and Linux; `tests/source.rs` still passes, including `start_refuses_a_bad_rule_before_doing_anything`)
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 and prints no line with `trigger-github` (no template warnings are left)
- [ ] `grep -rn "not implemented yet" src-tauri/crates/trigger-github/src/lib.rs src-tauri/crates/trigger-github/src/os.rs src-tauri/crates/trigger-github/README.md` and `grep -n "Slice C\|slice C" src-tauri/crates/trigger-github/src/lib.rs src-tauri/crates/trigger-github/README.md` print nothing (the `os.rs`, `logic.rs` and `rule.rs` allow comments that mention slice C go in T29)
- [ ] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- Registering the source in the app (`sources()`, the Cargo feature): slice D.
- Telling the app when the thread ends because `gh` lost its login. `trigger-core` has no channel for it; the source is started again the next time the app applies the rules, and `start` then answers `Unavailable`.
- A log crate. `eprintln!` is what the app's own forward thread uses.
- Live tests (the next card) and the other README sections (the last card).
- Removing `#![allow(dead_code)]` from `logic.rs`, `os.rs` and `rule.rs`. The last card does that.

## If stuck

- If a find text in steps 3 to 9 is not in its file, an earlier card left the file different. Set status blocked and say which edit.
- If a thread test times out ("timed out waiting for ..."), do not raise the timeouts. Run it three more times; if it fails every time compare `run.rs` with step 1 and set status blocked if it still fails.
- If `tests/source.rs` fails in `start_refuses_a_bad_rule_before_doing_anything`, the `armed` parse in step 4 was changed. Compare `lib.rs` with the steps.
- If `check-crates` still prints a trigger-github warning, paste the line into the Log and set status blocked.
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: blocked. Steps 1 to 10 done and every acceptance check passes except the grep check: `grep -rn "not implemented yet\|Slice C\|slice C" src-tauri/crates/trigger-github/src/lib.rs src-tauri/crates/trigger-github/src/os.rs src-tauri/crates/trigger-github/README.md` prints `src-tauri/crates/trigger-github/src/os.rs:5:// Removed in the last card of slice C, when the poll loop uses everything here.` (line 5 of os.rs, a comment the card does not edit; the card's "Out of scope" keeps the allow(dead_code) removal for the last card). Not changed, since no step covers it.
- 2026-10-12 architect: the grep also hit os.rs's allow comment, which T29 removes; narrowed the check. The build stands; sent to the checker.
- 2026-10-12 checker: pass
