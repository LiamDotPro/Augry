---
id: T12
epic: 002-trigger-internal
title: Count commands by history number in shell-integration.sh
status: todo
depends_on: [T11]
size: S
model: haiku
files:
  - src-tauri/resources/shell-integration.sh
  - src-tauri/src/pty/shell/live.rs
---

## Goal

Running the same command twice reports a command end twice. The script now compares the history number of the last command, not its text, so `make` twice (or `true` three times) gives three `OSC 133;D`. It also no longer reports a command at the first prompt: the last line of the history file was reported as if it had just run. zsh uses `$HISTCMD`. bash cannot: `$HISTCMD` reads `1` inside `PROMPT_COMMAND` on the bash 3.2 that macOS ships (checked), so bash takes the number that `history 1` prints.

## Read first

- `plans/002-trigger-internal/epic.md` Decision 7, last bullet
- `src-tauri/resources/shell-integration.sh` the whole file (19 lines)
- `plans/002-trigger-internal/research/R01-feeds-and-shell-integration.md` the "Evidence" bullet on `shell-integration.sh:1-19`
- `src-tauri/src/pty/shell/live.rs` the test harness from T10 and T11

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance.

2. Replace the whole content of `src-tauri/resources/shell-integration.sh` with exactly this:

```sh
# Terminal Grid shell integration for bash and zsh. Sourced at startup.
__tg_osc() { printf '\033]%s\007' "$1"; }
__tg_precmd() {
  local exit=$?
  local number last
  if [ -n "$ZSH_VERSION" ]; then
    number=$HISTCMD
  else
    # $HISTCMD reads 1 inside PROMPT_COMMAND on bash 3.2, so take the number `history 1` prints.
    read -r number last <<< "$(HISTTIMEFORMAT= history 1)"
  fi
  # The first prompt only records the number: the history file was loaded, not run.
  if [ -z "$__tg_seen" ]; then
    __tg_seen=1
    __tg_last_number=$number
  elif [ "$number" != "$__tg_last_number" ]; then
    __tg_last_number=$number
    if [ -n "$ZSH_VERSION" ]; then last=$(fc -ln -1 2>/dev/null); fi
    __tg_osc "7777;cmd;$(printf '%s' "$last" | base64 | tr -d '\n')"
    __tg_osc "133;D;$exit"
  fi
  __tg_osc "7;file://localhost$PWD"
  __tg_osc "133;A"
}
if [ -n "$ZSH_VERSION" ]; then
  precmd_functions+=(__tg_precmd)
else
  PROMPT_COMMAND="__tg_precmd${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
fi
```

3. Append this to the end of `src-tauri/src/pty/shell/live.rs`:

```rust

/// Runs `true` three times and `false` once in the named shell, with an old command
/// already in its history file.
fn check_repeats(name: &str, history_file: &str, history_text: &str) {
    let Some(shell) = find_shell(name) else {
        eprintln!("skipping: {name} is not installed");
        return;
    };
    let sandbox = Sandbox::new();
    sandbox.write_home(history_file, history_text);
    let out = run(&shell, &sandbox, "true\ntrue\ntrue\nfalse\nexit\n");
    assert_eq!(out.matches("\x1b]133;D;0\x07").count(), 3, "{out:?}");
    assert_eq!(out.matches("\x1b]133;D;1\x07").count(), 1, "{out:?}");
    // The history file's old command was not run in this shell.
    assert_eq!(out.matches("\x1b]133;D;").count(), 4, "{out:?}");
}

#[test]
fn zsh_reports_a_repeated_command_each_time() {
    check_repeats("zsh", ".zsh_history", ": 1700000000:0;echo old\n");
}

#[test]
fn bash_reports_a_repeated_command_each_time() {
    check_repeats("bash", ".bash_history", "echo old\n");
}
```

4. Run `cd src-tauri && cargo fmt`. If it changes the code above, keep the result.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid pty::shell::live` passes and lists `zsh_reports_a_repeated_command_each_time` and `bash_reports_a_repeated_command_each_time` as ok
- [ ] `grep -c 'HISTCMD' src-tauri/resources/shell-integration.sh` prints `2`
- [ ] `grep -c '__tg_last_cmd' src-tauri/resources/shell-integration.sh` prints `0`
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one
- [ ] `git status --porcelain | grep -v 'plans/002-trigger-internal/tasks/'` lists only paths under `files:`

## Out of scope

- PowerShell (T13), which has its own history id check.
- Handling `HISTCONTROL=ignoredups` or `ignorespace`: a command the shell does not record gets no history number, so it is not reported. That is a known limit, not a bug to fix here.
- Adding durations or `133;C` to the script. Rust times commands (T15).
- Changing the wrapper files, `shell.rs`, docs or the frontend.
- Fixing existing clippy warnings or renaming anything.

## If stuck

- If a repeat test fails with a count of 2 instead of 3, the second `true` was not reported: check that step 2 was copied exactly, in particular `read -r number last <<< "$(HISTTIMEFORMAT= history 1)"`, then run the test again.
- If a repeat test counts one more `133;D;` than expected, the first prompt reported the old history line: check the `__tg_seen` block.
- If `cargo test` fails only on a machine whose default shell setup sets `HISTCONTROL` or `setopt HIST_IGNORE_DUPS` globally in `/etc`, copy the printed output into the Log and set `status: blocked`.
- If `cargo fmt` changes code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
