---
id: T17
epic: 002-trigger-internal
title: Prove real zsh and bash output becomes trigger signals
status: done
depends_on: [T16, T12]
size: S
model: haiku
files:
  - src-tauri/src/pty/shell/live.rs
---

## Goal

Two pty tests start the real zsh and the real bash with the real launch args (the harness from T10 and T11), run a failing command and an `echo`, feed what the shell printed to a `Tap` in 7 byte chunks, and assert that the signals a trigger rule would receive come out: `CommandEnded` with exit code 1, `OutputLine` with the echoed text, and no `Bell`. This closes slice B: it proves the whole path from shell script to signal on a real shell, short of the Tauri window.

## Read first

- `src-tauri/src/pty/shell/live.rs` the file you append to (`Sandbox`, `run`, `find_shell`)
- `src-tauri/src/pty/tap.rs` `Tap::new` and `Tap::process` (T15)
- `src-tauri/src/triggers/internal/mod.rs:1-60` `InternalSignal` (T01)

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance.

2. In `src-tauri/src/pty/shell/live.rs`, find:

```rust
use std::path::{Path, PathBuf};
```

Replace with:

```rust
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
```

3. In the same file, find:

```rust
use super::*;
```

Replace with:

```rust
use super::*;
use crate::pty::tap::{CommandClock, Tap};
use crate::triggers::internal::InternalSignal;
```

4. Append this to the end of the file:

```rust

/// Feeds what the real shell printed to a `Tap` in odd sized chunks, the way
/// pty reads split it, and checks the signals the triggers would get.
fn check_signals(name: &str) {
    let Some(shell) = find_shell(name) else {
        eprintln!("skipping: {name} is not installed");
        return;
    };
    let sandbox = Sandbox::new();
    let out = run(&shell, &sandbox, "false\necho needle-line\nexit\n");

    let clock = Arc::new(Mutex::new(CommandClock::default()));
    let mut tap = Tap::new("p1".to_string(), clock);
    let mut signals = Vec::new();
    for chunk in out.as_bytes().chunks(7) {
        signals.extend(tap.process(chunk, true, Instant::now(), |_| "/work".to_string()));
    }
    assert!(
        signals
            .iter()
            .any(|s| matches!(s, InternalSignal::CommandEnded { exit_code: 1, .. })),
        "no failed command in {signals:?} from {out:?}"
    );
    assert!(
        signals
            .iter()
            .any(|s| matches!(s, InternalSignal::OutputLine { line, .. } if line == "needle-line")),
        "no output line in {signals:?} from {out:?}"
    );
    assert!(!signals
        .iter()
        .any(|s| matches!(s, InternalSignal::Bell { .. })));
}

#[test]
fn zsh_output_becomes_trigger_signals() {
    check_signals("zsh");
}

#[test]
fn bash_output_becomes_trigger_signals() {
    check_signals("bash");
}
```

5. Run `cd src-tauri && cargo fmt`. If it changes the code above, keep the result.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid pty::shell::live` passes and lists `zsh_output_becomes_trigger_signals` and `bash_output_becomes_trigger_signals` as ok
- [ ] `cd src-tauri && cargo test -p terminal-grid pty::shell::live -- --nocapture 2>&1 | grep -c skipping` prints `0` on a machine that has bash and zsh
- [ ] `cd src-tauri && cargo test -p terminal-grid pty::` passes
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one
- [ ] `npm test` passes and `npm run build` exits 0 (nothing in `src/` changed in this slice, so this confirms it)
- [ ] `git status --porcelain | grep -v 'plans/002-trigger-internal/tasks/'` lists only paths under `files:`

## Out of scope

- Testing durations against real time. `CommandClock` is tested with fixed instants in `tap.rs`.
- Testing PowerShell. There is no `pwsh` on the build machines.
- Changing `Tap`, `Scanner`, the scripts or `mod.rs`.
- Editing docs. Fixing existing clippy warnings or renaming anything.

## If stuck

- If `use crate::pty::tap::{CommandClock, Tap}` does not resolve, T15 is not done: set `status: blocked`.
- If a test fails with `no failed command`, print the shell output shown in the message into the Log. If it holds `133;D;1` but the test still fails, the scanner dropped it: set `status: blocked`. If it holds no `133;D`, T10 to T12 are not done or not working: set `status: blocked`.
- If a test fails with `no output line` and the output holds `needle-line` only after a `%` or `$` prompt on the same line, set `status: blocked` and put the output in the Log. Do not change the expected line.
- If `cargo fmt` changes code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (clippy baseline 7, after 7; live.rs file changed on disk mid-run, my edits were intact)
- 2026-10-12 checker: pass
