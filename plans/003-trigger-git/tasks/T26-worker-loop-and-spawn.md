---
id: T26
epic: 003-trigger-git
title: Add the trigger-git thread loop
status: done
depends_on: [T25]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/worker.rs
  - src-tauri/crates/trigger-git/src/worker/tests.rs
---

## Goal

`worker.rs` has the loop of the `trigger-git` thread: `run` sleeps until the earliest deadline or the next message, lets every slot do what is due, and sends the events. `spawn` starts it on a thread named `trigger-git`. It stops on `Msg::Stop`, when every sender is gone, or when the sink is closed, within a second. Tested with fake repositories and real channels.

## Read first

- `plans/003-trigger-git/epic.md` Decision 6 and the glue description in the Shape section
- `plans/_context/conventions.md` the Rust section (named threads that stop when their handle is dropped)
- `src-tauri/crates/trigger-fs/src/lib.rs:150-200` the `run` loop of `trigger-fs`, which this follows
- `src-tauri/crates/trigger-git/src/worker.rs` the file you append to
- `src-tauri/crates/trigger-git/src/worker/repo.rs` `RepoSlot::touch`, `deadline`, `tick`

## Steps

1. Check that `src-tauri/crates/trigger-git/src/worker/tests.rs` does not exist.

2. Append this to the end of `src-tauri/crates/trigger-git/src/worker.rs` (it starts with a blank line). The `keep` argument holds the file watchers so they live exactly as long as the loop. A watcher callback sends `Msg::Changed(index)`; `Msg::Stop` is sent when the handle is dropped (T27).

```rust

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use trigger_core::Sink;

/// What the thread is told.
pub enum Msg {
    /// A watched file changed in the repository slot with this index.
    Changed(usize),
    /// The handle was dropped.
    Stop,
}

/// The longest the thread sleeps without looking at its channel, so a stop
/// is noticed within this time even if its message were lost.
const IDLE: Duration = Duration::from_millis(500);

/// The loop of the `trigger-git` thread. Sleeps until the earliest deadline
/// or the next message, then lets every slot do what is due and sends the
/// events. Returns when told to stop, when the sender side is gone, or when
/// the sink is closed. `keep` holds the file watchers; they live as long as
/// the loop and stop when it returns.
pub fn run(
    mut repos: Vec<RepoSlot>,
    mut folders: Vec<FolderSlot>,
    keep: Vec<Box<dyn Send>>,
    rx: Receiver<Msg>,
    sink: Sink,
) {
    let _keep = keep;
    loop {
        let now = Instant::now();
        let wake = repos
            .iter()
            .filter_map(|r| r.deadline())
            .chain(folders.iter().map(|f| f.deadline()))
            .min();
        let wait = wake
            .map(|at| at.saturating_duration_since(now))
            .unwrap_or(IDLE)
            .min(IDLE);
        match rx.recv_timeout(wait) {
            Ok(Msg::Changed(index)) => {
                if let Some(repo) = repos.get_mut(index) {
                    repo.touch(Instant::now());
                }
            }
            Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {}
        }

        let now = Instant::now();
        let from_repos = repos.iter_mut().flat_map(|r| r.tick(now));
        let events: Vec<_> = from_repos
            .chain(folders.iter_mut().flat_map(|f| f.tick(now)))
            .collect();
        for event in events {
            if sink.send(event).is_err() {
                return;
            }
        }
    }
}

/// Starts the `trigger-git` thread.
pub fn spawn(
    repos: Vec<RepoSlot>,
    folders: Vec<FolderSlot>,
    keep: Vec<Box<dyn Send>>,
    rx: Receiver<Msg>,
    sink: Sink,
) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("trigger-git".into())
        .spawn(move || run(repos, folders, keep, rx, sink))
        .map(|_| ())
}

#[cfg(test)]
mod tests;
```

3. Create `src-tauri/crates/trigger-git/src/worker/tests.rs` with exactly this content.

```rust
use std::sync::mpsc::{channel, Sender};
use std::thread::JoinHandle;

use serde_json::json;
use trigger_core::testkit::rule;
use trigger_core::Event;

use super::*;
use crate::logic::fake::FakeRepo;
use crate::logic::RepoSnapshot;

const CEILING: Duration = Duration::from_secs(5);

fn snap(head: &str) -> RepoSnapshot {
    RepoSnapshot {
        head: Some(head.into()),
        branch: Some("main".into()),
        short: Some(head.into()),
        message: Some("a message".into()),
        reflog_subject: Some("commit: a message".into()),
        ..RepoSnapshot::default()
    }
}

fn entry(kind: &str, extra: serde_json::Value) -> Entry {
    let mut when = json!({ "repo": std::env::temp_dir() });
    when.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let parsed = crate::rule::parse(&rule("p/t", kind, when)).unwrap();
    Entry::new("p/t".into(), kind.into(), parsed, Instant::now())
}

/// A running loop over one repository slot.
struct Running {
    fake: FakeRepo,
    messages: Sender<Msg>,
    events: Receiver<Event>,
    thread: JoinHandle<()>,
}

fn start(entries: Vec<Entry>) -> Running {
    start_with(entries, |_| {})
}

/// Like `start`, with `setup` run on the fake before the thread begins, so a
/// polled rule's very first answer is the one the test wants.
fn start_with(entries: Vec<Entry>, setup: impl FnOnce(&FakeRepo)) -> Running {
    let fake = FakeRepo::new(snap("aaa"));
    setup(&fake);
    let slot = RepoSlot::new(
        "/work/app".into(),
        Box::new(fake.clone()),
        Box::new(|| false),
        entries,
    );
    let (messages, rx) = channel();
    let (sink, events) = channel();
    let thread = std::thread::spawn(move || run(vec![slot], Vec::new(), Vec::new(), rx, sink));
    Running {
        fake,
        messages,
        events,
        thread,
    }
}

#[test]
fn a_file_event_leads_to_one_event_after_the_settle_time() {
    let run = start(vec![entry("git.commit", json!({}))]);
    run.fake.set_snapshot(Ok(snap("bbb")));
    // A commit is a burst of file events.
    for _ in 0..5 {
        run.messages.send(Msg::Changed(0)).unwrap();
    }
    let event = run.events.recv_timeout(CEILING).expect("no event");
    assert_eq!(event.kind, "git.commit");
    assert_eq!(event.payload["commit"], json!("bbb"));
    // One baseline snapshot and one after the burst.
    assert_eq!(run.fake.snapshot_calls(), 2);
    assert!(run.events.recv_timeout(Duration::from_millis(800)).is_err());

    run.messages.send(Msg::Stop).unwrap();
    run.thread.join().unwrap();
}

#[test]
fn a_polled_rule_fires_without_any_file_event() {
    // The poll is due as soon as the loop starts.
    let run = start_with(
        vec![entry(
            "git.uncommitted-older-than",
            json!({ "older_than_min": 10 }),
        )],
        |fake| fake.set_age(Some(45 * 60)),
    );
    let event = run.events.recv_timeout(CEILING).expect("no event");
    assert_eq!(event.kind, "git.uncommitted-older-than");
    assert_eq!(event.payload["minutes"], json!(45));

    run.messages.send(Msg::Stop).unwrap();
    run.thread.join().unwrap();
}

#[test]
fn stop_ends_the_loop_within_a_second_and_closes_the_sink() {
    let run = start(vec![entry("git.commit", json!({}))]);
    let started = Instant::now();
    run.messages.send(Msg::Stop).unwrap();
    run.thread.join().unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(matches!(
        run.events.recv_timeout(Duration::from_millis(100)),
        Err(RecvTimeoutError::Disconnected)
    ));
}

#[test]
fn the_loop_ends_when_every_sender_is_gone() {
    let run = start(vec![entry("git.commit", json!({}))]);
    drop(run.messages);
    run.thread.join().unwrap();
}

#[test]
fn the_loop_ends_when_the_sink_is_closed() {
    let run = start(vec![entry("git.commit", json!({}))]);
    drop(run.events);
    run.fake.set_snapshot(Ok(snap("bbb")));
    run.messages.send(Msg::Changed(0)).unwrap();
    run.thread.join().unwrap();
}

#[test]
fn a_message_for_an_unknown_slot_is_ignored() {
    let run = start(vec![entry("git.commit", json!({}))]);
    run.messages.send(Msg::Changed(7)).unwrap();
    run.messages.send(Msg::Stop).unwrap();
    run.thread.join().unwrap();
}

#[test]
fn spawn_names_the_thread() {
    let (tx, rx) = channel();
    let (sink, _events) = channel();
    spawn(Vec::new(), Vec::new(), Vec::new(), rx, sink).unwrap();
    tx.send(Msg::Stop).unwrap();
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-git`.

Note on `check-crates.mjs`: it prints warnings, not errors. A warning about `src/logic/tests.rs` being long is expected and stays. Only errors fail the check.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git worker::tests` runs 7 tests and they pass (about 1 second)
- [ ] `cd src-tauri && cargo test -p trigger-git worker::` passes with 26 tests
- [ ] `cd src-tauri && cargo test -p trigger-git` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- Building slots from rules, resolving git folders or starting watchers (T27).
- Making the sleep longer than `IDLE` (500 ms) or shorter than the deadline.
- Joining the thread from the handle. A fetch can run for 30 seconds and must not block the caller.
- Sending events from anywhere but this loop.
- README changes (T27).

## If stuck

- If the file does not end with the `pub use repo::{Entry, RepoSlot};` line (followed only by blank lines) before you append, T25 was built differently from its card. Set status blocked.
- If `a_file_event_leads_to_one_event_after_the_settle_time` fails once, run it again alone. If it fails again set status blocked and quote the output. Do not change the numbers in it.
- If the tests hang for more than 30 seconds, stop them, set status blocked, and say which test was running.
- If clippy reports `unused_imports` for `FolderSlot`, `Entry` or `RepoSlot`, the `#[allow(unused_imports)]` lines in `worker.rs` were lost. Put them back.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
