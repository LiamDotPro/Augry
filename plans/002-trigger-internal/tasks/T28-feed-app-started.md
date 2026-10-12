---
id: T28
epic: 002-trigger-internal
title: Publish tg.app-started after the first refresh
status: done
depends_on: [T20]
size: S
model: haiku
files:
  - src-tauri/src/triggers/mod.rs
---

## Goal

`triggers::start` publishes `InternalSignal::AppStarted` once, after the first `refresh_now` has armed the rules. A test shows the signal reaches nothing before the first refresh and fires the `tg.app-started` rule after it.

## Read first

- `plans/002-trigger-internal/epic.md` Decision 12 ("app start after `triggers::start`")
- `src-tauri/src/triggers/mod.rs` the function `start` (search for `pub fn start(app: &AppHandle)`) and `refresh_now`
- `src-tauri/src/triggers/mod.rs` the tests module: `ctx()`, `config()`, `cleanup()` and the test `a_published_signal_fires_an_armed_tg_rule` (T08), which this card's test copies the shape of
- `src-tauri/src/lib.rs:95-100` where `triggers::start(&handle)` is called in setup

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance (it was 7 when this card was written).

2. All edits are in `src-tauri/src/triggers/mod.rs`. In `start`, find these three consecutive lines at the end of the function:

```rust
    });
    refresh_app(app);
}
```

and replace them with the following. How it works: `refresh_app` only spawns a thread and returns, so a signal published right after it could run before any rule is armed and be lost. The spawned closure refreshes first and publishes second, on the same blocking thread. `InternalSignal` is already imported at the top of the file (T08).

```rust
    });
    // `tg.app-started` goes out only after the first refresh has armed the
    // rules; published sooner, nothing would be listening.
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<crate::AppState>();
        refresh_now(&state);
        state.triggers.publish_internal(InternalSignal::AppStarted);
    });
}
```

3. In the `tests` module, find the two lines

```rust
    #[test]
    fn has_definition_looks_for_the_file() {
```

and put this test directly above them:

```rust
    #[test]
    fn app_started_fires_only_once_the_rules_are_armed() {
        let dir = std::env::temp_dir().join(format!("tg-triggers-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("hello.json"),
            r#"{"schemaVersion":1,"name":"Hello","triggers":[
                {"id":"started","kind":"tg.app-started","when":{}}]}"#,
        )
        .unwrap();
        let service = TriggerService::new(dir.clone());
        let (tx, rx) = channel();
        service.start_forwarding(move |payload| {
            let _ = tx.send(payload);
        });

        // Before the first refresh nothing is armed, so the signal goes nowhere.
        service.publish_internal(InternalSignal::AppStarted);
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());

        service.refresh(&config(&["hello"]), &ctx());
        service.publish_internal(InternalSignal::AppStarted);
        let payload = rx.recv_timeout(Duration::from_secs(5)).expect("a payload");
        assert_eq!(payload.rule_id, "hello/started");
        assert_eq!(payload.kind, "tg.app-started");
        assert!(payload.payload.is_empty());
        cleanup(&dir);
    }

```

4. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::tests::app_started_fires_only_once_the_rules_are_armed` shows `1 passed`
- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::` passes with no failures
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/002-trigger-internal/tasks/`
- [ ] `grep -c "AppStarted" src-tauri/src/triggers/mod.rs` prints `3` (the publish in `start` and two in the test)

## Out of scope

- Changing `refresh_app`, `refresh_now`, `lib.rs` or the setup order.
- Making the webview wait for the signal. The toast for `tg.app-started` can arrive before the webview is listening; that is a known limit, not a bug in this card.
- Fixing existing clippy warnings, renaming anything, or changing a file not listed under `files:`.

## If stuck

- If the three quoted lines are not at the end of `start`, set `status: blocked` and quote the function.
- If the test fails because the first `recv_timeout` returned a payload, the hub started the source with no rules: set `status: blocked` and quote the assert.
- If `InternalSignal` is not found in `triggers/mod.rs`, T08 is not applied: set `status: blocked`.
- If `cargo fmt` changes the code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-13 haiku: built, acceptance passed (clippy baseline 7, after 7)
- 2026-10-13 checker: pass
