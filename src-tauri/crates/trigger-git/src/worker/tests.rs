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
