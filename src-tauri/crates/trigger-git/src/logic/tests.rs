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
