---
id: 002-trigger-internal
title: Triggers from inside Terminal Grid
status: draft
depends_on: [001-trigger-core]
---

# 002 Triggers from inside Terminal Grid

## Goal

Things the app already sees become triggers, published into the hub with no
new crate: an agent finishing or waiting, a command failing, output matching a
pattern, a bell, a pane opening in a repo, a worktree being created, a note
saved with a tag. These are the cheapest triggers to build and the most useful,
because they tie processes to what the user is already doing in the grid.

## Non goals

- Any new OS watching. Everything here comes from events the app emits today.
- The run engine. This epic ends at `trigger://fired`.

## Decisions

1. **No crate.** The source lives in `src-tauri/src/triggers/internal.rs` as
   `InternalSource` (id `tg`), so the catalogue and the editor see it like any
   other source. The pure matchers (output patterns, exit codes, note tags) are
   tested like any crate's.
2. **Kinds**: `tg.agent-finished`, `tg.agent-waiting`, `tg.command-failed`,
   `tg.command-slow`, `tg.output-matched`, `tg.bell`, `tg.task-reported`,
   `tg.pane-opened-in-repo`, `tg.worktree-created`, `tg.worktree-removed`,
   `tg.review-staged`, `tg.note-saved`, `tg.note-tagged`, `tg.focus-entered`,
   `tg.app-started`.
3. **Payload** carries pane id, cwd, repo root, agent name, exit code,
   duration, the matched line (at most 200 characters), the note path. Never
   the full output.
4. **Output matching** reads the same chunks the reader thread emits, with a
   bounded rolling buffer per pane (4 KiB) and `regex` at the version research
   finds, compiled once per rule.
5. **Feeds** come from the existing events: `terminal://agent`,
   `terminal://task`, the shell integration prompt marks (exit status,
   timing), the OSC parser (bell), `create_terminal`, `worktree_create`,
   `review_stage`, notes writes. Each feed is one card.

## Shape

`InternalSource` implements `Source` and keeps a `Sender<Event>` the app's
services call through `TriggerService::publish_internal(kind, payload)`. The
`when` fields per kind are written in the Shape during planning: `agent`,
`pattern`, `exit_code`, `longer_than_ms`, `tag`, `repo`.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A | `InternalSource`, kinds table, `when` parsing, pure matchers with tests | |
| B | feeds from the agent watcher, the task file and the OSC parser | A |
| C | feeds from the pty (exit codes, timings, output), git and notes | A |
| D | README style doc section, repo map | B, C |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | Does the shell integration today report the exit status and the start time of each command on all three shells, and where in `pty/` is it parsed? | `tg.command-failed` and `tg.command-slow` | |
| R02 | Current `regex` crate version and its size cost in the app binary | Decision 4 | |

## Risks

- Output matching on every chunk of every pane must stay cheap: only panes
  with an armed `tg.output-matched` rule are scanned.
