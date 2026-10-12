---
id: T13
epic: 002-trigger-internal
title: Report the native exit code from the PowerShell prompt
status: done
depends_on: [T12]
size: S
model: haiku
files:
  - src-tauri/resources/shell-integration.ps1
  - src-tauri/src/pty/shell.rs
---

## Goal

The PowerShell prompt wrapper sends the real exit code of a failed native command (`$LASTEXITCODE`) in `OSC 133;D;<code>`, instead of always 1. Without this a rule with `exit_code = 2` can never match on PowerShell. This cannot be run on the machines that build cards (no `pwsh`), so it is checked by grep and by a Rust test that reads the script.

## Read first

- `plans/002-trigger-internal/epic.md` Decision 7 (PowerShell)
- `src-tauri/resources/shell-integration.ps1` the whole file (35 lines)
- `plans/002-trigger-internal/research/R01-feeds-and-shell-integration.md` implication 3

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance.

2. In `src-tauri/resources/shell-integration.ps1`, find exactly:

```powershell
$global:__tg_original_prompt = $function:prompt
$global:__tg_last_history_id = -1

function global:prompt {
    $exit = if ($?) { 0 } else { 1 }
```

Replace with exactly (the rest of the file stays as it is, starting at the line `    $esc = [char]27; $bel = [char]7`):

```powershell
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

3. In `src-tauri/src/pty/shell.rs`, in `mod tests`, directly above `    #[test]` + `    fn cmd_launches_bare() {`, insert:

```rust
    #[test]
    fn powershell_script_reports_the_native_exit_code() {
        let script = include_str!("../../resources/shell-integration.ps1");
        assert!(script.contains("$global:LASTEXITCODE"));
        // $? only means something if nothing runs before it is read.
        let body = script.split("function global:prompt {").nth(1).unwrap();
        let first = body
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#'))
            .unwrap();
        assert_eq!(first, "$ok = $?");
    }

```

4. Run `cd src-tauri && cargo fmt`. If it changes the code above, keep the result.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid pty::shell::tests::powershell_script_reports_the_native_exit_code` shows `1 passed`
- [ ] `grep -c 'LASTEXITCODE' src-tauri/resources/shell-integration.ps1` prints `4`
- [ ] `grep -n 'ok = \$?' src-tauri/resources/shell-integration.ps1` prints one line, number 16
- [ ] `grep -c 'if (\$?) { 0 } else { 1 }' src-tauri/resources/shell-integration.ps1` prints `0`
- [ ] `grep -c 'Write-Host -NoNewline "\$esc\]133;D;\$exit\$bel"' src-tauri/resources/shell-integration.ps1` prints `1` (the line that sends the code is unchanged)
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one
- [ ] `git status --porcelain | grep -v 'plans/002-trigger-internal/tasks/'` lists only paths under `files:`
- [ ] Not runnable here: on a Windows machine with pwsh, `cmd /c exit 3` then pressing Enter should send `133;D;3`, and `Get-Item nope` should send `133;D;1`. Write in the Log that this was not run.

## Out of scope

- Running or installing PowerShell.
- Sending durations, `133;C` or anything else new from the script. Rust times commands (T15).
- Changing the history id check, the profile loading or the encoding lines in the script.
- Editing docs or the frontend. Fixing existing clippy warnings.

## If stuck

- If the grep count of `LASTEXITCODE` is not 4, the replacement text differs from step 2: compare it character by character.
- If the Rust test fails on `first`, something was inserted above `$ok = $?` in the function: move it below.
- If the script file has Windows line endings and the find in step 2 does not match, match line by line and keep the file's line endings.
- If `cargo fmt` changes code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed. Clippy baseline 7 warnings, after 7. Manual pwsh check (`cmd /c exit 3` gives 133;D;3, `Get-Item nope` gives 133;D;1) not run: no pwsh on this machine.
- 2026-10-12 checker: pass
