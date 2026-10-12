---
id: T01
epic: 002-trigger-internal
title: Add regex and the internal module with InternalSignal
status: done
depends_on: []
size: S
model: haiku
files:
  - src-tauri/Cargo.toml
  - src-tauri/Cargo.lock
  - src-tauri/src/triggers/mod.rs
  - src-tauri/src/triggers/internal/mod.rs
---

## Goal

The app depends on `regex` 1.13 and `globset`, and `src-tauri/src/triggers/internal/mod.rs` exists with the `InternalSignal` enum that every later card and every feed in slices B and C uses. Nothing is registered or fed yet.

## Read first

- `plans/002-trigger-internal/epic.md` Decisions 1, 10, 11, 13 and the `InternalSignal` block under Shape
- `src-tauri/Cargo.toml` the `[workspace.dependencies]` table and the app's `[dependencies]`
- `src-tauri/src/triggers/mod.rs:1-12` the module list you add one line to
- `plans/002-trigger-internal/research/R02-regex.md` why `regex` costs nothing extra

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance (it was 7 when this card was written).

2. In `src-tauri/Cargo.toml`, in `[workspace.dependencies]`, add this line directly after the line `globset = "0.4.20"`:

```toml
regex = "1.13"
```

3. In the same file, in the app's `[dependencies]`, add these two lines directly after the line `dirs = "7"` (and before the comment that starts `# Cloud API client`):

```toml
globset = { workspace = true }
regex = { workspace = true }
```

`globset` is already a workspace dependency (the file source uses it); the app needs it for the `agent` and `glob` fields. It is the same version, so the lock does not grow.

4. In `src-tauri/src/triggers/mod.rs`, add `pub mod internal;` on its own line directly above `pub mod rules;`, so the module list reads `internal`, `rules`, `status`, `store`, `variables`. Change nothing else in the file.

5. Create `src-tauri/src/triggers/internal/mod.rs` with exactly this content:

```rust
//! The `tg` source: triggers the app raises itself (an agent finished, a
//! command failed, a worktree created). Built up over several cards; the
//! finished module doc and `README.md` come with the last ones.

/// Something the app saw. The feeds build one and hand it to
/// `TriggerService::publish_internal`.
#[derive(Debug, Clone, PartialEq)]
pub enum InternalSignal {
    AgentFinished {
        pane: String,
        cwd: String,
        agent: String,
    },
    CommandEnded {
        pane: String,
        cwd: String,
        exit_code: i32,
        duration_ms: u64,
    },
    OutputLine {
        pane: String,
        cwd: String,
        line: String,
    },
    Bell {
        pane: String,
    },
    TaskReported {
        pane: String,
        cwd: String,
    },
    PaneOpened {
        pane: String,
        cwd: String,
        repo_root: Option<String>,
    },
    WorktreeCreated {
        repo: String,
        path: String,
        branch: Option<String>,
    },
    WorktreeRemoved {
        repo: String,
        path: String,
    },
    ReviewStaged {
        repo: String,
        files: u32,
    },
    NoteSaved {
        path: String,
        relative: String,
    },
    FocusEntered {
        pane: String,
        cwd: String,
    },
    AppStarted,
}
```

`InternalSignal` is the shape the feeds publish. Field names and types are fixed: slices B and C build these values.

6. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance. The build updates `src-tauri/Cargo.lock` itself; do not edit it by hand. `git diff src-tauri/Cargo.lock` must show only `globset` and `regex` added to the dependency list of the `terminal-grid` package.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::internal` passes and reports 0 passed
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Registering the source, `InternalSource`, or any other type. Later cards add them.
- Using `regex` or `globset` anywhere yet.
- Adding any feed (`publish_internal` calls).
- Fixing existing clippy warnings, renaming anything, or changing a file not listed under `files:`.

## If stuck

- If `git diff src-tauri/Cargo.lock` shows any version change or any package added other than `globset` and `regex` in the `terminal-grid` dependency list, set `status: blocked` and quote the diff. Do not run `cargo update`.
- If the build says it cannot download a crate, the network is off: set `status: blocked`. `regex` 1.13.1 and `globset` are already in the lock, so a download means the lock was not used.
- If `cargo fmt` changes the code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (clippy baseline 7, after 7; Cargo.lock adds only globset and regex to terminal-grid; triggers::internal 0 passed; workspace tests ok; no-default-features check ok; fmt check ok)
- 2026-10-12 checker: pass
