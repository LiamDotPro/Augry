use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::json;
use trigger_core::testkit::rule;
use trigger_core::{RuleError, Source, SourceError};

use super::*;
use crate::git::testutil::{git_available, scratch_path, TempRepo};

fn commit_rule(repo: &std::path::Path) -> Rule {
    rule("p/t", "git.commit", json!({ "repo": repo }))
}

#[test]
fn the_source_describes_itself() {
    assert_eq!(GitSource.id(), "git");
    assert_eq!(GitSource.kinds().len(), 11);
    if git_available() {
        assert_eq!(GitSource.availability(), Availability::Available);
    }
}

#[test]
fn check_names_the_bad_field() {
    let bad = rule("p/t", "git.commit", json!({ "repo": "relative/dir" }));
    match GitSource.check(&bad) {
        Err(RuleError::Field { field, .. }) => assert_eq!(field, "repo"),
        other => panic!("expected a repo error, got {other:?}"),
    }
}

#[test]
fn start_fails_for_a_bad_rule_and_names_it() {
    let (tx, _rx) = channel();
    let bad = rule("p/bad", "git.commit", json!({}));
    match GitSource.start(vec![bad], tx) {
        Err(SourceError::Os(why)) => assert!(why.starts_with("p/bad: "), "{why}"),
        other => panic!("expected an error, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn start_fails_for_a_folder_that_is_not_a_repository() {
    if !git_available() {
        return;
    }
    let dir = scratch_path("not-a-repo");
    std::fs::create_dir_all(&dir).unwrap();
    let (tx, _rx) = channel();
    let result = GitSource.start(vec![commit_rule(&dir)], tx);
    let _ = std::fs::remove_dir_all(&dir);
    match result {
        Err(SourceError::Os(why)) => assert!(why.contains("not a git repository"), "{why}"),
        other => panic!("expected an error, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn start_fails_for_a_folder_that_does_not_exist() {
    let (tx, _rx) = channel();
    let result = GitSource.start(vec![commit_rule(&scratch_path("missing"))], tx);
    assert!(matches!(result, Err(SourceError::Os(_))));
}

#[test]
fn start_with_no_rules_does_nothing_and_stops_cleanly() {
    let (tx, rx) = channel();
    let handle = GitSource.start(Vec::new(), tx).unwrap();
    drop(handle);
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(1)),
        Err(RecvTimeoutError::Disconnected)
    ));
}

#[test]
fn dropping_the_handle_stops_the_thread_within_a_second() {
    if !git_available() {
        return;
    }
    let repo = TempRepo::new();
    repo.commit("a.txt", "first");
    let (tx, rx) = channel();
    let other = rule("p/o", "git.tag-created", json!({ "repo": repo.path() }));
    let handle = GitSource
        .start(vec![commit_rule(repo.path()), other], tx)
        .unwrap();
    let started = Instant::now();
    drop(handle);
    // The thread owns the only sender, so the channel closes when it ends.
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(2)),
        Err(RecvTimeoutError::Disconnected)
    ));
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn a_repo_cloned_rule_starts_and_stops_without_a_repository() {
    let folder = scratch_path("projects");
    std::fs::create_dir_all(&folder).unwrap();
    let (tx, rx) = channel();
    let cloned = rule("p/c", "git.repo-cloned", json!({ "folder": folder }));
    let handle = GitSource.start(vec![cloned], tx).unwrap();
    drop(handle);
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(2)),
        Err(RecvTimeoutError::Disconnected)
    ));
    let _ = std::fs::remove_dir_all(&folder);
}
