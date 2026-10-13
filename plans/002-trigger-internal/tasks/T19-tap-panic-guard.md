---
id: T19
epic: 002-trigger-internal
title: Keep a pane's output flowing if the tap panics
status: done
depends_on: [T18]
size: S
model: haiku
files:
  - src-tauri/src/pty/mod.rs
---

## Goal

A bug in the trigger tap can never freeze a terminal. Today the reader thread
calls `publish_signals` with nothing around it. If that call panicked, the
thread would end without `pump.close()`, the emitter thread would wait forever
and the pane's output would stop. After this card the call runs inside
`std::panic::catch_unwind`; on a panic the reader stops tapping for that pane
and keeps forwarding output as before. No behaviour changes when nothing
panics.

## Read first

- `src-tauri/src/pty/mod.rs` `start_reader` (the reader thread loop, around
  lines 293 to 345) and `publish_signals` (around line 493)

## Steps

1. In `src-tauri/src/pty/mod.rs`, in `start_reader`, find this text (inside
   the first `std::thread::spawn`):

```rust
                let mut buffer = [0u8; reader::READ_BUFFER];
                loop {
```

   and replace it with:

```rust
                let mut buffer = [0u8; reader::READ_BUFFER];
                // Cleared if the tap ever panics, so output keeps flowing.
                let mut tapping = true;
                loop {
```

2. In the same function, find this text:

```rust
                            pump.push(&buffer[..count]);
                            publish_signals(&tap_app, &mut tap, &buffer[..count]);
```

   and replace it with:

```rust
                            pump.push(&buffer[..count]);
                            if tapping {
                                // A bug in the tap must never stop the pane's output.
                                let published =
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        publish_signals(&tap_app, &mut tap, &buffer[..count])
                                    }));
                                tapping = published.is_ok();
                            }
```

3. Change nothing else. Run `cd src-tauri && cargo fmt`, then the acceptance
   commands.

## Acceptance

- [ ] `grep -c "catch_unwind" src-tauri/src/pty/mod.rs` prints `1`
- [ ] `grep -c "let mut tapping = true;" src-tauri/src/pty/mod.rs` prints `1`
- [ ] `grep -c "publish_signals(&tap_app, &mut tap, &buffer\[..count\])" src-tauri/src/pty/mod.rs` prints `1`
- [ ] `cd src-tauri && cargo test -p terminal-grid pty::` passes and does not hang
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number before and after this card (record both in the Log)
- [ ] `git status --porcelain | grep -v plans/002-trigger-internal/tasks/` lists only the path under `files:`

## Out of scope

- Changing `publish_signals`, the tap, the pump or the emitter thread.
- Logging the panic. Rust's default panic hook already prints it.
- A test that forces a panic; the reader thread needs a real `AppHandle`.

## If stuck

- If a find text in step 1 or 2 is not in the file exactly once, set status blocked and quote what is there.
- If the compiler says the closure is not `UnwindSafe`, check `AssertUnwindSafe` wraps the closure as written; if it still fails, set status blocked and quote the error.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed. Clippy warnings (grep -c warning, all-targets) before 7, after 7. Grep checks ran with plain grep, no ugrep issue seen.
- 2026-10-12 checker: pass
