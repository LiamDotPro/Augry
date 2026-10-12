---
id: T31
epic: 002-trigger-internal
title: Design 4.5, how each shell loads the script
status: todo
depends_on: [T30]
size: S
model: haiku
files:
  - docs/technical-design.md
---

## Goal

Section 4.5 of `docs/technical-design.md` describes how zsh, bash, `sh` and PowerShell load the integration, says that bash and zsh integration now really runs (it never did before), and explains how repeated commands are detected.

## Read first

- `docs/technical-design.md:262-310` section 4.5, after T30
- `plans/002-trigger-internal/tasks/T10-zsh-wrapper.md` steps 2 and 3, the zsh wrapper files and the env
- `plans/002-trigger-internal/tasks/T11-bash-rcfile.md` steps 2 and 3, the bash wrapper and the `sh` rule
- `plans/002-trigger-internal/tasks/T12-history-number.md` step 2, the script

## Steps

1. All edits are in `docs/technical-design.md`. Find this one paragraph (a single line):

```
For bash/zsh the `.sh` file does the same with `PROMPT_COMMAND` / `precmd`. cmd.exe gets no integration; the process tree watcher still works there, cwd stays whatever was set at creation unless the user changes it from the pane header.
```

2. Replace it with the following text, exactly. The table and the paragraphs go in this order.

```
For bash and zsh the `.sh` file does the same with `PROMPT_COMMAND` / `precmd_functions`. How each shell loads it:

| Shell | Launch | How the script is loaded |
|---|---|---|
| zsh | `zsh -i`, with `ZDOTDIR=<app_data>/zsh`, `TG_USER_ZDOTDIR` (the user's own `ZDOTDIR`, or `$HOME`) and `TG_SHELL_INTEGRATION` (path of the script) set in the child's environment | The VS Code approach. The four wrapper files `.zshenv`, `.zprofile`, `.zshrc` and `.zlogin` each source the user's own file of the same name with `ZDOTDIR` pointing at the user's folder, then put `ZDOTDIR` back. `.zshrc` then sources the script. A user `.zshenv` that moves `ZDOTDIR` is followed. The last wrapper step hands `ZDOTDIR` back to the user. zsh is not started as a login shell |
| bash | `bash --rcfile <app_data>/bashrc -i` | The wrapper `bashrc` sources `~/.bashrc` when it exists, then the script. `-i` is needed because `--rcfile` is only read by interactive shells |
| sh | no arguments | No integration. `sh` is classed as bash for the frontend, but it may be dash, which has no `--rcfile` |
| PowerShell | as above | The script dot sources the profile, then wraps `prompt` |
| cmd.exe | none | No integration |

Until this change bash and zsh integration never ran. The old launch was `-c '. script; exec <shell> -i'`, and `exec` replaced the shell that had sourced the script with a fresh one, so the functions and hooks the script defined were gone. Now the script is loaded by the shell that stays, and a pty test per shell (`pty/shell/live.rs`) starts the real zsh or bash with the real arguments and variables in a fake home, and asserts that `OSC 133;D` appears and that a variable set in the user's own rc file is visible. A shell that is not installed makes its test skip.

The script reports a command once per command, by history number and not by command text, so running `make` twice reports twice. zsh compares `$HISTCMD`. bash cannot: `$HISTCMD` reads `1` inside `PROMPT_COMMAND` on the bash 3.2 that macOS ships, so bash compares the number that `history 1` prints. The first prompt only records the number, because the history file was loaded and nothing ran. After each reported command the script prints `OSC 7777` (the command text in base64), `OSC 133;D;<code>`, then at every prompt `OSC 7` and `OSC 133;A`. Commands the shell does not record in its history are not reported; see 9.2.

cmd.exe gets no integration; the process tree watcher still works there, cwd stays whatever was set at creation unless the user changes it from the pane header.
```

## Acceptance

- [ ] `grep -c "Until this change bash and zsh integration never ran" docs/technical-design.md` prints `1`
- [ ] `grep -c "ZDOTDIR=<app_data>/zsh" docs/technical-design.md` prints `1`
- [ ] `grep -c "bash --rcfile <app_data>/bashrc -i" docs/technical-design.md` prints `1`
- [ ] `grep -c "history 1" docs/technical-design.md` prints `1`
- [ ] `grep -c "^For bash/zsh" docs/technical-design.md` prints `0`
- [ ] `node plans/_tools/lint-cards.mjs 002-trigger-internal` prints `0 error(s)`
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/002-trigger-internal/tasks/`

## Out of scope

- The "9.2" section the text points to: T33 adds it.
- Any code, script or wrapper file.
- Editing the PowerShell part of 4.5 (T30).

## If stuck

- If the paragraph in step 1 is not found, run `grep -n "For bash" docs/technical-design.md`. If the line differs only by a typo or spacing, replace that line. If it already reads `For bash and zsh the`, make no change and note it in the Log.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
