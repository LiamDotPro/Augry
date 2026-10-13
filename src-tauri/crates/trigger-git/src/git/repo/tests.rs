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

#[test]
fn count_unmerged_counts_u_records_and_skips_rename_sources() {
    let text = concat!(
        "1 .M N... 100644 100644 100644 aaa bbb a.txt\0",
        "u UU N... 100644 100644 100644 100644 aaa bbb ccc b.txt\0",
        "2 R. N... 100644 100644 100644 aaa bbb R100 new.txt\0u old name\0",
        "u AA N... 000000 100644 100644 100644 aaa bbb ccc c.txt\0",
    );
    assert_eq!(count_unmerged(text), 2);
    assert_eq!(count_unmerged(""), 0);
}

/// Commits on `main` and on `other` that both change `a.txt`, left on `main`.
fn diverged(temp: &TempRepo) {
    temp.commit("a.txt", "base");
    temp.git(&["checkout", "-q", "-b", "other"]);
    temp.commit("a.txt", "change on other");
    temp.git(&["checkout", "-q", "main"]);
    temp.commit("a.txt", "change on main");
}

#[test]
fn a_quiet_repository_has_no_merge_or_rebase() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    temp.commit("a.txt", "first");
    let snap = GitRepo::new(&temp.path).read_snapshot().unwrap();
    assert_eq!(snap.merge_head, None);
    assert!(!snap.rebase);
    assert_eq!(snap.unmerged, 0);
}

#[test]
fn a_conflicted_merge_shows_merge_head_and_the_file_count() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    diverged(&temp);
    assert!(!temp.try_git(&["merge", "other"]).ok);
    let snap = GitRepo::new(&temp.path).read_snapshot().unwrap();
    let other = temp.git(&["rev-parse", "other"]);
    assert_eq!(snap.merge_head, Some(other));
    assert_eq!(snap.unmerged, 1);
    assert!(!snap.rebase);
    assert_eq!(snap.branch.as_deref(), Some("main"));
}

#[test]
fn a_rebase_that_stops_shows_rebase_and_a_detached_head() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    diverged(&temp);
    assert!(!temp.try_git(&["rebase", "other"]).ok);
    let snap = GitRepo::new(&temp.path).read_snapshot().unwrap();
    assert!(snap.rebase);
    assert_eq!(snap.branch, None);
    assert!(snap.head.is_some());
    assert_eq!(snap.merge_head, None);
    assert_eq!(snap.unmerged, 0);
}

#[test]
fn a_linked_worktree_finds_its_own_merge_state() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    diverged(&temp);
    let linked = scratch_path("linked");
    temp.git(&["worktree", "add", "-q", linked.to_str().unwrap(), "other"]);
    let wt = GitRepo::new(&linked);
    assert_eq!(wt.read_snapshot().unwrap().merge_head, None);
    let merge = run(&linked, &["merge", "main"], TIMEOUT).unwrap();
    assert!(!merge.ok);
    let snap = wt.read_snapshot().unwrap();
    assert!(snap.merge_head.is_some());
    assert_eq!(snap.unmerged, 1);
    // The main work tree is untouched.
    assert_eq!(
        GitRepo::new(&temp.path).read_snapshot().unwrap().merge_head,
        None
    );
    std::fs::remove_dir_all(&linked).unwrap();
}
