use super::polled::*;
use super::*;

/// The payload field names `KINDS` lists for a kind, sorted.
fn listed(kind: &str) -> Vec<&'static str> {
    let info = crate::KINDS.iter().find(|k| k.id == kind).unwrap();
    let mut names: Vec<&'static str> = info.payload.iter().map(|f| f.name).collect();
    names.sort_unstable();
    names
}

fn keys(payload: &Payload) -> Vec<&str> {
    payload.keys().map(String::as_str).collect()
}

#[test]
fn behind_never_fires_on_the_first_count() {
    let mut watch = BehindWatch::new();
    assert_eq!(watch.observe(Some(3)), None);
    let mut zero = BehindWatch::new();
    assert_eq!(zero.observe(Some(0)), None);
}

#[test]
fn behind_fires_when_the_count_leaves_zero() {
    let mut watch = BehindWatch::new();
    assert_eq!(watch.observe(Some(0)), None);
    assert_eq!(watch.observe(Some(0)), None);
    assert_eq!(watch.observe(Some(2)), Some(2));
}

#[test]
fn behind_fires_once_while_the_count_stays_above_zero() {
    let mut watch = BehindWatch::new();
    watch.observe(Some(0));
    assert_eq!(watch.observe(Some(2)), Some(2));
    assert_eq!(watch.observe(Some(2)), None);
    assert_eq!(watch.observe(Some(5)), None);
    assert_eq!(watch.observe(Some(1)), None);
}

#[test]
fn behind_fires_again_after_a_pull_brings_it_back_to_zero() {
    let mut watch = BehindWatch::new();
    watch.observe(Some(0));
    assert_eq!(watch.observe(Some(2)), Some(2));
    assert_eq!(watch.observe(Some(0)), None);
    assert_eq!(watch.observe(Some(1)), Some(1));
}

#[test]
fn behind_ignores_polls_with_no_answer() {
    let mut watch = BehindWatch::new();
    assert_eq!(watch.observe(None), None);
    watch.observe(Some(0));
    assert_eq!(watch.observe(None), None);
    // The 0 is still remembered, so the next real count fires.
    assert_eq!(watch.observe(Some(4)), Some(4));
    assert_eq!(watch.observe(None), None);
    assert_eq!(watch.observe(Some(4)), None);
}

#[test]
fn behind_payload_has_exactly_the_listed_fields() {
    let payload = behind_payload("/work/app", Some("main"), 3);
    assert_eq!(keys(&payload), listed("git.behind-remote"));
    assert_eq!(payload["repo"], "/work/app");
    assert_eq!(payload["branch"], "main");
    assert_eq!(payload["behind"], 3);
    assert_eq!(behind_payload("/r", None, 1)["branch"], "");
}

#[test]
fn uncommitted_waits_for_the_limit() {
    let mut watch = UncommittedWatch::new();
    assert_eq!(watch.observe(Some(59 * 60), 60), None);
    assert_eq!(watch.observe(Some(60 * 60 - 1), 60), None);
    assert_eq!(watch.observe(Some(60 * 60), 60), Some(60));
}

#[test]
fn uncommitted_reports_whole_minutes() {
    let mut watch = UncommittedWatch::new();
    assert_eq!(watch.observe(Some(95 * 60 + 59), 60), Some(95));
}

#[test]
fn uncommitted_fires_once_until_the_tree_is_clean() {
    let mut watch = UncommittedWatch::new();
    assert_eq!(watch.observe(Some(7200), 60), Some(120));
    assert_eq!(watch.observe(Some(7500), 60), None);
    // Part of the work is committed: the rest is younger, still no new event.
    assert_eq!(watch.observe(Some(300), 60), None);
    assert_eq!(watch.observe(Some(7800), 60), None);
    // Clean tree, then old work again.
    assert_eq!(watch.observe(None, 60), None);
    assert_eq!(watch.observe(Some(300), 60), None);
    assert_eq!(watch.observe(Some(3600), 60), Some(60));
}

#[test]
fn uncommitted_work_that_is_already_old_fires_on_the_first_poll() {
    let mut watch = UncommittedWatch::new();
    assert_eq!(watch.observe(Some(10_000), 60), Some(166));
}

#[test]
fn uncommitted_with_a_zero_limit_fires_on_any_change() {
    let mut watch = UncommittedWatch::new();
    assert_eq!(watch.observe(Some(0), 0), Some(0));
    // A huge limit cannot overflow.
    let mut big = UncommittedWatch::new();
    assert_eq!(big.observe(Some(u64::MAX - 1), u64::MAX), None);
}

#[test]
fn uncommitted_payload_has_exactly_the_listed_fields() {
    let payload = uncommitted_payload("/work/app", Some("feat/x"), 75);
    assert_eq!(keys(&payload), listed("git.uncommitted-older-than"));
    assert_eq!(payload["repo"], "/work/app");
    assert_eq!(payload["branch"], "feat/x");
    assert_eq!(payload["minutes"], 75);
    assert_eq!(uncommitted_payload("/r", None, 1)["branch"], "");
}
