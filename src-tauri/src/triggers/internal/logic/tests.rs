use super::*;
use crate::triggers::internal::rule::parse;
use serde_json::json;
use trigger_core::testkit::rule;

fn armed(id: &str, kind: &str, when: serde_json::Value) -> Armed {
    Armed {
        id: id.into(),
        kind: kind.into(),
        parsed: parse(&rule(id, kind, when)).unwrap(),
    }
}

fn bell() -> InternalSignal {
    InternalSignal::Bell { pane: "p1".into() }
}

#[test]
fn nothing_armed_fires_nothing() {
    assert!(step(&[], &bell()).is_empty());
}

#[test]
fn a_bell_fires_a_bell_rule_with_the_pane() {
    let rules = [armed("a/bell", "tg.bell", json!({}))];
    let fired = step(&rules, &bell());
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].0, "a/bell");
    assert_eq!(fired[0].1, "tg.bell");
    assert_eq!(fired[0].2["pane"], json!("p1"));
    assert_eq!(fired[0].2.len(), 1);
}

#[test]
fn rules_of_other_kinds_stay_quiet() {
    let rules = [
        armed("a/bell", "tg.bell", json!({})),
        armed("a/start", "tg.app-started", json!({})),
    ];
    let fired = step(&rules, &InternalSignal::AppStarted);
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].0, "a/start");
    assert!(fired[0].2.is_empty());
}

#[test]
fn fired_rules_keep_the_order_they_were_armed_in() {
    let rules = [
        armed("b/bell", "tg.bell", json!({})),
        armed("a/bell", "tg.bell", json!({})),
    ];
    let ids: Vec<String> = step(&rules, &bell()).into_iter().map(|f| f.0).collect();
    assert_eq!(ids, vec!["b/bell", "a/bell"]);
}

#[test]
fn a_pane_opened_outside_a_repo_fires_nothing() {
    let rules = [armed("a/open", "tg.pane-opened-in-repo", json!({}))];
    let signal = InternalSignal::PaneOpened {
        pane: "p1".into(),
        cwd: "/tmp".into(),
        repo_root: None,
    };
    assert!(step(&rules, &signal).is_empty());
}

#[test]
fn a_pane_opened_in_a_repo_carries_the_repo_root() {
    let rules = [armed("a/open", "tg.pane-opened-in-repo", json!({}))];
    let signal = InternalSignal::PaneOpened {
        pane: "p1".into(),
        cwd: "/work/app/src".into(),
        repo_root: Some("/work/app".into()),
    };
    let fired = step(&rules, &signal);
    assert_eq!(fired[0].2["repo"], json!("/work/app"));
    assert_eq!(fired[0].2["cwd"], json!("/work/app/src"));
}

#[test]
fn a_worktree_branch_is_left_out_when_unknown() {
    let rules = [armed("a/wt", "tg.worktree-created", json!({}))];
    let signal = InternalSignal::WorktreeCreated {
        repo: "/work/app".into(),
        path: "/work/app-x".into(),
        branch: None,
    };
    let fired = step(&rules, &signal);
    assert!(!fired[0].2.contains_key("branch"));
    let with_branch = InternalSignal::WorktreeCreated {
        repo: "/work/app".into(),
        path: "/work/app-x".into(),
        branch: Some("x".into()),
    };
    assert_eq!(step(&rules, &with_branch)[0].2["branch"], json!("x"));
}

#[test]
fn review_staged_and_note_saved_payloads() {
    let rules = [
        armed("a/stage", "tg.review-staged", json!({})),
        armed("a/note", "tg.note-saved", json!({})),
    ];
    let staged = InternalSignal::ReviewStaged {
        repo: "/work/app".into(),
        files: 3,
    };
    assert_eq!(step(&rules, &staged)[0].2["files"], json!(3));
    let saved = InternalSignal::NoteSaved {
        path: "/notes/a/b.md".into(),
        relative: "a/b.md".into(),
    };
    let fired = step(&rules, &saved);
    assert_eq!(fired[0].2["relative"], json!("a/b.md"));
    assert_eq!(fired[0].2["path"], json!("/notes/a/b.md"));
}

#[test]
fn repo_matches_the_repository_and_folders_inside_it() {
    let repo = std::env::temp_dir().join("tg-internal-repo");
    let inside = repo.join("src");
    let beside = std::env::temp_dir().join("tg-internal-repo-two");
    let rules = [armed(
        "a/focus",
        "tg.focus-entered",
        json!({ "repo": repo.to_string_lossy() }),
    )];
    let focus = |cwd: &std::path::Path| InternalSignal::FocusEntered {
        pane: "p1".into(),
        cwd: cwd.to_string_lossy().into_owned(),
    };
    assert_eq!(step(&rules, &focus(&repo)).len(), 1);
    assert_eq!(step(&rules, &focus(&inside)).len(), 1);
    assert!(step(&rules, &focus(&beside)).is_empty());
}
