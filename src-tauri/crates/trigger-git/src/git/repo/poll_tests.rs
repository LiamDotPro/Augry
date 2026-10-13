use std::path::Path;
use std::time::Duration;

use super::*;
use crate::git::testutil::{git_available, TempRepo};

#[test]
fn changed_paths_reads_every_record_type() {
    let text = concat!(
        "1 .M N... 100644 100644 100644 aaa bbb a.txt\0",
        "1 M. N... 100644 100644 100644 aaa bbb dir/with space.txt\0",
        "2 R. N... 100644 100644 100644 aaa bbb R100 new name.txt\0old name.txt\0",
        "u UU N... 100644 100644 100644 100644 aaa bbb ccc conflict.txt\0",
    );
    assert_eq!(
        changed_paths(text),
        vec![
            "a.txt",
            "dir/with space.txt",
            "new name.txt",
            "conflict.txt"
        ]
    );
    assert!(changed_paths("").is_empty());
}

#[test]
fn age_secs_counts_back_and_never_goes_negative() {
    let now = SystemTime::now();
    assert_eq!(age_secs(now, now - Duration::from_secs(90)), 90);
    assert_eq!(age_secs(now, now + Duration::from_secs(90)), 0);
}

/// Sets a file's modified time to `secs` seconds ago.
fn age_file(path: &Path, secs: u64) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() - Duration::from_secs(secs))
        .unwrap();
}

#[test]
fn a_clean_tree_or_only_untracked_files_has_no_oldest_change() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    temp.commit("a.txt", "first");
    let repo = GitRepo::new(&temp.path);
    assert_eq!(repo.read_oldest_change_secs(), None);
    temp.write("new.txt", "untracked\n");
    assert_eq!(repo.read_oldest_change_secs(), None);
}

#[test]
fn the_oldest_modified_tracked_file_sets_the_age() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    temp.commit("a.txt", "first");
    temp.commit("b.txt", "second");
    temp.write("a.txt", "changed a, a longer line\n");
    temp.write("b.txt", "changed b, an even longer line than a\n");
    age_file(&temp.path.join("a.txt"), 7200);
    age_file(&temp.path.join("b.txt"), 600);
    let age = GitRepo::new(&temp.path).read_oldest_change_secs().unwrap();
    assert!((7200..7260).contains(&age), "{age}");
}

#[test]
fn a_deleted_tracked_file_is_not_counted() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    temp.commit("a.txt", "first");
    std::fs::remove_file(temp.path.join("a.txt")).unwrap();
    assert_eq!(GitRepo::new(&temp.path).read_oldest_change_secs(), None);
}

#[test]
fn a_git_repo_answers_the_repo_trait() {
    if !git_available() {
        return;
    }
    let origin = TempRepo::new();
    origin.commit("a.txt", "first");
    let clone = origin.clone_repo();
    clone.write("a.txt", "edited, longer than before\n");
    age_file(&clone.path.join("a.txt"), 3600);
    let repo: Box<dyn Repo> = Box::new(GitRepo::new(&clone.path));
    assert_eq!(repo.snapshot().unwrap().branch.as_deref(), Some("main"));
    assert_eq!(repo.behind(false), Some(0));
    assert!(repo.oldest_change_secs().unwrap() >= 3600);
}

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
