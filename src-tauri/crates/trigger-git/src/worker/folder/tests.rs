use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::json;

use super::*;

thread_local! {
    /// What the fake listing returns. Each test runs on its own thread, so
    /// tests do not see each other's value.
    static LISTING: RefCell<BTreeSet<String>> = const { RefCell::new(BTreeSet::new()) };
}

fn list(_folder: &Path) -> BTreeSet<String> {
    LISTING.with(|l| l.borrow().clone())
}

fn set_listing(names: &[&str]) {
    LISTING.with(|l| *l.borrow_mut() = names.iter().map(|n| n.to_string()).collect());
}

fn slot(now: Instant) -> FolderSlot {
    FolderSlot::new("p/c".into(), PathBuf::from("/work"), 30, now, list)
}

#[test]
fn the_first_listing_is_a_baseline() {
    set_listing(&["app", "lib"]);
    let t0 = Instant::now();
    let mut slot = slot(t0);
    assert_eq!(slot.deadline(), t0);
    assert!(slot.tick(t0).is_empty());
    assert_eq!(slot.deadline(), t0 + Duration::from_secs(30));
}

#[test]
fn a_new_child_fires_with_its_path() {
    set_listing(&["app"]);
    let t0 = Instant::now();
    let mut slot = slot(t0);
    assert!(slot.tick(t0).is_empty());

    set_listing(&["app", "tool"]);
    let events = slot.tick(t0 + Duration::from_secs(30));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].rule, "p/c");
    assert_eq!(events[0].kind, "git.repo-cloned");
    let expected = PathBuf::from("/work").join("tool");
    assert_eq!(events[0].payload["path"], json!(expected.to_string_lossy()));
    assert_eq!(events[0].payload.len(), 1);

    // The same child is not reported twice.
    assert!(slot.tick(t0 + Duration::from_secs(60)).is_empty());
}

#[test]
fn nothing_is_listed_before_the_interval_has_passed() {
    set_listing(&[]);
    let t0 = Instant::now();
    let mut slot = slot(t0);
    assert!(slot.tick(t0).is_empty());
    set_listing(&["new"]);
    assert!(slot.tick(t0 + Duration::from_secs(29)).is_empty());
    assert_eq!(slot.tick(t0 + Duration::from_secs(30)).len(), 1);
}

#[test]
fn several_new_children_fire_in_name_order() {
    set_listing(&[]);
    let t0 = Instant::now();
    let mut slot = slot(t0);
    slot.tick(t0);
    set_listing(&["b", "a"]);
    let events = slot.tick(t0 + Duration::from_secs(30));
    let paths: Vec<String> = events
        .iter()
        .map(|e| e.payload["path"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        paths,
        [
            PathBuf::from("/work").join("a").to_string_lossy(),
            PathBuf::from("/work").join("b").to_string_lossy()
        ]
    );
}
