---
id: R01
epic: 002-trigger-internal
question: Does the shell integration today report the exit status and the start time of each command on all three shells, and where in `pty/` is it parsed? Plus a map of every feed Decision 5 names, with file:line and the data at each point.
status: contradicts-decision
---

## Answer

- Nothing in `src-tauri/src/pty/` parses shell integration. OSC 7, 133, 7777 and the bell are parsed in the webview (`src/terminals/registry.ts`, `src/terminals/osc.ts`) and the Rust side only hears `report_cwd`. Exit codes and bell never reach Rust today.
- Exit status: the scripts emit `OSC 133;D;<code>`. PowerShell sends only 0 or 1. bash and zsh send the real code, but see the bug below. No script sends a start time, a duration or `133;C`.
- Bug found by running it: on bash and zsh the launch args (`sh -c '. script; exec "$0" -i'`, `pty/shell.rs:105-111`) throw the script away, because functions and `precmd_functions` do not survive `exec`. Run for real on macOS, zsh and bash emit no OSC at all under those args. Sourcing the script into a live shell works.
- Decision 5 says the Rust side has "the shell integration prompt marks" and "the OSC parser". Neither exists in Rust. Feeds for exit code, duration and bell need a new Rust OSC scanner in the pty reader (or a new IPC command called from the frontend handlers). Either choice is new work, not a tap on an existing feed.
- Other shells: `cmd` and unknown shells (fish, nu) run bare, no marks at all (`shell.rs:113-114`).

## Evidence

### Shell integration scripts (read, then run)

- `src-tauri/resources/shell-integration.sh:1-19`. One function `__tg_precmd` on `precmd_functions` (zsh) or `PROMPT_COMMAND` (bash). Line 4 captures `$?`. Lines 7-11 emit `OSC 7777;cmd;<base64 command>` then `OSC 133;D;<exit>`, but only when the last command text differs from the previous one (`!= $__tg_last_cmd`, line 7). Running `true` twice reports once. Lines 12-13 emit OSC 7 (cwd) and `133;A`. No `133;B`, no `133;C`, no timestamp.
- `src-tauri/resources/shell-integration.ps1:13-34`. `prompt` wrapper. Line 14: `$exit = if ($?) { 0 } else { 1 }`, so only 0 or 1, `$LASTEXITCODE` is ignored. Lines 17-25 emit 7777 and `133;D;$exit` once per new history id. Line 29 `133;A`, line 32 `133;B`. `Get-History` gives `StartExecutionTime` and `Duration`, which are not sent.
- Launch args, `src-tauri/src/pty/shell.rs:92-115`. PowerShell (Pwsh, WindowsPowerShell): `-NoLogo -NoExit -Command ". '<script>'"` (works, the script runs in the live session). Bash and Zsh (`ShellKind`, lines 11-20; `sh` maps to Bash, line 34): `-c '. "<script>"; exec "$0" -i'` (lines 105-111). Cmd and Other: no args (line 114).
- Run in a scratch dir with a pty (python `pty.fork`, macOS):
  - Launch args as in `shell.rs`, zsh and bash: output contained no `ESC ] 7777`, no `ESC ] 133`, no `ESC ] 7`. Only the stock prompt.
  - Same script sourced into a running `-i` shell: zsh and bash both emitted `7777;cmd;ZmFsc2U=` then `133;D;1` for `false`, `133;D;0` for `true`, then OSC 7 and `133;A`. A repeated `true` emitted only OSC 7 and `133;A` (dedupe).
  - Not run: PowerShell (no pwsh on this machine). Its behaviour is from reading the script only.
- Git history only has the original and one install fix (`git log -- shell-integration.sh shell.rs`: 8972777, a0f1ae7, affd14e), so this is not a regression from a recent change.

### Where the marks are parsed

- `src/terminals/osc.ts:25-42` `parseOsc133` (A, B, C, D with optional code), `:45-50` `parseOsc7777`, `:9-16` `parseOsc7`.
- `src/terminals/registry.ts:177` `term.onBell(() => callbacks.onBell(id))`. `:181-185` OSC 7. `:195-200` OSC 133: `command-end` calls `onCommandEnd(id, exitCode)`, `prompt-end` calls `onPrompt`. `:201-205` OSC 7777 calls `onCommand(id, command)`.
- `src/state/AppProvider.tsx:364-366` `onCwd` calls the `report_cwd` IPC. `:370-383` `onCommandEnd` ignores the exit code argument and calls `refresh_git_info`. `:384-392` `onBell` only dispatches a reducer action. No IPC carries exit code, bell or command text to Rust.
- Rust receives only the cwd: `src-tauri/src/pty/mod.rs:600-632` `report_cwd`.

### Output chunks in the reader thread (`src-tauri/src/pty/mod.rs`)

- `start_reader` at `:283-327`. Thread 1 (`:296-317`) loops `output.read(&mut buffer)` (8 KiB, `reader::READ_BUFFER`), line 302 `pump.push(&buffer[..count])`. Raw bytes, one session, one thread, in order. Thread 2 (`:320-326`) `pump.next_batch()` then `emit_output(&app, &id, batch)`.
- `emit_output` at `:468-479`: base64, `app.emit_to("main", "terminal://output", OutputEvent{id, data_b64})`.
- Best tap point: line 302 beside `pump.push` (raw read chunks, has `id` and `generation`), or line 322 (coalesced batches, up to 16 KiB, may split an OSC sequence or a line between batches either way). Chunks split anywhere, so any OSC or line scanner needs its own carry over state. Data is raw terminal bytes including escape sequences: strip ANSI before matching a pattern. Thread 1 captures only `pump`, `id`, `sessions`, `generation`. The AppHandle is available in `start_reader`'s arguments (`:284`) and is moved into thread 2 only; clone it before the move to give thread 1 access.
- `OutputPump` (`pty/reader.rs:99-158`) is generic bytes in, batches out; it has no scanner hook.

### Feed map

| Feed | Where | Data there |
|---|---|---|
| `terminal://agent` | `src-tauri/src/agent/mod.rs:271-289` `emit_transition`, called from `tick` at `:252-255`. `EVENT_AGENT` const `:26`. State machine `step` `:74-132` | pane id (`id`), `event` ("started" or "finished"), `name` (agent), `duration_ms` (finished only), `source` ("process"). Struct `AgentEvent` `:37-44`. No cwd or repo root: get cwd with `state.pty.cwd_of(id)` (`pty/mod.rs:107`) and repo root from `state.git.info(...).repo_root` |
| agent "waiting" | Rust has no such event. Frontend only: `src/state/model.ts:106-110` `paneStatus` returns "idle" when `term.agent` and no output for `idleTimeoutMs` (4000 ms, `reducer.ts:36`), driven by the 1 s `tick` action (`reducer.ts:332`) and `lastOutputAt` from the registry. Pane chip "waiting for input" at `TerminalPane.tsx:149` | Nothing in Rust. `tg.agent-waiting` needs either a Rust idle timer fed by the reader (last output time per pane, pane has a running agent from `AgentState::Running`, `agent/mod.rs:56-64`) or a new IPC command called by the frontend. Decision 5 does not name it |
| `terminal://task` | `src-tauri/src/agent/task.rs:111-146` `TaskTracker::poll`, emit at `:135-143`. `EVENT_TASK` const `:22`. Called each tick from `agent/mod.rs:197` | `TaskEvent{id, task: Option<String> (cut to 280 chars, `:28`), updated_at_ms}` (`:32-37`). The empty file at spawn is skipped (`:125`). No agent name here: look it up from the `AgentState` map `states` in the watcher thread (`agent/mod.rs:185`, name is in `AgentState::Running{name}`) |
| `create_terminal` | `src-tauri/src/pty/mod.rs:499-526`. Spawn at `:518`, then `state.agent.wake()` and `triggers::refresh_app(&app)`. The spawn itself is `PtyManager::spawn` `:187-278` | `id` (uuid, `:516`), `cwd` (resolved, falls back to home, `:506-513`), cols, rows, shell kind. Repo root not known yet (git is lazy): use `state.git.info(&cwd, ...)`. `restart_terminal` `:571` keeps the id and should not count as a new pane |
| `report_cwd` (second pane-in-repo source) | `pty/mod.rs:600-632`. Changed cwd emits `terminal://cwd` at `:612-619`, then `triggers::refresh_app` and `git::refresh_repo_of` | `id`, `cwd`. `tg.pane-opened-in-repo` also fires when an existing pane `cd`s into a repo, if wanted. Needs the OSC 7 script, which is broken on bash and zsh (see above), so this feed is only reliable on PowerShell today |
| `worktree_create` | `src-tauri/src/git/worktree.rs:277-285`; `create(cwd, branch)` returns the new path (`:110`) | pane `id`, `cwd` (via `cwd_of`, `:262`), `branch`, returned `path`. Repo root: `state.git.info(&cwd, ..).repo_root`. Publish after `create` succeeds at `:280-281` |
| `worktree_remove` | `git/worktree.rs:287-295`; `remove(cwd, path)` `:155` | pane `id`, `cwd`, `path` of the removed worktree. No branch name |
| `review_stage` | `src-tauri/src/git/review.rs:572-580`; `stage(cwd, &paths)` `:413`, call at `:575` | pane `id`, `cwd`, `paths: Vec<String>` (relative). Repo root via git info. Per paths call, so the event should carry a count, not every path |
| notes writes | `src-tauri/src/notes/mod.rs:460-473` command `notes_write` calls `NotesService::write` `:126-166` (atomic temp file and rename, returns `WriteResult{mtime_ms}` at `:163`). Also `create` `:168`, `rename` `:195`. External changes: the `notify` watcher `:255-314`, emit `notes://changed` `{rel_path, kind}` at `:294-298` | `rel_path` (relative), absolute path via `self.resolve(rel_path)` (`:96`), `content` (in `notes_write`, only there, needed to read tags). No pane id. The watcher also fires for the app's own writes (debounced), so publish from `notes_write` only to avoid doubles. There is no tag concept in Rust (`grep tag src-tauri/src/notes` is empty), so `tg.note-tagged` must parse the content (for example `#tag` or front matter): needs a decision |
| bell | Webview only. `registry.ts:177` `term.onBell`, `AppProvider.tsx:384-392` | pane id only. A Rust feed needs either a new IPC command (for example `report_bell`) or scanning `0x07` outside OSC in the reader. A BEL byte also terminates every OSC (the scripts end each OSC with `\007`), so a naive `0x07` count in Rust gives false bells; the scanner must track "inside an OSC string" |
| command exit code | Webview only: `registry.ts:195-197`, `AppProvider.tsx:370`. `exitCode` is parsed in `osc.ts:34-38` and dropped | pane id, exit code (null when absent). No start time or duration anywhere |
| pty exit (not a Decision 5 feed) | `pty/mod.rs:329-354` waiter thread, emits `terminal://exit` `{id, code}` at `:352` | shell process exit, not a command exit. Do not use for `tg.command-failed` |
| triggers publish point | `src-tauri/src/triggers/mod.rs:43-50` `TriggerService` holds `hub`; no `publish_internal` yet. `refresh_app` `:177`. Hub publish is used in a test at `:339` | Add `publish_internal` here (slice A) |

### Registry facts used above

- `src-tauri/Cargo.lock:3278` has `regex` as a transitive dep. `src-tauri/Cargo.toml` has no direct `regex` entry (R02 covers the version).
- `AppState` fields (`pty`, `git`, `agent`, `notes`, `state`, `triggers`) are in `src-tauri/src/lib.rs:41-48`; every feed above reaches them with `app.state::<crate::AppState>()`.

## Implications for the cards

1. Fix the launch args for bash and zsh before any `tg.command-*` card, or those kinds never fire on macOS and Linux. Options: for zsh, set `ZDOTDIR` to a dir with a `.zshrc` that sources the user's rc and then ours; for bash, `--rcfile` a file that sources `~/.bashrc` then ours (the comment at `shell.rs:104` rejects `--rcfile` as skipping the user rc, but a wrapper rc that sources it first avoids that). Alternatively set `ENV`/`BASH_ENV` and keep `-i`. This is outside the epic's slices: raise it as a precondition card, or add it to slice C. Test it with the same pty script shape used here (spawn with the exact args, assert `133;D` appears).
2. Extend the scripts to carry timing, since nothing has it. Cheapest: add to the `133;D` payload a third field `;<duration_ms>`, or emit a new `OSC 7777;dur;<ms>`. bash: use `$EPOCHREALTIME` (bash 5, absent in macOS 3.2) or `date +%s` fallback in a `DEBUG` trap or `PS0`; zsh: `preexec_functions` plus `$EPOCHREALTIME` after `zmodload zsh/datetime`; PowerShell: `(Get-History -Count 1).Duration.TotalMilliseconds`. Alternative with no script change: time it in Rust, start = last `133;A`/`B` or the Enter write in `write_terminal` (`pty/mod.rs:527`), end = next `133;D`. That gives rough timings for all shells that emit marks and does not depend on shell versions. Pick one in the card; the epic Shape lists `longer_than_ms` so a duration is required.
3. Fix PowerShell exit code: line 14 of `shell-integration.ps1` should use `$LASTEXITCODE` when `$?` is false and a native command ran. As written, `tg.command-failed` with `exit_code = 2` can never match on PowerShell.
4. Fix the dedupe at `shell-integration.sh:7`, otherwise a repeated failing command (`make` twice) fires once. Compare a counter such as `$HISTCMD` (bash, zsh) instead of the text.
5. Decide where marks are parsed for Rust, one place for exit codes, command start and bell:
   - Option A, recommended: a small pure scanner in `src-tauri/src/pty/` (new file, for example `marks.rs`, with the parse logic and tests, as with `step` in `agent/mod.rs`) fed from `start_reader` thread 1 at `pty/mod.rs:302`. It keeps an OSC state machine across chunks and yields `Mark::CommandEnd{exit}`, `Mark::PromptStart`, `Mark::Bell` (BEL outside an OSC), and the stripped printable text for output matching. Then there is one tap for command, bell and output feeds. It needs the AppHandle (or a `Sender`) cloned into thread 1.
   - Option B: new IPC commands the frontend calls from `registry.ts:177,195` (`report_command_end{id, exit_code}`, `report_bell{id}`). Smaller Rust change, but the feeds then depend on the window being alive and on the webview's xterm having parsed it, and a hidden or throttled webview lags.
6. Output matching (Decision 4): add the 4 KiB rolling buffer per pane in the Option A scanner. Only run the regex when the pane has an armed `tg.output-matched` rule (the Risk in the epic). Keep the buffer split on `\n` after stripping ANSI and the OSC strings, and cut the matched line to 200 characters.
7. Per feed card: publish from the success path, after the work, not before. Exact places: `agent/mod.rs:254` (after `emit_transition`; map `Finished` to `tg.agent-finished`), `agent/task.rs:135` (after the emit), `pty/mod.rs:518` (after `spawn`), `git/worktree.rs:280-281` and `:290-291`, `git/review.rs:575`, `notes/mod.rs` `notes_write` after `write` returns Ok. For `tg.pane-opened-in-repo`, repo root needs a `git.info` call, so run it in the `blocking` closure (`create_terminal` already is). Never call git on the agent watcher thread without a cache max age, since it ticks every second.
8. `tg.agent-waiting` and `tg.note-tagged` have no existing feed. Either add a Rust idle check in the agent watcher tick (`agent/mod.rs:188-198`, last output time per pane from the reader scanner) and a tag parser for note content, or drop them from the first cut. They need an epic decision.
9. `tg.focus-entered` is also not in Decision 5. The source is `set_focused_terminal` (`pty/mod.rs:631`, `state.pty.set_focused(id)`), which has pane id and cwd.

## Doubts

- PowerShell was read, not run (no pwsh available). Windows behaviour of the `-Command` dot source is as documented in `docs/technical-design.md` 4.5.
- I did not run the app itself, only the scripts under a pty with the exact `shell.rs` args. The finding that bash/zsh emit nothing could still be masked by a user rc that sets its own precmd, but not by the app.
- Exact stdout framing for a command that contains a literal BEL or OSC string is unchecked.
- The bash 3.2 default on macOS has no `$EPOCHREALTIME`; the timing approach in item 2 needs a card level decision.
