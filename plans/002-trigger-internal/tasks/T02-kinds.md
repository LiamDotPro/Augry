---
id: T02
epic: 002-trigger-internal
title: Add the thirteen tg kinds
status: todo
depends_on: [T01]
size: M
model: haiku
files:
  - src-tauri/src/triggers/internal/kinds.rs
  - src-tauri/src/triggers/internal/mod.rs
---

## Goal

`KINDS`, the catalogue of the thirteen `tg.*` kinds with their `when` and payload fields, exists in `kinds.rs` and is re-exported from `triggers::internal`. It is in its own file so `mod.rs` stays under 400 lines.

## Read first

- `plans/002-trigger-internal/epic.md` Decisions 2, 3 and 11
- `src-tauri/crates/trigger-fs/src/lib.rs:15-145` the same shape for the `fs` kinds: `FieldInfo` constants and one `KindInfo` per kind
- `src-tauri/crates/trigger-core/src/lib.rs:89-110` `FieldInfo` and `KindInfo`
- `src-tauri/src/triggers/internal/mod.rs` the file you edit (T01)

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance (it was 7 when this card was written).

2. Create `src-tauri/src/triggers/internal/kinds.rs` with exactly this content. The kind ids are a contract with saved process definitions. The `required` flags and the field names are fixed by the epic (Decision 11): only `longer_than_ms` and `pattern` are required.

```rust
//! The kinds of the `tg` source and what their `when` and payload hold.
//! The ids are a contract with saved process definitions: add kinds, never
//! rename one.

use trigger_core::{FieldInfo, KindInfo, Platforms};

const REPO: FieldInfo = FieldInfo {
    name: "repo",
    doc: "Only fire for this repository, absolute path. Panes inside it count",
    required: false,
};

const WHEN_REPO: &[FieldInfo] = &[REPO];

const WHEN_AGENT: &[FieldInfo] = &[
    FieldInfo {
        name: "agent",
        doc: "Glob for the agent name, ignoring case, such as claude or c*. Default any agent",
        required: false,
    },
    REPO,
];

const WHEN_FAILED: &[FieldInfo] = &[
    FieldInfo {
        name: "exit_code",
        doc: "Only this exit code. Default any code except 0",
        required: false,
    },
    REPO,
];

const WHEN_SLOW: &[FieldInfo] = &[
    FieldInfo {
        name: "longer_than_ms",
        doc: "Fire when the command took more than this many milliseconds",
        required: true,
    },
    FieldInfo {
        name: "exit_code",
        doc: "Only this exit code. Default any code",
        required: false,
    },
    REPO,
];

const WHEN_OUTPUT: &[FieldInfo] = &[
    FieldInfo {
        name: "pattern",
        doc: "Regular expression, ignoring case, tried on each line of output",
        required: true,
    },
    REPO,
];

const WHEN_NOTE: &[FieldInfo] = &[FieldInfo {
    name: "glob",
    doc: "Which notes count, matched against the path below the notes folder. Default any note",
    required: false,
}];

const WHEN_NONE: &[FieldInfo] = &[];

const PANE: FieldInfo = FieldInfo {
    name: "pane",
    doc: "Id of the terminal pane",
    required: true,
};

const CWD: FieldInfo = FieldInfo {
    name: "cwd",
    doc: "Working directory of the pane",
    required: true,
};

const PAYLOAD_AGENT: &[FieldInfo] = &[
    PANE,
    CWD,
    FieldInfo {
        name: "agent",
        doc: "Name of the agent that finished",
        required: true,
    },
];

const PAYLOAD_COMMAND: &[FieldInfo] = &[
    PANE,
    CWD,
    FieldInfo {
        name: "exit_code",
        doc: "Exit code of the command",
        required: true,
    },
    FieldInfo {
        name: "duration_ms",
        doc: "How long the command ran, in milliseconds",
        required: true,
    },
];

const PAYLOAD_OUTPUT: &[FieldInfo] = &[
    PANE,
    CWD,
    FieldInfo {
        name: "line",
        doc: "The line that matched, cut to 200 characters",
        required: true,
    },
];

const PAYLOAD_BELL: &[FieldInfo] = &[PANE];

const PAYLOAD_PANE: &[FieldInfo] = &[PANE, CWD];

const PAYLOAD_OPENED: &[FieldInfo] = &[
    PANE,
    CWD,
    FieldInfo {
        name: "repo",
        doc: "Root of the repository the pane opened in",
        required: true,
    },
];

const PAYLOAD_WORKTREE_CREATED: &[FieldInfo] = &[
    FieldInfo {
        name: "repo",
        doc: "Root of the repository",
        required: true,
    },
    FieldInfo {
        name: "path",
        doc: "Absolute path of the new worktree",
        required: true,
    },
    FieldInfo {
        name: "branch",
        doc: "Branch checked out in it, when there is one",
        required: false,
    },
];

const PAYLOAD_WORKTREE_REMOVED: &[FieldInfo] = &[
    FieldInfo {
        name: "repo",
        doc: "Root of the repository",
        required: true,
    },
    FieldInfo {
        name: "path",
        doc: "Absolute path of the removed worktree",
        required: true,
    },
];

const PAYLOAD_STAGED: &[FieldInfo] = &[
    FieldInfo {
        name: "repo",
        doc: "Root of the repository",
        required: true,
    },
    FieldInfo {
        name: "files",
        doc: "How many files were staged",
        required: true,
    },
];

const PAYLOAD_NOTE: &[FieldInfo] = &[
    FieldInfo {
        name: "path",
        doc: "Absolute path of the note",
        required: true,
    },
    FieldInfo {
        name: "relative",
        doc: "Path below the notes folder, with / separators",
        required: true,
    },
];

const PAYLOAD_NONE: &[FieldInfo] = &[];

/// Every kind of the `tg` source.
pub static KINDS: &[KindInfo] = &[
    KindInfo {
        id: "tg.agent-finished",
        title: "Agent finished",
        summary: "A coding agent in a pane stopped running.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_AGENT,
        payload: PAYLOAD_AGENT,
    },
    KindInfo {
        id: "tg.command-failed",
        title: "Command failed",
        summary: "A command in a pane ended with a failing exit code.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_FAILED,
        payload: PAYLOAD_COMMAND,
    },
    KindInfo {
        id: "tg.command-slow",
        title: "Command was slow",
        summary: "A command in a pane took longer than a set time.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_SLOW,
        payload: PAYLOAD_COMMAND,
    },
    KindInfo {
        id: "tg.output-matched",
        title: "Output matched",
        summary: "A line of terminal output matched a pattern.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_OUTPUT,
        payload: PAYLOAD_OUTPUT,
    },
    KindInfo {
        id: "tg.bell",
        title: "Bell",
        summary: "A terminal rang its bell.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_NONE,
        payload: PAYLOAD_BELL,
    },
    KindInfo {
        id: "tg.task-reported",
        title: "Task reported",
        summary: "An agent in a pane wrote a new task line.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_PANE,
    },
    KindInfo {
        id: "tg.pane-opened-in-repo",
        title: "Pane opened in a repository",
        summary: "A new terminal pane opened inside a git repository.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_OPENED,
    },
    KindInfo {
        id: "tg.worktree-created",
        title: "Worktree created",
        summary: "A git worktree was created from the app.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_WORKTREE_CREATED,
    },
    KindInfo {
        id: "tg.worktree-removed",
        title: "Worktree removed",
        summary: "A git worktree was removed from the app.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_WORKTREE_REMOVED,
    },
    KindInfo {
        id: "tg.review-staged",
        title: "Review staged",
        summary: "Files were staged in the review panel.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_STAGED,
    },
    KindInfo {
        id: "tg.note-saved",
        title: "Note saved",
        summary: "A note was saved from the notes editor.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_NOTE,
        payload: PAYLOAD_NOTE,
    },
    KindInfo {
        id: "tg.focus-entered",
        title: "Pane focused",
        summary: "A terminal pane got the focus.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_PANE,
    },
    KindInfo {
        id: "tg.app-started",
        title: "App started",
        summary: "Terminal Grid started.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_NONE,
        payload: PAYLOAD_NONE,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_thirteen_unique_tg_kinds() {
        assert_eq!(KINDS.len(), 13);
        let mut ids: Vec<&str> = KINDS.iter().map(|k| k.id).collect();
        assert!(ids.iter().all(|id| id.starts_with("tg.")));
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 13);
    }

    #[test]
    fn every_kind_has_a_title_and_a_summary() {
        for kind in KINDS {
            assert!(!kind.title.is_empty(), "{}", kind.id);
            assert!(!kind.summary.is_empty(), "{}", kind.id);
        }
    }

    #[test]
    fn only_command_slow_and_output_matched_require_a_field() {
        let required: Vec<&str> = KINDS
            .iter()
            .filter(|k| k.when.iter().any(|f| f.required))
            .map(|k| k.id)
            .collect();
        assert_eq!(required, vec!["tg.command-slow", "tg.output-matched"]);
    }
}
```

3. In `src-tauri/src/triggers/internal/mod.rs` find these lines:

```rust
//! finished module doc and `README.md` come with the last ones.

/// Something the app saw.
```

and replace them with:

```rust
//! finished module doc and `README.md` come with the last ones.

mod kinds;

pub use kinds::KINDS;

/// Something the app saw.
```

4. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::internal` passes and reports 3 passed
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- The `README.md` and its table test. T09 does them.
- Any `platforms` other than `Platforms::ALL` or any `permission`. Nothing here talks to the OS.
- Renaming a kind or changing a field name.
- Fixing existing clippy warnings, renaming anything, or changing a file not listed under `files:`.

## If stuck

- If `cargo test` says `Platforms::ALL` or `FieldInfo` is not found, the import line `use trigger_core::{FieldInfo, KindInfo, Platforms};` was changed; restore it as given.
- If `cargo fmt` changes the code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
