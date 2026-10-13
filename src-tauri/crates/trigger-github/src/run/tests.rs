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
