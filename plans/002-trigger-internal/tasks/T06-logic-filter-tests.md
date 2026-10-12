---
id: T06
epic: 002-trigger-internal
title: Test every filter of step
status: todo
depends_on: [T05]
size: M
model: haiku
files:
  - src-tauri/src/triggers/internal/logic/tests.rs
---

## Goal

`logic/tests.rs` covers the agent glob, both exit code filters, the duration limit, output patterns and the 200 character cut, the note glob, and `repo` on worktree, review and pane opened signals. The behaviour is already in `logic.rs`; this card pins it.

## Read first

- `src-tauri/src/triggers/internal/logic.rs` the code under test (T05)
- `src-tauri/src/triggers/internal/logic/tests.rs` the file you add to (T05)

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance (it was 7 when this card was written).

2. Open `src-tauri/src/triggers/internal/logic/tests.rs` and add the following at the end of the file, after the last test. Do not change the existing tests or the `use` lines at the top.

```rust

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
        armed("a/claude", "tg.agent-finished", json!({ "agent": "Claude" })),
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
    let rules = [armed("a/failed", "tg.command-failed", json!({ "exit_code": 2 }))];
    assert!(step(&rules, &ended(1, 10)).is_empty());
    assert_eq!(step(&rules, &ended(2, 10)).len(), 1);
    let zero = [armed("a/failed", "tg.command-failed", json!({ "exit_code": 0 }))];
    assert_eq!(step(&zero, &ended(0, 10)).len(), 1);
}

#[test]
fn command_slow_needs_more_than_the_limit() {
    let rules = [armed("a/slow", "tg.command-slow", json!({ "longer_than_ms": 1000 }))];
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
    assert_eq!(ids(step(&rules, &ended(1, 500))), vec!["a/failed", "a/slow"]);
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
    let rules = [armed("a/out", "tg.output-matched", json!({ "pattern": "error|panicked" }))];
    assert!(step(&rules, &line("all good")).is_empty());
    let fired = step(&rules, &line("thread main PANICKED at src/lib.rs"));
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].2["line"], json!("thread main PANICKED at src/lib.rs"));
}

#[test]
fn the_matched_line_is_cut_to_200_characters() {
    let rules = [armed("a/out", "tg.output-matched", json!({ "pattern": "x" }))];
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
    assert_eq!(ids(step(&rules, &note("docs/a/b.md"))), vec!["a/any", "a/docs"]);
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
    assert!(step(&rules, &opened(&std::env::temp_dir().join("tg-internal-open-two"))).is_empty());
}
```

3. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance. `cargo fmt` may rewrap the long lines you added; that is expected.

4. If a test you added fails, do not change `logic.rs` or the test. Follow "If stuck".

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::internal` passes and reports 35 passed
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Changing `logic.rs`, `rule.rs` or `mod.rs`.
- More tests than the ones given.
- Fixing existing clippy warnings, renaming anything, or changing a file not listed under `files:`.

## If stuck

- If one of the new tests fails, set `status: blocked`, and in the Log write the test name and the two sides of the failing assertion. A failing test means `logic.rs` differs from T05; the architect decides which is right.
- If `cargo fmt` changes the code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
