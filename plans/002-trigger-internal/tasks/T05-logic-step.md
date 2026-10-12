---
id: T05
epic: 002-trigger-internal
title: Add the pure step function
status: todo
depends_on: [T04]
size: M
model: haiku
files:
  - src-tauri/src/triggers/internal/logic.rs
  - src-tauri/src/triggers/internal/logic/tests.rs
  - src-tauri/src/triggers/internal/mod.rs
---

## Goal

`logic::step(&[Armed], &InternalSignal) -> Vec<Fire>` exists: given the armed rules and one signal, it returns which rules fire with their payloads. It is pure and tested. T06 adds the filter tests.

## Read first

- `plans/002-trigger-internal/epic.md` Decisions 3, 10, 11 and the `InternalSignal` block
- `src-tauri/crates/trigger-fs/src/logic.rs:1-60` the same split: pure logic with its tests in a `logic` folder beside it
- `src-tauri/src/triggers/internal/rule.rs` `Parsed` and `Filter` (T04)
- `src-tauri/src/triggers/internal/mod.rs` `InternalSignal` (T01)

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance (it was 7 when this card was written).

2. Create `src-tauri/src/triggers/internal/logic.rs` with exactly this content. How it works: `kinds_of` says which kinds a signal can fire (one command end can fire both `tg.command-failed` and `tg.command-slow`; a pane opened outside a repository fires nothing); `place_of` gives the path that a rule's `repo` is compared with (the pane's `cwd`, or the repository a worktree or review signal names, or a pane's `repo_root`); `repo_ok` accepts the repository itself and any folder inside it; `filter_ok` applies the kind's own filter; `payload_of` builds the payload, cutting `line` to 200 characters by counting characters, not bytes.

```rust
//! Which armed rules a signal fires, and with what payload. Pure: no threads,
//! no clock, no channels. `InternalSource::publish` sends what this returns.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::json;
use trigger_core::{RuleId, Value};

use super::rule::{Filter, Parsed};
use super::InternalSignal;

#[cfg(test)]
mod tests;

/// The longest `line` in a `tg.output-matched` payload, in characters.
pub const MAX_LINE_CHARS: usize = 200;

/// A rule that passed `rule::parse`, ready to be matched.
#[derive(Debug, Clone)]
pub struct Armed {
    pub id: RuleId,
    pub kind: String,
    pub parsed: Parsed,
}

/// One rule that fired: its id, its kind and the payload.
pub type Fire = (RuleId, String, BTreeMap<String, Value>);

/// The rules this signal fires, in the order of `armed`.
pub fn step(armed: &[Armed], signal: &InternalSignal) -> Vec<Fire> {
    let kinds = kinds_of(signal);
    // A pane that opened outside a repository has nothing to fire.
    if kinds.is_empty() {
        return Vec::new();
    }
    let place = place_of(signal);
    let payload = payload_of(signal);
    armed
        .iter()
        .filter(|a| kinds.contains(&a.kind.as_str()))
        .filter(|a| repo_ok(a.parsed.repo.as_deref(), place))
        .filter(|a| filter_ok(&a.parsed.filter, signal))
        .map(|a| (a.id.clone(), a.kind.clone(), payload.clone()))
        .collect()
}

/// The kinds a signal can fire. One command end can be both a failure and a
/// slow command.
fn kinds_of(signal: &InternalSignal) -> &'static [&'static str] {
    use InternalSignal::*;
    match signal {
        AgentFinished { .. } => &["tg.agent-finished"],
        CommandEnded { .. } => &["tg.command-failed", "tg.command-slow"],
        OutputLine { .. } => &["tg.output-matched"],
        Bell { .. } => &["tg.bell"],
        TaskReported { .. } => &["tg.task-reported"],
        PaneOpened {
            repo_root: Some(_), ..
        } => &["tg.pane-opened-in-repo"],
        PaneOpened {
            repo_root: None, ..
        } => &[],
        WorktreeCreated { .. } => &["tg.worktree-created"],
        WorktreeRemoved { .. } => &["tg.worktree-removed"],
        ReviewStaged { .. } => &["tg.review-staged"],
        NoteSaved { .. } => &["tg.note-saved"],
        FocusEntered { .. } => &["tg.focus-entered"],
        AppStarted => &["tg.app-started"],
    }
}

/// The path a rule's `repo` is compared with: the pane's directory, or the
/// repository the signal names. None for signals that have no place.
fn place_of(signal: &InternalSignal) -> Option<&str> {
    use InternalSignal::*;
    match signal {
        AgentFinished { cwd, .. }
        | CommandEnded { cwd, .. }
        | OutputLine { cwd, .. }
        | TaskReported { cwd, .. }
        | FocusEntered { cwd, .. } => Some(cwd),
        PaneOpened { repo_root, .. } => repo_root.as_deref(),
        WorktreeCreated { repo, .. } | WorktreeRemoved { repo, .. } | ReviewStaged { repo, .. } => {
            Some(repo)
        }
        Bell { .. } | NoteSaved { .. } | AppStarted => None,
    }
}

/// No `repo` on the rule matches everything. Otherwise the place must be the
/// repository or a folder inside it.
fn repo_ok(repo: Option<&Path>, place: Option<&str>) -> bool {
    match (repo, place) {
        (None, _) => true,
        (Some(repo), Some(place)) => Path::new(place).starts_with(repo),
        (Some(_), None) => false,
    }
}

fn filter_ok(filter: &Filter, signal: &InternalSignal) -> bool {
    use InternalSignal::*;
    match (filter, signal) {
        (Filter::None, _) => true,
        (Filter::Agent(glob), AgentFinished { agent, .. }) => {
            glob.as_ref().is_none_or(|g| g.is_match(agent))
        }
        (Filter::Failed { exit_code }, CommandEnded { exit_code: got, .. }) => match exit_code {
            Some(wanted) => i64::from(*got) == *wanted,
            None => *got != 0,
        },
        (
            Filter::Slow {
                longer_than_ms,
                exit_code,
            },
            CommandEnded {
                exit_code: got,
                duration_ms,
                ..
            },
        ) => {
            duration_ms > longer_than_ms && exit_code.is_none_or(|wanted| i64::from(*got) == wanted)
        }
        (Filter::Pattern(regex), OutputLine { line, .. }) => regex.is_match(line),
        (Filter::Note(glob), NoteSaved { relative, .. }) => {
            glob.as_ref().is_none_or(|g| g.is_match(relative))
        }
        _ => false,
    }
}

fn payload_of(signal: &InternalSignal) -> BTreeMap<String, Value> {
    use InternalSignal::*;
    let pairs: Vec<(&str, Value)> = match signal {
        AgentFinished { pane, cwd, agent } => {
            vec![
                ("pane", json!(pane)),
                ("cwd", json!(cwd)),
                ("agent", json!(agent)),
            ]
        }
        CommandEnded {
            pane,
            cwd,
            exit_code,
            duration_ms,
        } => vec![
            ("pane", json!(pane)),
            ("cwd", json!(cwd)),
            ("exit_code", json!(exit_code)),
            ("duration_ms", json!(duration_ms)),
        ],
        OutputLine { pane, cwd, line } => {
            let line: String = line.chars().take(MAX_LINE_CHARS).collect();
            vec![
                ("pane", json!(pane)),
                ("cwd", json!(cwd)),
                ("line", json!(line)),
            ]
        }
        Bell { pane } => vec![("pane", json!(pane))],
        TaskReported { pane, cwd } | FocusEntered { pane, cwd } => {
            vec![("pane", json!(pane)), ("cwd", json!(cwd))]
        }
        PaneOpened {
            pane,
            cwd,
            repo_root,
        } => {
            let mut pairs = vec![("pane", json!(pane)), ("cwd", json!(cwd))];
            if let Some(root) = repo_root {
                pairs.push(("repo", json!(root)));
            }
            pairs
        }
        WorktreeCreated { repo, path, branch } => {
            let mut pairs = vec![("repo", json!(repo)), ("path", json!(path))];
            if let Some(branch) = branch {
                pairs.push(("branch", json!(branch)));
            }
            pairs
        }
        WorktreeRemoved { repo, path } => vec![("repo", json!(repo)), ("path", json!(path))],
        ReviewStaged { repo, files } => vec![("repo", json!(repo)), ("files", json!(files))],
        NoteSaved { path, relative } => vec![("path", json!(path)), ("relative", json!(relative))],
        AppStarted => Vec::new(),
    };
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}
```

3. Create `src-tauri/src/triggers/internal/logic/tests.rs` with exactly this content:

```rust
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
```

4. In `src-tauri/src/triggers/internal/mod.rs` find the two lines `mod kinds;` and `pub mod rule;` and replace them with three lines, so the module list reads:

```rust
mod kinds;
pub mod logic;
pub mod rule;
```

5. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::internal` passes and reports 24 passed
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- `InternalSource`, sending events or a clock. `step` returns data only.
- Tests for the agent, exit code, duration, pattern and note filters. T06 adds them.
- Changing a payload field name. They are fixed by the epic and the kinds table.
- Fixing existing clippy warnings, renaming anything, or changing a file not listed under `files:`.

## If stuck

- If `cargo test` fails to compile `logic.rs` because `Filter::Failed`, `Filter::Slow` or `Filter::Pattern` is not found, T04 was not applied; set `status: blocked`.
- If the repo tests fail on Windows paths, set `status: blocked` and quote the failing assertion; do not change the comparison.
- If `cargo fmt` changes the code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
