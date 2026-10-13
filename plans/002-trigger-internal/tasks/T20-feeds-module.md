---
id: T20
epic: 002-trigger-internal
title: Add the feeds module for the app's own signals
status: done
depends_on: [T09]
size: M
model: haiku
files:
  - src-tauri/src/triggers/feeds.rs
  - src-tauri/src/triggers/mod.rs
---

## Goal

`src-tauri/src/triggers/feeds.rs` exists with the helpers every feed card in slice C uses: `publish`, `enabled`, `repo_root_of`, and four small pure builders (`task_reported`, `focus_entered`, `note_saved`, `review_staged`) that hold the few decisions the feeds make, each with a unit test. Nothing calls them yet.

## Read first

- `plans/002-trigger-internal/epic.md` Decisions 3, 10 and 12
- `src-tauri/src/triggers/mod.rs:1-12` the module list you add one line to
- `src-tauri/src/triggers/internal/mod.rs` `InternalSignal` (the enum the builders return)
- `src-tauri/src/git/mod.rs:19-21` `CACHE_TTL`, used by `repo_root_of`
- `src-tauri/src/pty/mod.rs` the function `path_string` (forward slashes), used by `note_saved`

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance (it was 7 when this card was written).

2. In `src-tauri/src/triggers/mod.rs`, add `pub mod feeds;` on its own line directly above `pub mod internal;`, so the module list starts `feeds`, `internal`, `rules`. Change nothing else in the file.

3. Create `src-tauri/src/triggers/feeds.rs` with exactly this content. How it works: `publish` is the one way a feed hands a signal to `TriggerService::publish_internal`; `enabled` and `repo_root_of` exist so a feed that needs git skips the lookup while triggers are off and reads the git cache instead of running git twice; the four builders are pure.

```rust
//! The seam between the app's own code and the `tg` trigger source. A feed
//! point (an agent finished, a worktree created, a note saved) builds an
//! `InternalSignal` and calls `publish` after the work it reports has
//! succeeded. The small pure builders here hold the few decisions the feeds
//! make, so they are tested without a window. See `internal/README.md`.

use std::path::Path;

use tauri::{AppHandle, Manager};

use super::internal::InternalSignal;
use crate::pty::path_string;

/// Hands `signal` to the trigger service. Cheap when no rule is armed, and a
/// no op before the app state exists.
pub fn publish(app: &AppHandle, signal: InternalSignal) {
    if let Some(state) = app.try_state::<crate::AppState>() {
        state.triggers.publish_internal(signal);
    }
}

/// True when the user has switched process triggers on. Feeds that need a git
/// lookup of their own check this first, so nobody pays for it while the
/// feature is off.
pub fn enabled(state: &crate::AppState) -> bool {
    state.state.config().triggers.enabled
}

/// The repository root of `cwd` as `GitInfo.repo_root` reports it (forward
/// slashes), or None outside a repository. Uses the git cache, so call it
/// from a blocking thread and never from the agent watcher.
pub fn repo_root_of(state: &crate::AppState, cwd: &Path) -> Option<String> {
    let open_dirs = state.pty.open_dirs();
    state
        .git
        .info(cwd, crate::git::CACHE_TTL, &open_dirs)
        .repo_root
}

/// A task report fires `tg.task-reported`; emptying the file (no task) does not.
pub fn task_reported(pane: &str, cwd: &str, task: Option<&str>) -> Option<InternalSignal> {
    task.map(|_| InternalSignal::TaskReported {
        pane: pane.to_string(),
        cwd: cwd.to_string(),
    })
}

/// Focus entering the pane that already had it is not news.
pub fn focus_entered(previous: Option<&str>, pane: &str, cwd: &str) -> Option<InternalSignal> {
    (previous != Some(pane)).then(|| InternalSignal::FocusEntered {
        pane: pane.to_string(),
        cwd: cwd.to_string(),
    })
}

/// `rel_path` is what the frontend sent to `notes_write`. `relative` in the
/// signal always has forward slashes and no leading slash, so a rule's `glob`
/// means the same on every platform; `path` is the file under the notes root.
pub fn note_saved(root: &Path, rel_path: &str) -> InternalSignal {
    let relative = rel_path
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_string();
    InternalSignal::NoteSaved {
        path: path_string(&root.join(&relative)),
        relative,
    }
}

/// `paths` is how many paths the user asked to stage.
pub fn review_staged(repo: &str, paths: usize) -> InternalSignal {
    InternalSignal::ReviewStaged {
        repo: repo.to_string(),
        files: u32::try_from(paths).unwrap_or(u32::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_report_fires_and_an_emptied_file_does_not() {
        assert_eq!(
            task_reported("p1", "/work/app", Some("Fixing tests")),
            Some(InternalSignal::TaskReported {
                pane: "p1".into(),
                cwd: "/work/app".into()
            })
        );
        assert_eq!(task_reported("p1", "/work/app", None), None);
    }

    #[test]
    fn focus_fires_only_when_it_moves_to_another_pane() {
        assert!(focus_entered(None, "p1", "/work/app").is_some());
        assert!(focus_entered(Some("p2"), "p1", "/work/app").is_some());
        assert_eq!(focus_entered(Some("p1"), "p1", "/work/app"), None);
    }

    #[test]
    fn a_saved_note_is_named_relative_to_the_notes_root() {
        let root = std::env::temp_dir().join("tg-feeds-notes");
        let signal = note_saved(&root, "work/plan.md");
        assert_eq!(
            signal,
            InternalSignal::NoteSaved {
                path: path_string(&root.join("work").join("plan.md")),
                relative: "work/plan.md".into(),
            }
        );
    }

    #[test]
    fn note_paths_use_forward_slashes_and_no_leading_slash() {
        let root = std::env::temp_dir().join("tg-feeds-notes");
        let InternalSignal::NoteSaved { relative, .. } = note_saved(&root, "\\work\\plan.md")
        else {
            panic!("expected NoteSaved");
        };
        assert_eq!(relative, "work/plan.md");
    }

    #[test]
    fn review_staged_counts_the_paths() {
        assert_eq!(
            review_staged("/work/app", 3),
            InternalSignal::ReviewStaged {
                repo: "/work/app".into(),
                files: 3
            }
        );
    }
}
```

4. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::feeds` passes and reports 5 passed
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/002-trigger-internal/tasks/`

## Out of scope

- Calling any of these helpers. The feed cards T21 to T28 do.
- Builders for the signals that need no decision (agent finished, pane opened, worktree created and removed, app started). The feed cards build those inline.
- The end to end test of every feed. T29 adds it to this file.
- Editing `internal/`, docs or any README. Fixing existing clippy warnings.

## If stuck

- If `cargo test` says `publish_internal` does not exist on `TriggerService`, slice A (T08) is not applied: set `status: blocked`.
- If `InternalSignal` is missing a variant used above, quote the compile error, set `status: blocked`; do not edit `internal/mod.rs`.
- If `cargo fmt` changes the code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (clippy baseline 7, after 7; feeds tests 5 passed)
- 2026-10-12 checker: pass
