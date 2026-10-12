use super::*;
use crate::git::testutil::{git_available, scratch_path, TempRepo};

#[test]
fn parse_log_splits_short_sha_and_subject() {
    assert_eq!(
        parse_log("abc1234\0Fix the thing"),
        (Some("abc1234".into()), Some("Fix the thing".into()))
    );
    assert_eq!(parse_log("abc1234\0"), (Some("abc1234".into()), None));
    assert_eq!(parse_log(""), (None, None));
}

#[test]
fn parse_refs_reads_name_and_sha_per_line() {
    let text = "refs/heads/main\0aaa\nrefs/tags/v1\0bbb\nrefs/stash\0ccc\nnonsense\n";
    let refs = parse_refs(text);
    assert_eq!(refs.len(), 3);
    assert_eq!(refs["refs/heads/main"], "aaa");
    assert_eq!(refs["refs/tags/v1"], "bbb");
    assert_eq!(refs["refs/stash"], "ccc");
    assert!(parse_refs("").is_empty());
}

#[test]
fn a_repository_with_no_commits_has_a_branch_and_no_head() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    let snap = GitRepo::new(&temp.path).read_snapshot().unwrap();
    assert_eq!(snap.branch.as_deref(), Some("main"));
    assert_eq!(snap.head, None);
    assert_eq!(snap.short, None);
    assert_eq!(snap.message, None);
    assert_eq!(snap.reflog_subject, None);
    assert!(snap.refs.is_empty());
}

#[test]
fn a_commit_shows_in_head_message_reflog_and_refs() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    temp.commit("a.txt", "first commit");
    let snap = GitRepo::new(&temp.path).read_snapshot().unwrap();
    let head = snap.head.clone().unwrap();
    assert_eq!(head.len(), 40);
    assert_eq!(snap.branch.as_deref(), Some("main"));
    assert!(head.starts_with(snap.short.as_deref().unwrap()));
    assert_eq!(snap.message.as_deref(), Some("first commit"));
    assert!(snap.reflog_subject.unwrap().starts_with("commit"));
    assert_eq!(snap.refs["refs/heads/main"], head);
    assert_eq!(snap.refs.len(), 1);
}

#[test]
fn tags_the_stash_and_other_branches_are_in_refs() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    temp.commit("a.txt", "first");
    temp.git(&["tag", "v1"]);
    temp.git(&["branch", "other"]);
    temp.write("a.txt", "changed\n");
    temp.git(&["stash", "push", "-q"]);
    let refs = GitRepo::new(&temp.path).read_snapshot().unwrap().refs;
    let names: Vec<&str> = refs.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        vec![
            "refs/heads/main",
            "refs/heads/other",
            "refs/stash",
            "refs/tags/v1"
        ]
    );
}

#[test]
fn a_detached_head_has_a_head_and_no_branch() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    temp.commit("a.txt", "first");
    temp.git(&["checkout", "-q", "--detach"]);
    let snap = GitRepo::new(&temp.path).read_snapshot().unwrap();
    assert_eq!(snap.branch, None);
    assert!(snap.head.is_some());
}

#[test]
fn a_folder_that_is_not_a_repository_is_an_error() {
    if !git_available() {
        return;
    }
    let dir = scratch_path("plain");
    std::fs::create_dir_all(&dir).unwrap();
    let err = GitRepo::new(&dir).read_snapshot().unwrap_err();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(err.starts_with("not a git repository"), "{err}");

    let missing = GitRepo::new(scratch_path("missing")).read_snapshot();
    assert!(missing.is_err());
}
