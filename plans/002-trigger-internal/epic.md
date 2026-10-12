---
id: 002-trigger-internal
title: Triggers from inside Terminal Grid
status: carding
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
- `tg.agent-waiting` and `tg.note-tagged`: no feed exists for either (R01);
  a later epic adds an idle check and a note tag parser.
- `tg.file-dropped`, `tg.file-opened`, `tg.tray-menu`: the app has no drop
  handler, file association or tray today. They come with those features.

## Decisions

1. **No crate, same shape.** Only the app sees these events, so a crate would
   need the app to feed it, which breaks isolation. The source lives in
   `src-tauri/src/triggers/internal/` with the same split as a crate: `mod.rs`
   (`KINDS`, `InternalSource` with id `tg`), `rule.rs` and `logic.rs` (pure,
   tested), and `README.md` with the same eight sections. The catalogue and the
   editor see it like any other source.
2. **Kinds**: `tg.agent-finished`, `tg.command-failed`,
   `tg.command-slow`, `tg.output-matched`, `tg.bell`, `tg.task-reported`,
   `tg.pane-opened-in-repo`, `tg.worktree-created`, `tg.worktree-removed`,
   `tg.review-staged`, `tg.note-saved`, `tg.focus-entered`, `tg.app-started`
   (thirteen; Decision 6 removed `tg.agent-waiting`, `tg.note-tagged` and the
   three app events, see Non goals).
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

6. **R01 contradicts Decision 5** (status `contradicts-decision`): Rust has no
   shell integration parser, exit codes and the bell never reach Rust, and on
   bash and zsh the integration does not run at all because the launch args
   `exec` away the sourced script. Decided: fix the integration and parse
   marks in Rust (R01 option A), as below.
7. **Shell integration fixes** (`src-tauri/resources/` scripts and
   `src-tauri/src/pty/shell.rs`), each its own card with a pty test that
   spawns the real shell with the real args and asserts `OSC 133;D` appears
   and that a variable set in the user's own rc file is visible:
   - **zsh**: the VS Code approach. Set `ZDOTDIR` to an app owned folder whose
     `.zshenv`, `.zprofile`, `.zshrc` and `.zlogin` each source the user's own
     file (from `$TG_USER_ZDOTDIR`, set to the user's original `ZDOTDIR` or
     `$HOME`) and restore `ZDOTDIR`; `.zshrc` then sources our script. Launch
     with `-i`.
   - **bash**: `--rcfile <app folder>/bashrc` with `-i`, where that file
     sources `~/.bashrc` when it exists, then our script.
   - **PowerShell**: report `$LASTEXITCODE` when `$?` is false and a native
     command ran (R01 implication 3). Only checkable by reading on macOS;
     the card's acceptance greps the script.
   - **Repeated commands**: compare `$HISTCMD` instead of the command text in
     `shell-integration.sh`, so running `make` twice reports twice.
   The scripts stay installed by `install_scripts`; the wrapper files are
   installed beside them the same way.
8. **Marks are parsed in Rust**: a pure scanner `src-tauri/src/pty/marks.rs`
   (state carried across chunks, unit tested like `agent::step`) fed from the
   reader thread at `pty/mod.rs:302`. It yields `Mark::PromptStart`,
   `Mark::CommandEnd { exit: i32 }`, `Mark::Bell` (a BEL outside an OSC) and,
   only while the pane has an armed `tg.output-matched` rule, complete
   printable lines with CSI, OSC and two byte escapes stripped (R02's ~40 line
   stripper). The webview keeps its own parsing; nothing in the frontend
   changes.
9. **Command timing is measured in Rust**: the start is the first
   `write_terminal` containing `\r` after a `PromptStart` (or after the last
   `CommandEnd`), the end is the next `CommandEnd`. No script prints timings,
   so every shell with marks gets durations.
10. **The source** is `InternalSource`, cloneable (an `Arc` inside) so
    `sources()` boxes one clone and `TriggerService` keeps another to feed it.
    `start` stores the armed rules and the sink; dropping the handle clears
    them. The app feeds it with `TriggerService::publish_internal(signal:
    InternalSignal)`, which runs the pure `logic::step(&armed, &signal)` and
    sends the events. `wants_output()` is an `AtomicBool`, true only while an
    output rule is armed, so the reader thread skips line assembly otherwise.
11. **`when` per kind**: `agent` (glob) for agent-finished; `exit_code`
    (integer, default any non zero) for command-failed; `longer_than_ms`
    (required) and optional `exit_code` for command-slow; `pattern` (required,
    `regex` 1.13, `RegexBuilder` case insensitive, size limit 256 KiB,
    compiled once in `start`) for output-matched; `repo` (absolute path, after
    variables) optional on every pane, worktree and review kind; `glob`
    (relative to the notes root) for note-saved; nothing for bell and
    app-started. Pane kinds (task-reported, focus-entered, command and output
    kinds) match `repo` against the pane's cwd, so a rule fires for panes
    inside that repository (architect ruling during T02). Unknown fields are a
    `RuleError`.
12. **Feed points** are R01 implication 7's exact places, publishing after the
    work succeeds: `agent/mod.rs` after `emit_transition` (Finished only),
    `agent/task.rs` after its emit, `create_terminal` after spawn (repo root
    from the git service inside the existing `blocking` closure),
    `git/worktree.rs` create and remove, `git/review.rs` stage, `notes_write`
    after `write` returns Ok, `set_focused_terminal`, and app start after
    `triggers::start`.
13. **`regex = "1.13"`** goes in `[workspace.dependencies]` (already in the
    lock, R02).

## Shape

```
src-tauri/src/triggers/internal/
  mod.rs        KINDS (13), InternalSource, InternalSignal, publish
  rule.rs       When, parse per kind (Decision 11)
  logic.rs      Armed, step(&[Armed], &InternalSignal) -> Vec<(RuleId, kind, payload)>
  logic/tests.rs
  README.md     the eight sections, like a crate
src-tauri/src/pty/marks.rs   Scanner, Mark, strip_ansi (pure, tested)
src-tauri/resources/          shell-integration.sh (HISTCMD), .ps1 (LASTEXITCODE),
                              zsh/ wrapper dotfiles, bashrc wrapper
```

```rust
pub enum InternalSignal {
    AgentFinished { pane: String, cwd: String, agent: String },
    CommandEnded { pane: String, cwd: String, exit_code: i32, duration_ms: u64 },
    OutputLine { pane: String, cwd: String, line: String },
    Bell { pane: String },
    TaskReported { pane: String, cwd: String },
    PaneOpened { pane: String, cwd: String, repo_root: Option<String> },
    WorktreeCreated { repo: String, path: String, branch: Option<String> },
    WorktreeRemoved { repo: String, path: String },
    ReviewStaged { repo: String, files: u32 },
    NoteSaved { path: String, relative: String },
    FocusEntered { pane: String, cwd: String },
    AppStarted,
}
```

Payload per Decision 3: `pane`, `cwd`, `repo`, `agent`, `exit_code`,
`duration_ms`, `line` (at most 200 characters), `path`, `relative`, `branch`,
`files` as each signal has them. Never the full output.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A (T01 to T09) | `triggers/internal/`: KINDS, `InternalSignal`, `rule.rs`, pure `step`, README, registration in `sources()`, `publish_internal` | |
| B (T10 to T19) | shell integration fixes (zsh, bash, PowerShell, HISTCMD), `pty/marks.rs` scanner and stripper, reader thread tap, command timing, command, bell and output feeds | A |
| C (T20 to T29) | the other feeds: agent, task, pane opened, worktrees, review, notes, focus, app started | A |
| D (T30 to T39) | technical design sections, repo map | B, C |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | Does the shell integration today report the exit status and the start time of each command on all three shells, and where in `pty/` is it parsed? | `tg.command-failed` and `tg.command-slow` | No: Rust parses nothing, bash and zsh integration is broken by `exec`, PowerShell sends 0 or 1. Decisions 6 to 9. |
| R02 | Current `regex` crate version and its size cost in the app binary | Decision 4 | `regex` 1.13.1 already in the lock; write a small ANSI stripper. Decisions 8, 11, 13. |

## Risks

- Output matching on every chunk of every pane must stay cheap: only panes
  with an armed `tg.output-matched` rule are scanned.
