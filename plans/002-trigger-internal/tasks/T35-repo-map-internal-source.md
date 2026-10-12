---
id: T35
epic: 002-trigger-internal
title: Add the internal source and pty marks to the repo map
status: done
depends_on: [T17, T29]
size: S
model: haiku
files:
  - plans/_context/repo-map.md
---

## Goal

`plans/_context/repo-map.md` has rows for `triggers/internal/`, `triggers/feeds.rs`, `pty/marks.rs`, `pty/tap.rs`, `pty/shell/live.rs` and the wrapper files in `src-tauri/resources/`, and a short section on how a `tg` signal gets from the app to a toast.

## Read first

- `plans/_context/repo-map.md:21-40` the Path table, where the pty and triggers rows are
- `plans/_context/repo-map.md:70-82` the "Process triggers (epic 001)" section
- `plans/002-trigger-internal/epic.md` the Shape section
- `docs/technical-design.md` section 9.2 (T33), which this card points to

## Steps

1. All edits are in `plans/_context/repo-map.md`. Use exact find and replace, in this order.

2. Find this row:

```
| `src-tauri/src/pty/` | PTY sessions, shell integration, the reader thread. `create_terminal`, `write_terminal` |
```

Replace it with these four rows:

```
| `src-tauri/src/pty/` | PTY sessions, shell integration, the reader thread. `create_terminal`, `write_terminal`. `shell.rs` picks the shell and its launch arguments and `env` (zsh `ZDOTDIR` wrapper, bash `--rcfile`, `sh` bare) and writes the scripts and wrapper files to the app data dir |
| `src-tauri/src/pty/marks.rs`, `src-tauri/src/pty/tap.rs` | `marks.rs`: the pure `Scanner` that reads a shell's bytes for `OSC 133` marks, the bell and (on request) printable lines. `tap.rs`: `CommandClock` (command timing) and `Tap`, which turn marks into `InternalSignal`s for the reader thread to publish |
| `src-tauri/src/pty/shell/live.rs` | Unix only tests that start the real zsh and bash with the app's launch arguments in a fake home; a shell that is not installed skips |
| `src-tauri/resources/` | `shell-integration.sh` and `.ps1`, the bash wrapper `bashrc` and the zsh wrapper `zsh/.zshenv`, `.zprofile`, `.zshrc`, `.zlogin`. The wrapper files are compiled in with `include_str!` and written by `install_scripts` |
```

3. Find this text at the end of the `src-tauri/src/triggers/` row:

```
`status.rs` (status and `trigger://fired` JSON) |
```

Replace it with:

```
`status.rs` (status and `trigger://fired` JSON), `feeds.rs` (`publish` and the small builders the feed points call) |
| `src-tauri/src/triggers/internal/` | The `tg` source, not a crate because only the app sees these events. `mod.rs` (`InternalSource`, `InternalSignal`), `kinds.rs` (`KINDS`, thirteen `tg.*` kinds), `rule.rs`, `logic.rs` (pure `step`, tests in `logic/tests.rs`), `README.md` with the same sections as a crate |
```

4. Add this section at the very end of the file (after the paragraph that starts `To add a source:`), with a blank line before it:

```
## Internal triggers (epic 002)

The `tg` source is registered first in `sources()` and is always on. `TriggerService::publish_internal(signal)` is the one entry point; every feed builds an `InternalSignal` and calls `triggers::feeds::publish` after the work it reports has succeeded. Design: `docs/technical-design.md` 9.2. The kinds and their rule fields are in `src-tauri/src/triggers/internal/README.md`.

Where each signal comes from:

| Signal | Feed point |
|---|---|
| `CommandEnded`, `Bell`, `OutputLine` | the reader thread in `pty/mod.rs`, through `pty::tap::Tap` and `pty::marks::Scanner` |
| `AgentFinished` | `agent/mod.rs` |
| `TaskReported` | `agent/task.rs` |
| `PaneOpened` | `create_terminal` in `pty/mod.rs` |
| `FocusEntered` | `set_focused_terminal` in `pty/mod.rs` |
| `WorktreeCreated`, `WorktreeRemoved` | `git/worktree.rs` |
| `ReviewStaged` | `git/review.rs` |
| `NoteSaved` | `notes/mod.rs` |
| `AppStarted` | `triggers::start` in `triggers/mod.rs` |

To add a kind: add it to `KINDS` in `kinds.rs`, parse its `when` in `rule.rs`, handle its signal in `logic.rs` with a test, call `feeds::publish` from the feed point, and refresh the Kinds table in `internal/README.md` (a test fails while it is stale).
```

## Acceptance

- [ ] `grep -c "pty/marks.rs" plans/_context/repo-map.md` prints `1`
- [ ] `grep -c "triggers/internal/" plans/_context/repo-map.md` prints `2` or more
- [ ] `grep -c "^## Internal triggers (epic 002)" plans/_context/repo-map.md` prints `1`
- [ ] `grep -c "feeds.rs" plans/_context/repo-map.md` prints `1`
- [ ] `grep -c "src-tauri/resources/" plans/_context/repo-map.md` prints `1`
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `node plans/_tools/lint-cards.mjs 002-trigger-internal` prints `0 error(s)`
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/002-trigger-internal/tasks/`

## Out of scope

- The "Process triggers (epic 001)" section, the CI row and the Commands table (no new command was added).
- `plans/_context/architecture.md` and `epic.md` (T36).
- Any code.

## If stuck

- If a find text is not found exactly, run `grep -n "src-tauri/src/pty/\|status.rs\|To add a source" plans/_context/repo-map.md` and make the equivalent edit on the line you find. Note it in the Log.
- If `check-crates.mjs` fails, it is not about this file: set status blocked and paste its first error line in the Log.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-13 haiku: built, acceptance passed. All named paths and functions exist in the code. Note: `docs/technical-design.md` has no section 9.2 yet (section 9 has only 9.1), so the new section's pointer to 9.2 is not yet true; it depends on T33/T30 landing.
- 2026-10-13 checker: pass
