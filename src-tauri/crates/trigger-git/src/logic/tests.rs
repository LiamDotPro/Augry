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
