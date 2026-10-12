---
id: T33
epic: 002-trigger-internal
title: Design 9.2, the internal tg trigger source
status: todo
depends_on: [T32]
size: M
model: haiku
files:
  - docs/technical-design.md
---

## Goal

`docs/technical-design.md` has a new section 9.2 that describes the internal `tg` trigger source: its kinds, where each signal is fed from, the `publish_internal` entry point and the known limits.

## Read first

- `docs/technical-design.md:585-606` section 9.1, which the new section follows
- `plans/002-trigger-internal/epic.md` Decisions 1 to 13
- `src-tauri/src/triggers/internal/README.md` the Kinds table and Known limits (T09)
- `plans/002-trigger-internal/tasks/T20-feeds-module.md` the feed helpers
- `plans/002-trigger-internal/tasks/T08-register-and-publish.md` `publish_internal` and `wants_output`

## Steps

1. All edits are in `docs/technical-design.md`. Find this one line, the last sentence of section 9.1:

```
A rule whose variable has no value on this machine is `skipped` with a reason. Rule ids are `<processId>/<triggerId>`, with `#<n>` added (n from 1) when `${repo.root}` produced several.
```

2. Replace it with the same line, then a blank line, then the text between the markers `BEGIN` and `END` below (do not copy the two marker lines). The new heading `### 9.2 Internal triggers (`tg`)` must start at the beginning of a line.

BEGIN
```
### 9.2 Internal triggers (`tg`)

Things the app already sees become triggers with no new crate, because only the app sees them and a crate would need the app to feed it. The source lives in `src-tauri/src/triggers/internal/`: `mod.rs` (`InternalSource`, source id `tg`, and the `InternalSignal` enum), `kinds.rs` (`KINDS`), `rule.rs` (parsing a rule's `when` per kind), `logic.rs` (the pure `step(&[Armed], &InternalSignal)`, tested in `logic/tests.rs`) and a `README.md` with the same sections as a crate. The catalogue and the editor see it like any other source. `triggers/feeds.rs` holds the helpers the feed points call.

`InternalSource` is cloneable (an `Arc` inside). `sources()` boxes one clone for the hub and `TriggerService` keeps another. `start` stores the parsed rules and the sink, and dropping the handle clears them. The app feeds it with `TriggerService::publish_internal(signal)`, which runs `logic::step` and sends an event for every rule that matches. `TriggerService::wants_output()` is an `AtomicBool`, true only while a `tg.output-matched` rule is armed, so the reader thread skips line assembly otherwise.

| Kind | Signal | Fed from |
|---|---|---|
| `tg.agent-finished` | `AgentFinished` | `agent/mod.rs`, after the finished event is emitted (starts publish nothing) |
| `tg.command-failed`, `tg.command-slow` | `CommandEnded` | the reader thread tap, on `OSC 133;D` (4.3) |
| `tg.output-matched` | `OutputLine` | the reader thread tap, only while a rule is armed |
| `tg.bell` | `Bell` | the reader thread tap |
| `tg.task-reported` | `TaskReported` | `agent/task.rs`, when a new task line is read (emptying the file publishes nothing) |
| `tg.pane-opened-in-repo` | `PaneOpened` | `create_terminal`, after spawn and a refresh; the repository root comes from the git cache and a pane outside a repository fires nothing |
| `tg.worktree-created`, `tg.worktree-removed` | `WorktreeCreated`, `WorktreeRemoved` | `git/worktree.rs`, after git succeeded |
| `tg.review-staged` | `ReviewStaged` | `git/review.rs`, `review_stage`, after `git add` succeeded |
| `tg.note-saved` | `NoteSaved` | `notes_write`, after the write returned Ok |
| `tg.focus-entered` | `FocusEntered` | `set_focused_terminal`, only when focus moves onto a different pane |
| `tg.app-started` | `AppStarted` | `triggers::start`, once, after the first refresh has armed the rules |

Feeds publish after the work they report has succeeded. A feed that needs a git lookup checks that process triggers are switched on first, so nothing runs while the feature is off.

Payloads carry `pane`, `cwd`, `repo`, `agent`, `exit_code`, `duration_ms`, `line`, `path`, `relative`, `branch` and `files` as each signal has them, never the full output. The `when` fields per kind: `agent` (glob) for `tg.agent-finished`; `exit_code` for `tg.command-failed` (default: any non zero); `longer_than_ms` (required) and `exit_code` for `tg.command-slow`; `pattern` (required) for `tg.output-matched`, a `regex` 1.13 expression, case insensitive, with a 256 KiB size limit, compiled once when the rule starts; `repo` (absolute path) on every pane, worktree and review kind, where pane kinds match it against the pane's working directory; `glob` (relative to the notes root) for `tg.note-saved`. Unknown fields are a rule error. `tg.agent-waiting`, `tg.note-tagged`, `tg.file-dropped`, `tg.file-opened` and `tg.tray-menu` do not exist yet.

Known limits:

- Commands the shell does not record in its history are not reported, so `tg.command-failed` and `tg.command-slow` miss them: bash with `HISTCONTROL=ignoredups` or `ignorespace`, and zsh with `HIST_IGNORE_DUPS`, `HIST_IGNORE_SPACE` and the other `HIST_IGNORE_*` options. The script reports a command only when the history number changed (4.5).
- `tg.output-matched` can match the echoed command line the shell prints back as the user types, not only the command's own output.
- A rule sees at most 200 characters of a line, and the scanner keeps at most 1024 bytes of one line.
- An `OSC 133;D` with no code counts as exit 0.
- A command's duration starts at the first Enter after a prompt mark (or after the last command end); later Enters typed into a running program do not restart it. A shell with no marks (cmd.exe, `sh`) reports no command ends at all.
- zsh runs with `-i` and is not a login shell, so the user's `.zprofile` and `.zlogin` do not run for a pane.
- Each rule has the hub's two second cool down, and rules run on the thread that publishes the signal, so a pattern is matched on the terminal reader thread.
```
END

## Acceptance

- [ ] `grep -c "^### 9.2 Internal triggers" docs/technical-design.md` prints `1`
- [ ] `grep -c "publish_internal" docs/technical-design.md` prints `2`
- [ ] `grep -c "HIST_IGNORE_DUPS" docs/technical-design.md` prints `1`
- [ ] `grep -c "counts as exit 0" docs/technical-design.md` prints `1`
- [ ] `grep -c "^Known limits:" docs/technical-design.md` prints `1`
- [ ] `grep -c "Rule ids are" docs/technical-design.md` prints `1`
- [ ] `grep -n "^### 9.2\|^## 10\. " docs/technical-design.md` prints two lines, the `### 9.2` line first
- [ ] `node plans/_tools/lint-cards.mjs 002-trigger-internal` prints `0 error(s)`
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/002-trigger-internal/tasks/`

## Out of scope

- Editing 9.1, section 10 (no new IPC command or event was added) or anything in 4 and 5.
- Adding kinds to the table that the epic does not build.
- Any code or the `README.md` in `internal/`.

## If stuck

- If the 9.1 line in step 1 is not found, run `grep -n "Rule ids are" docs/technical-design.md` and put the new section after the paragraph that holds it. If a heading `### 9.2` already exists, set `status: blocked`.
- If `grep -c "publish_internal"` prints `1`, T32 was not applied or the new section was pasted short; check the text was pasted whole.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
