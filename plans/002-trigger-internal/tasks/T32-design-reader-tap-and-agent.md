---
id: T32
epic: 002-trigger-internal
title: Design 4.3, 5.2 and 5.3, the marks scanner and tap
status: done
depends_on: [T31]
size: S
model: haiku
files:
  - docs/technical-design.md
---

## Goal

Section 4.3 describes the marks scanner and the tap that the reader thread now feeds, and 5.2 and 5.3 say that Rust now reads the same marks and the bell but uses them for trigger signals only, not for agent detection.

## Read first

- `docs/technical-design.md:237-248` section 4.3 as it is today
- `docs/technical-design.md:323-330` sections 5.2 and 5.3
- `plans/002-trigger-internal/tasks/T14-marks-scanner.md` Goal and the `Mark` and `Scan` types
- `plans/002-trigger-internal/tasks/T15-command-clock-and-tap.md` Goal and the `Tap::process` signature
- `plans/002-trigger-internal/tasks/T16-reader-tap.md` steps 5 and 6, where `write` and the reader call the tap

## Steps

1. All edits are in `docs/technical-design.md`. Use exact find and replace, in this order.

2. Find this text (the start of a paragraph in 4.3):

```
Exit: the waiter thread emits `terminal://exit { id, code }`.
```

Replace it with the following, then a blank line, then the same `Exit:` text again, so the new paragraphs sit directly above the `Exit:` paragraph:

```
Marks and the tap: after a chunk is queued for the window, the reader thread also gives the same bytes to the session's `tap::Tap` (`pty/tap.rs`), so the screen is never delayed. A `Tap` owns a `marks::Scanner` (`pty/marks.rs`), a pure byte state machine that carries its state across chunks, so a read that ends inside an escape sequence gives the same result as one that does not. The scanner reports `Mark::PromptStart` (`OSC 133;A`), `Mark::CommandEnd { exit }` (`OSC 133;D;<code>`, no code means 0), `Mark::Bell` (a BEL that does not end an OSC string) and, only while `TriggerService::wants_output()` is true, the complete printable lines with CSI, OSC and two byte escapes removed. Nothing is scanned for lines in a grid without an armed `tg.output-matched` rule. The tap turns marks into `InternalSignal`s (`CommandEnded` with a duration, `Bell`, `OutputLine` cut to 200 characters) and the reader thread hands each to `TriggerService::publish_internal` (9.2). The webview keeps its own OSC parsing; nothing in the frontend changed.

Command timing is measured in Rust by a `CommandClock` shared by the reader thread and `PtyManager::write`. A command starts at the first `write` containing `\r` after a prompt mark (or after the last command end) and ends at the next `CommandEnd`. `write` locks the clock while it holds the sessions lock, so the reader thread locks the clock for one statement at a time and never while it asks for the sessions lock (it needs the working directory of the pane for the signal).
```

3. Find this text at the end of the 5.2 paragraph:

```
Covers the case where the agent exits so quickly between ticks that the watcher misses it.
```

Replace it with:

```
Covers the case where the agent exits so quickly between ticks that the watcher misses it. The Rust side also reads `OSC 133;D` (4.3), but only to publish `tg.command-failed` and `tg.command-slow` (9.2); agent detection does not use it.
```

4. Find this text at the end of the 5.3 paragraph:

```
Codex and others may also ring.
```

Replace it with:

```
Codex and others may also ring. The Rust scanner also sees every bell and publishes `tg.bell` (9.2) for any pane, focused or not.
```

## Acceptance

- [ ] `grep -c "Marks and the tap: after a chunk is queued" docs/technical-design.md` prints `1`
- [ ] `grep -c "Command timing is measured in Rust" docs/technical-design.md` prints `1`
- [ ] `grep -c "agent detection does not use it" docs/technical-design.md` prints `1`
- [ ] `grep -c "publishes \`tg.bell\` (9.2)" docs/technical-design.md` prints `1`
- [ ] `grep -c "^Exit: the waiter thread emits" docs/technical-design.md` prints `1`
- [ ] `node plans/_tools/lint-cards.mjs 002-trigger-internal` prints `0 error(s)`
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/002-trigger-internal/tasks/`

## Out of scope

- Section 4.5 (T30, T31) and the new section 9.2 (T33).
- Rewriting the 4.3 coalescing or backpressure text, or the agent detection text of 5.
- Any code.

## If stuck

- If a find text is not found, run `grep -n "Exit: the waiter\|watcher misses it\|may also ring" docs/technical-design.md`, and edit the line you find in the same way. If a replacement is already present, make no change and note it in the Log.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-13 haiku: built, acceptance passed
- 2026-10-13 architect: added one sentence on T19's catch_unwind guard to the 4.3 paragraph.
- 2026-10-13 checker: pass
