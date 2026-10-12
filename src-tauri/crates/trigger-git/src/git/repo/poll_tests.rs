use super::*;
use crate::git::testutil::{git_available, TempRepo};

/// An origin with one commit and a clone of it.
fn pair() -> (TempRepo, TempRepo) {
    let origin = TempRepo::new();
    origin.commit("a.txt", "first");
    let clone = origin.clone_repo();
    (origin, clone)
}

#[test]
fn behind_counts_new_remote_commits_only_after_a_fetch() {
    if !git_available() {
        return;
    }
    let (origin, clone) = pair();
    let repo = GitRepo::new(&clone.path);
    assert_eq!(repo.read_behind(false), Some(0));
    origin.commit("a.txt", "second");
    origin.commit("a.txt", "third");
    assert_eq!(repo.read_behind(false), Some(0));
    assert_eq!(repo.read_behind(true), Some(2));
    assert_eq!(repo.read_behind(false), Some(2));
    clone.git(&["merge", "--ff-only", "-q", "@{u}"]);
    assert_eq!(repo.read_behind(false), Some(0));
}

#[test]
fn behind_is_none_without_an_upstream_or_on_a_detached_head() {
    if !git_available() {
        return;
    }
    let (origin, clone) = pair();
    assert_eq!(GitRepo::new(&origin.path).read_behind(false), None);
    clone.git(&["checkout", "-q", "--detach"]);
    assert_eq!(GitRepo::new(&clone.path).read_behind(false), None);
}

#[test]
fn a_fetch_that_fails_still_counts_from_the_refs_on_disk() {
    if !git_available() {
        return;
    }
    let (origin, clone) = pair();
    let repo = GitRepo::new(&clone.path);
    origin.commit("a.txt", "second");
    assert_eq!(repo.read_behind(true), Some(1));
    drop(origin);
    assert_eq!(repo.read_behind(true), Some(1));
}

#[test]
fn a_fetch_changes_nothing_in_the_snapshot() {
    if !git_available() {
        return;
    }
    let (origin, clone) = pair();
    let repo = GitRepo::new(&clone.path);
    let before = repo.read_snapshot().unwrap();
    origin.commit("a.txt", "second");
    origin.git(&["tag", "v1"]);
    origin.git(&["branch", "side"]);
    assert_eq!(repo.read_behind(true), Some(1));
    assert_eq!(repo.read_snapshot().unwrap(), before);
}
