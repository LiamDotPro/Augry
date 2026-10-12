---
id: T08
epic: 002-trigger-internal
title: Register the tg source and add publish_internal
status: done
depends_on: [T07]
size: M
model: haiku
files:
  - src-tauri/src/triggers/mod.rs
---

## Goal

`sources()` boxes one `InternalSource` clone for the hub, `TriggerService` keeps another, and `TriggerService::publish_internal(signal)` and `TriggerService::wants_output()` exist for the feeds in slices B and C. A test arms a `tg.bell` rule from a definition file and sees the fired payload come out of the forwarding thread.

## Read first

- `plans/002-trigger-internal/epic.md` Decision 10
- `src-tauri/src/triggers/mod.rs:1-100` `sources()`, `TriggerService`, `new` and `with_hub`
- `src-tauri/src/triggers/mod.rs:255-300` the tests module and its `service()` helper
- `src-tauri/src/triggers/internal/mod.rs` `InternalSource` (T07)

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance (it was 7 when this card was written).

2. All edits are in `src-tauri/src/triggers/mod.rs`. Make them in this order.

3. Find the line `use serde::Serialize;` and add this line directly above it:

```rust
use internal::{InternalSignal, InternalSource};
```

4. Replace the whole `sources()` function, including its doc comment lines and the `#[allow(clippy::vec_init_then_push)]` attribute, with:

```rust
/// The sources this build runs. A new source crate adds one line here, behind
/// its own cargo feature. The internal source is always on: it is not a crate.
#[allow(clippy::vec_init_then_push)]
fn sources(internal: &InternalSource) -> Vec<Box<dyn Source>> {
    let mut list: Vec<Box<dyn Source>> = Vec::new();
    list.push(Box::new(internal.clone()));
    #[cfg(feature = "trigger-fs")]
    list.push(Box::new(trigger_fs::FsSource));
    list
}
```

5. In `pub struct TriggerService`, add this field directly after `hub: Hub,`:

```rust
    /// The `tg` source. The hub runs one clone; this one is how the app feeds it.
    internal: InternalSource,
```

6. Replace `new` and `with_hub` (the two functions directly after `impl TriggerService {`) with:

```rust
    pub fn new(dir: PathBuf) -> TriggerService {
        let internal = InternalSource::default();
        let hub = Hub::new(sources(&internal));
        TriggerService::with_hub(dir, hub, internal)
    }

    fn with_hub(dir: PathBuf, hub: Hub, internal: InternalSource) -> TriggerService {
        TriggerService {
            hub,
            internal,
            dir,
            names: Arc::new(Mutex::new(BTreeMap::new())),
            inner: Mutex::new(Inner::default()),
        }
    }
```

7. Directly above `pub fn check(&self, rule: &Rule)` (and below `catalogue`), add:

```rust
    /// Hands a signal from inside the app to the `tg` source, which fires the
    /// armed rules it matches. Cheap when no rule is armed. Call it after the
    /// work it reports has succeeded.
    pub fn publish_internal(&self, signal: InternalSignal) {
        self.internal.publish(&signal);
    }

    /// True while a `tg.output-matched` rule is armed, so the terminal reader
    /// only assembles lines when someone is listening.
    pub fn wants_output(&self) -> bool {
        self.internal.wants_output()
    }

```

8. In the tests module, in the helper `fn service()`, change the line `TriggerService::with_hub(dir.clone(), Hub::new(Vec::new())),` to:

```rust
            TriggerService::with_hub(dir.clone(), Hub::new(Vec::new()), InternalSource::default()),
```

9. In the tests module, directly above `#[test]` of `fn has_definition_looks_for_the_file`, add this test:

```rust
    #[test]
    fn a_published_signal_fires_an_armed_tg_rule() {
        let dir = std::env::temp_dir().join(format!("tg-triggers-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("ring.json"),
            r#"{"schemaVersion":1,"name":"Ring","triggers":[
                {"id":"bell","kind":"tg.bell","when":{}}]}"#,
        )
        .unwrap();
        let service = TriggerService::new(dir.clone());
        let status = service.refresh(&config(&["ring"]), &ctx());
        assert!(matches!(
            status.processes[0].rules[0].state,
            status::RuleStatusState::Active
        ));
        let (tx, rx) = channel();
        service.start_forwarding(move |payload| {
            let _ = tx.send(payload);
        });
        service.publish_internal(InternalSignal::Bell { pane: "p1".into() });
        let payload = rx.recv_timeout(Duration::from_secs(5)).expect("a payload");
        assert_eq!(payload.process_id, "ring");
        assert_eq!(payload.rule_id, "ring/bell");
        assert!(!service.wants_output());
        cleanup(&dir);
    }

```

10. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::internal` passes and reports 43 passed
- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::tests::a_published_signal_fires_an_armed_tg_rule` shows `1 passed`
- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::` passes with no failures
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Calling `publish_internal` from anywhere else. The feeds are slices B and C.
- Splitting `triggers/mod.rs` to shorten it. It was already near 400 lines and the rule is for the new `internal/` files.
- Making the internal source optional behind a cargo feature.
- Fixing existing clippy warnings, renaming anything, or changing a file not listed under `files:`.

## If stuck

- If `cargo check --no-default-features` fails with an unused import or `unused_mut`, the `#[allow(clippy::vec_init_then_push)]` line was dropped from `sources()`; put it back as given.
- If the new test times out, the hub did not start the source: run `cargo test -p terminal-grid triggers::internal` and, if that passes, set `status: blocked` with the status state printed by the first assert.
- If `cargo fmt` changes the code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (clippy baseline 7 warnings, after 7)
- 2026-10-12 checker: pass
