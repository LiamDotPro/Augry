---
id: T34
epic: 002-trigger-internal
title: Design sections 2 and 14, layout and testing
status: todo
depends_on: [T33]
size: S
model: haiku
files:
  - docs/technical-design.md
---

## Goal

The repository layout in section 2 of `docs/technical-design.md` lists the new `pty/` and `triggers/` files and the wrapper files in `resources/`, and section 14 says how the internal source and the shell integration are tested.

## Read first

- `docs/technical-design.md:60-92` the layout block in section 2
- `docs/technical-design.md:736-746` the trigger testing text in section 14
- `plans/002-trigger-internal/epic.md` the Shape section, for file names

## Steps

1. All edits are in `docs/technical-design.md`. Use exact find and replace, in this order.

2. Find this line (in the layout block, 8 spaces of indent):

```
        reader.rs           # reader thread, chunk coalescing, event emission
```

Replace it with these four lines:

```
        reader.rs           # reader thread, chunk coalescing, event emission
        marks.rs            # Scanner: OSC 133 marks, bell and printable lines from a shell's bytes (pure, tested)
        tap.rs              # CommandClock and Tap: marks to InternalSignals for the tg source
        shell/live.rs       # pty tests that start the real zsh and bash (unix only)
```

3. Find this line:

```
        status.rs           # the triggers_status answer and the trigger://fired payload
```

Replace it with these three lines:

```
        status.rs           # the triggers_status answer and the trigger://fired payload
        feeds.rs            # publish() and the small builders the feed points call (9.2)
        internal/           # the tg source: mod.rs, kinds.rs, rule.rs, logic.rs, README.md (9.2)
```

4. Find these two lines:

```
      shell-integration.ps1 # bundled, copied to app data dir on first run
      shell-integration.sh
```

Replace them with:

```
      shell-integration.ps1 # bundled, copied to app data dir on first run
      shell-integration.sh
      bashrc                # wrapper for bash --rcfile, installed beside the scripts
      zsh/                  # .zshenv .zprofile .zshrc .zlogin, the ZDOTDIR wrapper (4.5)
```

5. In section 14, find the whole line that starts `- The app side (`src-tauri/src/triggers/`): definition parsing`. Keep it unchanged and add these bullets on new lines directly after it:

```
- The internal source (`src-tauri/src/triggers/internal/`): `rule.rs` parsing per kind, `logic::step` over every kind (agent glob, exit code and duration filters, the 200 character cut, note glob, `repo`), `InternalSource` start, publish and drop, `sources()` and `publish_internal` with a `tg.bell` rule, and a unit test that fails while the Kinds table in its `README.md` is stale.
- `pty::marks` and `pty::tap`: the scanner over escape sequences however the chunks are cut, bell versus the end of an OSC string, exit codes, line stripping, and the command clock (start at the first Enter after a prompt, end at the next command end).
- Shell integration, in `src-tauri/src/pty/shell/live.rs` (unix only): the real zsh and the real bash are started under a pty with the app's real launch arguments and variables, in a fake home. The tests assert that `OSC 133;D` appears, that a variable set in the user's own rc file is visible, that a repeated command is reported each time, and that a failing command and an `echo` give the signals a rule would receive. A shell that is not installed makes its test skip. The PowerShell script cannot run on these machines, so a test only reads it.
- The feed points: one end to end test in `triggers/feeds.rs` arms a rule for each kind the app feeds from its own code and checks each fires with its payload.
```

6. Find the line that starts `Manual check for triggers:` and add this paragraph on a new line directly after it, with a blank line between:

```
Manual check for the internal source: in a process definition add a `tg.command-failed` trigger with no `when`, arm it, open a pane, run `false`, and confirm the same toast appears with the kind `tg.command-failed`. Do the same in a zsh pane and a bash pane.
```

## Acceptance

- [ ] `grep -c "marks.rs            # Scanner" docs/technical-design.md` prints `1`
- [ ] `grep -c "internal/           # the tg source" docs/technical-design.md` prints `1`
- [ ] `grep -c "zsh/                  # .zshenv" docs/technical-design.md` prints `1`
- [ ] `grep -c "^- The internal source" docs/technical-design.md` prints `1`
- [ ] `grep -c "^Manual check for the internal source" docs/technical-design.md` prints `1`
- [ ] `node plans/_tools/lint-cards.mjs 002-trigger-internal` prints `0 error(s)`
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/002-trigger-internal/tasks/`

## Out of scope

- Rewriting existing testing bullets or the CI paragraph.
- Sections 4, 5 and 9 (T30 to T33).
- Any code.

## If stuck

- If a layout line is not found exactly, run `grep -n "reader.rs\|status.rs\|shell-integration" docs/technical-design.md` and make the same insert after the line you find, keeping its indent.
- If a replacement is already present, make no change to it and note it in the Log.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
