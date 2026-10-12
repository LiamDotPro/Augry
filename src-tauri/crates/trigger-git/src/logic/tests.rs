use super::*;

#[test]
fn kind_ids_are_kinds_of_this_crate() {
    let changes = [
        Change::RebaseStopped {
            branch: None,
            commit: None,
        },
        Change::MergeConflict {
            branch: None,
            files: 1,
        },
        Change::BranchSwitched {
            from: "a".into(),
            to: "b".into(),
            commit: None,
        },
        Change::DetachedHead {
            from: "a".into(),
            commit: None,
        },
        Change::Pulled {
            branch: None,
            commit: None,
            message: None,
        },
        Change::Commit {
            branch: None,
            commit: None,
            message: None,
        },
        Change::TagCreated {
            branch: None,
            tag: "v1".into(),
        },
        Change::StashCreated { branch: None },
    ];
    let mut seen = Vec::new();
    for change in &changes {
        assert!(
            crate::KINDS.iter().any(|k| k.id == change.kind()),
            "{} is not in KINDS",
            change.kind()
        );
        assert!(!seen.contains(&change.kind()), "{} twice", change.kind());
        seen.push(change.kind());
    }
}

#[test]
fn branch_is_the_branch_a_glob_matches_against() {
    let switched = Change::BranchSwitched {
        from: "old".into(),
        to: "new".into(),
        commit: None,
    };
    assert_eq!(switched.branch(), Some("new"));
    let detached = Change::DetachedHead {
        from: "main".into(),
        commit: None,
    };
    assert_eq!(detached.branch(), Some("main"));
    let commit = Change::Commit {
        branch: Some("dev".into()),
        commit: None,
        message: None,
    };
    assert_eq!(commit.branch(), Some("dev"));
    let tag = Change::TagCreated {
        branch: None,
        tag: "v1".into(),
    };
    assert_eq!(tag.branch(), None);
}

fn keys(payload: &Payload) -> Vec<&str> {
    payload.keys().map(String::as_str).collect()
}

fn names(fields: &[trigger_core::FieldInfo]) -> Vec<&str> {
    let mut names: Vec<&str> = fields.iter().map(|f| f.name).collect();
    names.sort_unstable();
    names
}

#[test]
fn payloads_carry_exactly_the_fields_listed_in_kinds() {
    let changes = [
        Change::RebaseStopped {
            branch: Some("main".into()),
            commit: Some("abc1234".into()),
        },
        Change::MergeConflict {
            branch: Some("main".into()),
            files: 2,
        },
        Change::BranchSwitched {
            from: "a".into(),
            to: "b".into(),
            commit: Some("abc1234".into()),
        },
        Change::DetachedHead {
            from: "main".into(),
            commit: Some("abc1234".into()),
        },
        Change::Pulled {
            branch: Some("main".into()),
            commit: Some("abc1234".into()),
            message: Some("Merge x".into()),
        },
        Change::Commit {
            branch: Some("main".into()),
            commit: Some("abc1234".into()),
            message: Some("Fix y".into()),
        },
        Change::TagCreated {
            branch: Some("main".into()),
            tag: "v1".into(),
        },
        Change::StashCreated {
            branch: Some("main".into()),
        },
    ];
    for change in &changes {
        let info = crate::KINDS
            .iter()
            .find(|k| k.id == change.kind())
            .expect("kind exists");
        let payload = change.payload("/work/app");
        assert_eq!(keys(&payload), names(info.payload), "{}", change.kind());
    }
}

#[test]
fn payload_values() {
    let commit = Change::Commit {
        branch: Some("main".into()),
        commit: Some("abc1234".into()),
        message: Some("Fix y".into()),
    };
    let p = commit.payload("/work/app");
    assert_eq!(p["repo"], "/work/app");
    assert_eq!(p["branch"], "main");
    assert_eq!(p["commit"], "abc1234");
    assert_eq!(p["message"], "Fix y");

    let switched = Change::BranchSwitched {
        from: "a".into(),
        to: "b".into(),
        commit: None,
    };
    let p = switched.payload("/work/app");
    assert_eq!(p["branch"], "b");
    assert_eq!(p["from"], "a");
    assert_eq!(p["to"], "b");
    assert_eq!(p["commit"], "");

    let conflict = Change::MergeConflict {
        branch: None,
        files: 3,
    };
    let p = conflict.payload("/work/app");
    assert_eq!(p["files"], 3);
    assert_eq!(p["branch"], "");
}

/// A snapshot on `branch` at `head`, with a short sha made from the head.
fn on(branch: &str, head: &str) -> RepoSnapshot {
    RepoSnapshot {
        head: Some(head.into()),
        branch: Some(branch.into()),
        short: Some(head.chars().take(7).collect()),
        message: Some(format!("message of {head}")),
        ..RepoSnapshot::default()
    }
}

/// A snapshot with HEAD detached at `head`.
fn detached(head: &str) -> RepoSnapshot {
    RepoSnapshot {
        branch: None,
        ..on("x", head)
    }
}

fn kinds(changes: &[Change]) -> Vec<&'static str> {
    changes.iter().map(Change::kind).collect()
}

#[test]
fn the_first_snapshot_fires_nothing() {
    let mut first = on("main", "aaaaaaa1");
    first.merge_head = Some("bbbbbbb2".into());
    first.unmerged = 2;
    first.rebase = true;
    assert!(observe(None, &first).is_empty());
    assert_eq!(observe(Some(&on("main", "a")), &first).len(), 1);
}

#[test]
fn an_unchanged_repository_fires_nothing() {
    let snap = on("main", "aaaaaaa1");
    assert!(step(&snap, &snap.clone()).is_empty());
}

#[test]
fn a_merge_conflict_reports_the_unmerged_count() {
    let prev = on("main", "aaaaaaa1");
    let mut next = prev.clone();
    next.merge_head = Some("bbbbbbb2".into());
    next.unmerged = 3;
    assert_eq!(
        step(&prev, &next),
        vec![Change::MergeConflict {
            branch: Some("main".into()),
            files: 3
        }]
    );
}

#[test]
fn a_merge_without_conflicts_or_one_already_running_fires_nothing() {
    let prev = on("main", "aaaaaaa1");
    let mut no_conflict = prev.clone();
    no_conflict.merge_head = Some("bbbbbbb2".into());
    no_conflict.unmerged = 0;
    assert!(step(&prev, &no_conflict).is_empty());

    let mut running = prev.clone();
    running.merge_head = Some("bbbbbbb2".into());
    running.unmerged = 3;
    let mut still = running.clone();
    still.unmerged = 2;
    assert!(step(&running, &still).is_empty());
}

#[test]
fn a_rebase_stop_fires_once_and_names_the_branch_it_left() {
    let prev = on("feature", "aaaaaaa1");
    let mut next = detached("ccccccc3");
    next.rebase = true;
    assert_eq!(
        step(&prev, &next),
        vec![Change::RebaseStopped {
            branch: Some("feature".into()),
            commit: Some("ccccccc".into())
        }]
    );
}

#[test]
fn nothing_else_fires_while_a_rebase_runs() {
    let prev = on("feature", "aaaaaaa1");
    let mut stopped = detached("ccccccc3");
    stopped.rebase = true;
    // Still rebasing, HEAD moved, a tag appeared, a conflict file is listed.
    let mut later = detached("ddddddd4");
    later.rebase = true;
    later.merge_head = Some("eeeeeee5".into());
    later.unmerged = 1;
    later.refs.insert("refs/tags/v1".into(), "f".into());
    assert!(step(&stopped, &later).is_empty());
    // Even a first sight of a rebase with a tag only reports the stop.
    assert_eq!(kinds(&step(&prev, &later)), vec!["git.rebase-stopped"]);
}

/// `prev` on main at one commit, `next` on main at another with `subject` as
/// the newest reflog entry.
fn moved(subject: Option<&str>) -> (RepoSnapshot, RepoSnapshot) {
    let prev = on("main", "aaaaaaa1");
    let mut next = on("main", "bbbbbbb2");
    next.reflog_subject = subject.map(String::from);
    (prev, next)
}

#[test]
fn a_new_commit_fires_commit_with_its_message() {
    for subject in [
        "commit: add the thing",
        "commit (amend): add the thing",
        "commit (merge): Merge branch x",
        "commit (initial): first",
        "cherry-pick: add the thing",
    ] {
        let (prev, next) = moved(Some(subject));
        assert_eq!(
            step(&prev, &next),
            vec![Change::Commit {
                branch: Some("main".into()),
                commit: Some("bbbbbbb".into()),
                message: Some("message of bbbbbbb2".into())
            }],
            "{subject}"
        );
    }
}

#[test]
fn the_first_commit_on_an_unborn_branch_fires_commit() {
    let mut prev = on("main", "x");
    prev.head = None;
    prev.short = None;
    let (_, next) = moved(Some("commit (initial): first"));
    assert_eq!(kinds(&step(&prev, &next)), vec!["git.commit"]);
}

#[test]
fn a_pull_or_merge_fires_pulled() {
    for subject in [
        "pull: Fast-forward",
        "pull: Merge made by the 'ort' strategy.",
        "pull --rebase (finish): returning to refs/heads/main",
        "merge origin/main: Fast-forward",
    ] {
        let (prev, next) = moved(Some(subject));
        assert_eq!(kinds(&step(&prev, &next)), vec!["git.pulled"], "{subject}");
    }
}

#[test]
fn a_reset_or_a_finished_rebase_fires_nothing() {
    for subject in [
        Some("reset: moving to HEAD~1"),
        Some("rebase (finish): returning to refs/heads/main"),
        Some("checkout: moving from a to main"),
        Some(""),
        None,
    ] {
        let (prev, next) = moved(subject);
        assert!(step(&prev, &next).is_empty(), "{subject:?}");
    }
}

#[test]
fn a_commit_on_a_detached_head_fires_commit() {
    let prev = detached("aaaaaaa1");
    let mut next = detached("bbbbbbb2");
    next.reflog_subject = Some("commit: wip".into());
    assert_eq!(kinds(&step(&prev, &next)), vec!["git.commit"]);
}

#[test]
fn switching_branch_fires_only_the_switch() {
    let prev = on("main", "aaaaaaa1");
    // The new branch is at another commit and the reflog says checkout.
    let mut next = on("feature", "bbbbbbb2");
    next.reflog_subject = Some("checkout: moving from main to feature".into());
    assert_eq!(
        step(&prev, &next),
        vec![Change::BranchSwitched {
            from: "main".into(),
            to: "feature".into(),
            commit: Some("bbbbbbb".into())
        }]
    );
    // A subject starting with commit or pull cannot turn a switch into more.
    next.reflog_subject = Some("pull: Fast-forward".into());
    assert_eq!(kinds(&step(&prev, &next)), vec!["git.branch-switched"]);
}

#[test]
fn a_new_branch_made_at_the_same_commit_is_a_switch() {
    let prev = on("main", "aaaaaaa1");
    let next = on("topic", "aaaaaaa1");
    assert_eq!(kinds(&step(&prev, &next)), vec!["git.branch-switched"]);
}

#[test]
fn leaving_a_branch_for_a_commit_fires_detached_head() {
    let prev = on("main", "aaaaaaa1");
    let next = detached("bbbbbbb2");
    assert_eq!(
        step(&prev, &next),
        vec![Change::DetachedHead {
            from: "main".into(),
            commit: Some("bbbbbbb".into())
        }]
    );
}

#[test]
fn getting_back_onto_a_branch_from_a_detached_head_fires_nothing() {
    let prev = detached("aaaaaaa1");
    let next = on("main", "bbbbbbb2");
    assert!(step(&prev, &next).is_empty());
}

#[test]
fn a_detach_during_a_merge_is_not_a_detached_head() {
    let prev = on("main", "aaaaaaa1");
    let mut next = detached("aaaaaaa1");
    next.merge_head = Some("ccccccc3".into());
    assert!(step(&prev, &next).is_empty());
}

#[test]
fn a_conflict_and_a_switch_in_one_window_come_in_order() {
    let prev = on("main", "aaaaaaa1");
    let mut next = on("feature", "bbbbbbb2");
    next.merge_head = Some("ccccccc3".into());
    next.unmerged = 1;
    assert_eq!(
        kinds(&step(&prev, &next)),
        vec!["git.merge-conflict", "git.branch-switched"]
    );
}
