---
id: T30
epic: 002-trigger-internal
title: Design 4.5, intro and PowerShell exit code
status: done
depends_on: [T17, T29]
size: S
model: haiku
files:
  - docs/technical-design.md
---

## Goal

Section 4.5 of `docs/technical-design.md` says which files the app installs for shell integration and shows the PowerShell prompt wrapper as it now is, reporting `$LASTEXITCODE`. T31 documents the bash and zsh loading in the same section.

## Read first

- `docs/technical-design.md:262-300` section 4.5 as it is today
- `plans/002-trigger-internal/tasks/T13-powershell-exit-code.md` the exact script text this card copies into the doc
- `plans/002-trigger-internal/tasks/T10-zsh-wrapper.md` step 3f, how `install_scripts` writes the wrapper files

## Steps

1. All edits are in `docs/technical-design.md`. Use exact find and replace, in this order. Do not reflow or reword any other line.

2. Find this one line:

```
The app writes `shell-integration.ps1` (and `.sh`) into `app_data_dir()` on startup and launches the shell with it. PowerShell launch:
```

Replace it with:

```
The app writes `shell-integration.ps1` and `shell-integration.sh` into `app_data_dir()` on startup, together with the wrapper files `bashrc` and `zsh/.zshenv`, `zsh/.zprofile`, `zsh/.zshrc`, `zsh/.zlogin`, and launches the shell with them. The scripts are copied from the bundled `resources/` folder. The wrapper files are compiled into the binary with `include_str!` and written by the same `install_scripts` function, because Tauri's `resources/*` glob skips folders. Each file is removed before it is rewritten. The `Shell` struct that `resolve` returns has `program`, `kind`, `args` and `env`; `env` holds the variables the integration needs, and `PtyManager::spawn` sets them on the child. PowerShell launch:
```

3. Find exactly this block (inside the powershell code fence of 4.5):

```
$global:__tg_original_prompt = $function:prompt
$global:__tg_last_history_id = -1

function global:prompt {
    $exit = if ($?) { 0 } else { 1 }
```

Replace it with:

```
$global:__tg_original_prompt = $function:prompt
$global:__tg_last_history_id = -1
$global:__tg_last_error = $null

function global:prompt {
    # Read $? and $LASTEXITCODE first: any other statement changes them.
    $ok = $?
    $native = $global:LASTEXITCODE
    # A failed native command sets $LASTEXITCODE and adds no error record. A
    # failed cmdlet adds one, and then a stale $LASTEXITCODE must not be reported.
    $lastError = if ($global:Error.Count -gt 0) { $global:Error[0] } else { $null }
    $newError = -not [object]::ReferenceEquals($lastError, $global:__tg_last_error)
    $global:__tg_last_error = $lastError
    $exit = if ($ok) { 0 } elseif (-not $newError -and $native -is [int] -and $native -ne 0) { $native } else { 1 }
```

4. Find this line (the one directly after the closing fence of the powershell block):

```
The frontend registers OSC handlers on xterm's parser:
```

Replace it with:

```
The exit code in `OSC 133;D` is the native command's `$LASTEXITCODE` when the last command was a native program that failed, and 1 for any other failure, so a trigger rule with `exit_code = 2` can match on PowerShell. This was never run on macOS or Linux; the Rust test `powershell_script_reports_the_native_exit_code` only reads the script.

The frontend registers OSC handlers on xterm's parser:
```

## Acceptance

- [ ] `grep -c "compiled into the binary with \`include_str!\`" docs/technical-design.md` prints `1`
- [ ] `grep -c '\$ok = \$?' docs/technical-design.md` prints `1`
- [ ] `grep -c "__tg_last_error" docs/technical-design.md` prints `3`
- [ ] `grep -c 'exit = if (\$?) { 0 } else { 1 }' docs/technical-design.md` prints `0`
- [ ] `node plans/_tools/lint-cards.mjs 002-trigger-internal` prints `0 error(s)`
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/002-trigger-internal/tasks/`

## Out of scope

- The bash and zsh text of 4.5 (T31), sections 4.3, 5, 9 (T32, T33).
- Changing the script in `src-tauri/resources/`, or any code.
- Fixing other stale text in 4.5, such as the `-Command` launch line.

## If stuck

- If a find text is not found exactly, run `grep -n "shell-integration.ps1\|__tg_last_history_id\|frontend registers OSC" docs/technical-design.md`. If the doc already contains the replacement text, make no change and note it in the Log. Otherwise set `status: blocked`.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-13 haiku: built, acceptance passed. Four edits to docs/technical-design.md applied in order; lint-cards 0 error(s). Note: src-tauri/src/triggers/feeds.rs shows as modified in git status, not touched by this card.
- 2026-10-13 checker: pass
