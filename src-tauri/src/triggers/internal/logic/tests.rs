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

fn ended(exit_code: i32, duration_ms: u64) -> InternalSignal {
    InternalSignal::CommandEnded {
        pane: "p1".into(),
        cwd: "/work/app".into(),
        exit_code,
        duration_ms,
    }
}

fn ids(fired: Vec<Fire>) -> Vec<String> {
    fired.into_iter().map(|f| f.0).collect()
}

#[test]
fn an_agent_glob_picks_the_agent_by_name() {
    let rules = [
        armed("a/any", "tg.agent-finished", json!({})),
        armed(
            "a/claude",
            "tg.agent-finished",
            json!({ "agent": "Claude" }),
        ),
        armed("a/codex", "tg.agent-finished", json!({ "agent": "codex" })),
    ];
    let signal = InternalSignal::AgentFinished {
        pane: "p1".into(),
        cwd: "/work/app".into(),
        agent: "claude".into(),
    };
    let fired = step(&rules, &signal);
    assert_eq!(fired[0].2["agent"], json!("claude"));
    assert_eq!(ids(fired), vec!["a/any", "a/claude"]);
}

#[test]
fn command_failed_without_a_code_fires_on_any_non_zero_exit() {
    let rules = [armed("a/failed", "tg.command-failed", json!({}))];
    assert!(step(&rules, &ended(0, 10)).is_empty());
    let fired = step(&rules, &ended(2, 1500));
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].2["exit_code"], json!(2));
    assert_eq!(fired[0].2["duration_ms"], json!(1500));
    assert_eq!(step(&rules, &ended(-1, 10)).len(), 1);
}

#[test]
fn command_failed_with_a_code_fires_on_that_code_only() {
    let rules = [armed(
        "a/failed",
        "tg.command-failed",
        json!({ "exit_code": 2 }),
    )];
    assert!(step(&rules, &ended(1, 10)).is_empty());
    assert_eq!(step(&rules, &ended(2, 10)).len(), 1);
    let zero = [armed(
        "a/failed",
        "tg.command-failed",
        json!({ "exit_code": 0 }),
    )];
    assert_eq!(step(&zero, &ended(0, 10)).len(), 1);
}

#[test]
fn command_slow_needs_more_than_the_limit() {
    let rules = [armed(
        "a/slow",
        "tg.command-slow",
        json!({ "longer_than_ms": 1000 }),
    )];
    assert!(step(&rules, &ended(0, 1000)).is_empty());
    assert_eq!(step(&rules, &ended(0, 1001)).len(), 1);
    assert_eq!(step(&rules, &ended(1, 5000)).len(), 1);
}

#[test]
fn command_slow_with_a_code_checks_the_code_too() {
    let rules = [armed(
        "a/slow",
        "tg.command-slow",
        json!({ "longer_than_ms": 1000, "exit_code": 0 }),
    )];
    assert!(step(&rules, &ended(1, 5000)).is_empty());
    assert_eq!(step(&rules, &ended(0, 5000)).len(), 1);
}

#[test]
fn one_command_end_can_fire_both_failed_and_slow() {
    let rules = [
        armed("a/failed", "tg.command-failed", json!({})),
        armed("a/slow", "tg.command-slow", json!({ "longer_than_ms": 10 })),
    ];
    assert_eq!(
        ids(step(&rules, &ended(1, 500))),
        vec!["a/failed", "a/slow"]
    );
}

fn line(text: &str) -> InternalSignal {
    InternalSignal::OutputLine {
        pane: "p1".into(),
        cwd: "/work/app".into(),
        line: text.into(),
    }
}

#[test]
fn output_fires_on_a_matching_line_ignoring_case() {
    let rules = [armed(
        "a/out",
        "tg.output-matched",
        json!({ "pattern": "error|panicked" }),
    )];
    assert!(step(&rules, &line("all good")).is_empty());
    let fired = step(&rules, &line("thread main PANICKED at src/lib.rs"));
    assert_eq!(fired.len(), 1);
    assert_eq!(
        fired[0].2["line"],
        json!("thread main PANICKED at src/lib.rs")
    );
}

#[test]
fn the_matched_line_is_cut_to_200_characters() {
    let rules = [armed(
        "a/out",
        "tg.output-matched",
        json!({ "pattern": "x" }),
    )];
    let long = "x".repeat(500);
    let fired = step(&rules, &line(&long));
    let sent = fired[0].2["line"].as_str().unwrap();
    assert_eq!(sent.chars().count(), MAX_LINE_CHARS);
    // Cutting counts characters, never bytes, so a multi byte line stays valid.
    let wide = "é".repeat(300);
    let fired = step(&rules, &line(&format!("x{wide}")));
    assert_eq!(fired[0].2["line"].as_str().unwrap().chars().count(), 200);
}

#[test]
fn a_note_glob_is_matched_against_the_relative_path() {
    let rules = [
        armed("a/any", "tg.note-saved", json!({})),
        armed("a/docs", "tg.note-saved", json!({ "glob": "docs/**/*.md" })),
    ];
    let note = |relative: &str| InternalSignal::NoteSaved {
        path: format!("/notes/{relative}"),
        relative: relative.into(),
    };
    assert_eq!(
        ids(step(&rules, &note("docs/a/b.md"))),
        vec!["a/any", "a/docs"]
    );
    assert_eq!(ids(step(&rules, &note("ideas.md"))), vec!["a/any"]);
}

#[test]
fn repo_filters_worktree_and_review_signals_by_their_repo() {
    let repo = std::env::temp_dir().join("tg-internal-wt");
    let other = std::env::temp_dir().join("tg-internal-other");
    let when = json!({ "repo": repo.to_string_lossy() });
    let rules = [
        armed("a/created", "tg.worktree-created", when.clone()),
        armed("a/removed", "tg.worktree-removed", when.clone()),
        armed("a/staged", "tg.review-staged", when),
    ];
    let created = |r: &std::path::Path| InternalSignal::WorktreeCreated {
        repo: r.to_string_lossy().into_owned(),
        path: "/x".into(),
        branch: None,
    };
    assert_eq!(ids(step(&rules, &created(&repo))), vec!["a/created"]);
    assert!(step(&rules, &created(&other)).is_empty());
    let staged = InternalSignal::ReviewStaged {
        repo: repo.to_string_lossy().into_owned(),
        files: 1,
    };
    assert_eq!(ids(step(&rules, &staged)), vec!["a/staged"]);
}

#[test]
fn repo_filters_a_pane_opened_by_its_repo_root() {
    let repo = std::env::temp_dir().join("tg-internal-open");
    let rules = [armed(
        "a/open",
        "tg.pane-opened-in-repo",
        json!({ "repo": repo.to_string_lossy() }),
    )];
    let opened = |root: &std::path::Path| InternalSignal::PaneOpened {
        pane: "p1".into(),
        cwd: root.join("src").to_string_lossy().into_owned(),
        repo_root: Some(root.to_string_lossy().into_owned()),
    };
    assert_eq!(step(&rules, &opened(&repo)).len(), 1);
    assert!(step(
        &rules,
        &opened(&std::env::temp_dir().join("tg-internal-open-two"))
    )
    .is_empty());
}
