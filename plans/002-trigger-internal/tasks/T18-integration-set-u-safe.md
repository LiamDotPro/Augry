---
id: T18
epic: 002-trigger-internal
title: Make shell-integration.sh safe under set -u
status: done
depends_on: [T17]
size: S
model: haiku
files:
  - src-tauri/resources/shell-integration.sh
---

## Goal

A user whose `~/.bashrc` or `~/.zshrc` runs `set -u` still gets a working
prompt. Today the script reads `$ZSH_VERSION`, `$__tg_seen` and
`$__tg_last_number` without a default, so bash stops with
`ZSH_VERSION: unbound variable` and every prompt prints an error. Only the
way those three names are read changes; behaviour is the same.

## Read first

- `src-tauri/resources/shell-integration.sh` (29 lines, the file you edit)

## Steps

1. In `src-tauri/resources/shell-integration.sh`, replace every
   `"$ZSH_VERSION"` with `"${ZSH_VERSION:-}"`. There are exactly three: the
   `if` on line 6, the `if` inside the `elif` branch on line 18, and the
   `if` on line 25.
2. In the same file, replace `[ -z "$__tg_seen" ]` with
   `[ -z "${__tg_seen:-}" ]`.
3. In the same file, replace `[ "$number" != "$__tg_last_number" ]` with
   `[ "$number" != "${__tg_last_number:-}" ]`.
4. Change nothing else. `git diff` shows exactly five changed lines.

## Acceptance

- [ ] `grep -c '"\$ZSH_VERSION"' src-tauri/resources/shell-integration.sh` prints `0`
- [ ] `grep -cF '${ZSH_VERSION:-}' src-tauri/resources/shell-integration.sh` prints `3`
- [ ] `git diff --numstat -- src-tauri/resources/shell-integration.sh` prints `5	5	src-tauri/resources/shell-integration.sh`
- [ ] `bash -c 'set -u; source src-tauri/resources/shell-integration.sh; __tg_precmd; __tg_precmd; echo SAFE' | tail -c 5` prints `SAFE`, and the command prints nothing on stderr
- [ ] `zsh -c 'set -u; source src-tauri/resources/shell-integration.sh; __tg_precmd; __tg_precmd; echo SAFE' | tail -c 5` prints `SAFE`, and the command prints nothing on stderr
- [ ] `cd src-tauri && cargo test -p terminal-grid pty::shell` passes, and `cargo test -p terminal-grid pty::shell::live -- --nocapture | grep -c skipping` prints `0`
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `git status --porcelain | grep -v plans/002-trigger-internal/tasks/` lists only the path under `files:`

## Out of scope

- Any other change to the script, including `$PWD`, `$last`, `$number` and `PROMPT_COMMAND`, which are already safe.
- `shell-integration.ps1` and the zsh and bash wrapper files.
- Adding a Rust test for `set -u`.

## If stuck

- If a `set -u` acceptance command prints `unbound variable`, quote the line in the Log and set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: blocked. Five edits applied to src-tauri/resources/shell-integration.sh (lines 6, 13, 16, 18, 25); numstat 5 5 passes. Acceptance check `grep -c '${ZSH_VERSION:-}' src-tauri/resources/shell-integration.sh` prints 0 on this macOS grep, not 3 as the card says (`grep -cF` gives 3, and the file has exactly 3 occurrences on lines 6, 18, 25). Card check is wrong, so stopped per "If stuck". Passed: bash and zsh `set -u` runs print SAFE with 0 bytes on stderr. Not run: cargo tests, git status check.
- 2026-10-12 architect: the BRE grep for ${ZSH_VERSION:-} does not match on this grep; changed the check to grep -cF. The edits stand; sent to the checker, which runs the remaining checks.
- 2026-10-12 checker: pass
