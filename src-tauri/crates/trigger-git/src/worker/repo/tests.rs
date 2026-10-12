use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use trigger_core::testkit::rule;

use super::*;
use crate::logic::fake::FakeRepo;
use crate::logic::settle::SETTLE;

fn dir() -> String {
    std::env::temp_dir().to_string_lossy().into_owned()
}

/// A snapshot of `branch` whose HEAD is `head` (also used as the short sha),
/// reached by a commit.
fn snap(branch: &str, head: &str) -> RepoSnapshot {
    RepoSnapshot {
        head: Some(head.into()),
        branch: Some(branch.into()),
        short: Some(head.into()),
        message: Some("a message".into()),
        reflog_subject: Some("commit: a message".into()),
        ..RepoSnapshot::default()
    }
}

fn entry(id: &str, kind: &str, extra: serde_json::Value) -> Entry {
    let mut when = json!({ "repo": dir() });
    when.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let parsed = crate::rule::parse(&rule(id, kind, when)).unwrap();
    Entry::new(id.into(), kind.into(), parsed, Instant::now())
}

fn slot(fake: &FakeRepo, entries: Vec<Entry>) -> RepoSlot {
    RepoSlot::new(
        "/work/app".into(),
        Box::new(fake.clone()),
        Box::new(|| false),
        entries,
    )
}

/// Touches the slot at `t0` and ticks it once the settle time has passed.
fn settle_and_tick(slot: &mut RepoSlot, t0: Instant) -> Vec<Event> {
    slot.touch(t0);
    slot.tick(t0 + SETTLE)
}

#[test]
fn the_baseline_snapshot_fires_nothing_and_idle_ticks_do_nothing() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut slot = slot(&fake, vec![entry("p/c", "git.commit", json!({}))]);
    assert_eq!(fake.snapshot_calls(), 1);
    assert_eq!(slot.deadline(), None);
    assert!(slot
        .tick(Instant::now() + Duration::from_secs(60))
        .is_empty());
    assert_eq!(fake.snapshot_calls(), 1);
}

#[test]
fn a_commit_fires_after_the_settle_time_with_its_payload() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut slot = slot(&fake, vec![entry("p/c", "git.commit", json!({}))]);
    fake.set_snapshot(Ok(snap("main", "bbb")));
    let t0 = Instant::now();
    slot.touch(t0);
    assert_eq!(slot.deadline(), Some(t0 + SETTLE));
    assert!(slot.tick(t0 + Duration::from_millis(100)).is_empty());
    assert_eq!(fake.snapshot_calls(), 1);

    let events = slot.tick(t0 + SETTLE);
    assert_eq!(events.len(), 1);
    let e = &events[0];
    assert_eq!((e.rule.as_str(), e.kind.as_str()), ("p/c", "git.commit"));
    assert_eq!(e.payload["repo"], json!("/work/app"));
    assert_eq!(e.payload["branch"], json!("main"));
    assert_eq!(e.payload["commit"], json!("bbb"));
    assert_eq!(e.payload["message"], json!("a message"));
    assert_eq!(slot.deadline(), None);
}

#[test]
fn only_rules_of_the_kind_that_happened_fire() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut slot = slot(
        &fake,
        vec![
            entry("p/c", "git.commit", json!({})),
            entry("p/s", "git.branch-switched", json!({})),
            entry("p/d", "git.commit", json!({})),
        ],
    );
    fake.set_snapshot(Ok(snap("main", "bbb")));
    let events = settle_and_tick(&mut slot, Instant::now());
    let ids: Vec<&str> = events.iter().map(|e| e.rule.as_str()).collect();
    assert_eq!(ids, ["p/c", "p/d"]);
}

#[test]
fn the_branch_glob_filters_each_rule() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut slot = slot(
        &fake,
        vec![
            entry("p/rel", "git.commit", json!({ "branch": "release/*" })),
            entry("p/main", "git.commit", json!({ "branch": "main" })),
        ],
    );
    fake.set_snapshot(Ok(snap("main", "bbb")));
    let events = settle_and_tick(&mut slot, Instant::now());
    let ids: Vec<&str> = events.iter().map(|e| e.rule.as_str()).collect();
    assert_eq!(ids, ["p/main"]);
}

#[test]
fn a_switch_is_matched_on_the_new_branch_and_carries_from_and_to() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut slot = slot(
        &fake,
        vec![
            entry("p/old", "git.branch-switched", json!({ "branch": "main" })),
            entry(
                "p/new",
                "git.branch-switched",
                json!({ "branch": "feature/*" }),
            ),
        ],
    );
    fake.set_snapshot(Ok(snap("feature/x", "aaa")));
    let events = settle_and_tick(&mut slot, Instant::now());
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].rule, "p/new");
    assert_eq!(events[0].payload["from"], json!("main"));
    assert_eq!(events[0].payload["to"], json!("feature/x"));
}

#[test]
fn a_snapshot_that_changes_nothing_fires_nothing() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut slot = slot(&fake, vec![entry("p/c", "git.commit", json!({}))]);
    assert!(settle_and_tick(&mut slot, Instant::now()).is_empty());
    assert_eq!(fake.snapshot_calls(), 2);
}

#[test]
fn a_held_lock_delays_the_snapshot() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let locked = Arc::new(AtomicBool::new(true));
    let flag = locked.clone();
    let mut slot = RepoSlot::new(
        "/work/app".into(),
        Box::new(fake.clone()),
        Box::new(move || flag.load(Ordering::Relaxed)),
        vec![entry("p/c", "git.commit", json!({}))],
    );
    fake.set_snapshot(Ok(snap("main", "bbb")));
    let t0 = Instant::now();
    slot.touch(t0);
    assert!(slot.tick(t0 + SETTLE).is_empty());
    assert_eq!(fake.snapshot_calls(), 1);
    assert_eq!(slot.deadline(), Some(t0 + SETTLE * 2));

    locked.store(false, Ordering::Relaxed);
    let events = slot.tick(t0 + SETTLE * 2);
    assert_eq!(events.len(), 1);
    assert_eq!(fake.snapshot_calls(), 2);
}

#[test]
fn a_failed_snapshot_keeps_the_old_baseline() {
    let fake = FakeRepo::new(snap("main", "aaa"));
    let mut slot = slot(&fake, vec![entry("p/c", "git.commit", json!({}))]);
    fake.set_snapshot(Err("git timed out".into()));
    let t0 = Instant::now();
    assert!(settle_and_tick(&mut slot, t0).is_empty());

    fake.set_snapshot(Ok(snap("main", "bbb")));
    let events = settle_and_tick(&mut slot, t0 + Duration::from_secs(1));
    assert_eq!(events.len(), 1);
}

#[test]
fn with_no_baseline_the_first_good_snapshot_becomes_it() {
    let fake = FakeRepo::default();
    fake.set_snapshot(Err("not ready".into()));
    let mut slot = slot(&fake, vec![entry("p/c", "git.commit", json!({}))]);
    fake.set_snapshot(Ok(snap("main", "aaa")));
    let t0 = Instant::now();
    assert!(settle_and_tick(&mut slot, t0).is_empty());

    fake.set_snapshot(Ok(snap("main", "bbb")));
    let events = settle_and_tick(&mut slot, t0 + Duration::from_secs(1));
    assert_eq!(events.len(), 1);
}
