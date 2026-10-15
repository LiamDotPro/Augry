---
id: R06
epic: 001-trigger-core
question: How do `thiserror` 2 and `serde` derive behave on a `&'static str` payload (`Permission::Folder`) and `SystemTime`, and does the `trigger-core/src/lib.rs` Shape compile as written?
status: answered
---

## Answer

The Shape does not compile as written: exactly one error, `Platforms` has no `Serialize` (needed by `KindInfo`). Everything else compiles, including `Folder(&'static str)`, `&'static [FieldInfo]`, `Copy` on `KindInfo`, `thiserror` 2 and `Box<dyn FnOnce() + Send>`.
`SystemTime` serialises as `{"secs_since_epoch":..,"nanos_since_epoch":..}`; use `#[serde(with = "ms_since_epoch")]` on `Event.at` to get a number of milliseconds.
Implement `Serialize` for `Platforms` by hand as `["macos","windows","linux"]`. `Event` needs no `Deserialize`. Clippy (`-D warnings`) also wants `impl Default for RecordingSink`.

## Evidence

Ran in a scratch crate (`mktemp -d`), editions 2021, pinned to the versions in `src-tauri/Cargo.lock`: serde 1.0.229, serde_json 1.0.151, thiserror 2.0.20 (the lock also holds thiserror 1.0.69 for other deps; the workspace dependency `thiserror = "2"` resolves to 2.0.20). Rust toolchain on this machine has clippy 1.99. Shape pasted with stub bodies filled minimally (`here()` via `cfg!`, `kinds_markdown` stub, testkit bodies).

First `cargo build` of the Shape: one error, `E0277 the trait bound Platforms: serde::Serialize is not satisfied` at `KindInfo.platforms`. Nothing else failed. After the fix, `cargo test -- --nocapture` printed:

```
KindInfo:     {"id":"fs.x","title":"T","summary":"s","platforms":["macos","linux"],"permission":{"folder":"Downloads"},"when":[{"name":"path","doc":"d","required":true}],"payload":[]}
Availability: {"state":"needsPermission","permission":{"folder":"Downloads"},"how":"x"}
Event (fix):  {"rule":"r","kind":"k","at":1760000000123,"payload":{}}
raw SystemTime: {"at":{"secs_since_epoch":1760000000,"nanos_since_epoch":123000000}}
Permission::Accessibility -> "accessibility"   ScreenRecording -> "screenRecording"
Platforms::ALL -> ["macos","windows","linux"]
Handle drop ran the stop closure; RuleError::Field display -> "field a: b"
```

`cargo clippy --all-targets -- -D warnings` on the filled in crate: one error, `new_without_default` on `RecordingSink::new`.

Answers to each sub question:
- `Folder(&'static str)`: `Serialize` derive works (serialising borrowed `&'static str` is fine; it only breaks for `Deserialize`). Unit variants serialise as strings, `Folder` as `{"folder":"Downloads"}`. The mix is valid but the frontend type is `string | { folder: string }`. `Copy, PartialEq, Eq` derive fine.
- `KindInfo` with `&'static [FieldInfo]`: derives `Serialize`, `Clone`, `Copy`, `Debug` fine once `Platforms: Serialize`. Usable in `static`/`const` items (tested).
- `Event`: no `Deserialize` needed. `Rule` is the only one that is deserialised (already derived). The hub and app only serialise events. `PartialEq` on `Event` works with `SystemTime`.
- `thiserror` 2: both enums compile as written, including `{0}` and named fields `{field}`.
- `Handle(Option<Box<dyn FnOnce() + Send>>)`: fine; `Drop` with `take()` compiles. `Source: Send + Sync` is satisfied by implementors holding `Mutex` fields. `Handle` is `Send` but not `Sync`, which is fine for a hub that owns it.

## Implications for the cards

1. In `trigger-core/Cargo.toml` use `serde.workspace = true`, `serde_json.workspace = true`, `thiserror.workspace = true`.
2. Add to `lib.rs`, after the `Platforms` struct (this is the verified code):

```rust
impl serde::Serialize for Platforms {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let all = [(Platforms::MACOS, "macos"), (Platforms::WINDOWS, "windows"), (Platforms::LINUX, "linux")];
        let names: Vec<&str> = all.iter().filter(|(p, _)| self.has(*p)).map(|(_, n)| *n).collect();
        let mut seq = s.serialize_seq(Some(names.len()))?;
        for n in names { seq.serialize_element(n)?; }
        seq.end()
    }
}

/// serde helper: SystemTime as whole milliseconds since the Unix epoch.
pub mod ms_since_epoch {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    pub fn serialize<S: serde::Serializer>(t: &SystemTime, s: S) -> Result<S::Ok, S::Error> {
        let ms = match t.duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_millis() as i64,
            Err(e) => -(e.duration().as_millis() as i64),
        };
        s.serialize_i64(ms)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<SystemTime, D::Error> {
        let ms = <i64 as serde::Deserialize>::deserialize(d)?;
        Ok(if ms >= 0 { UNIX_EPOCH + Duration::from_millis(ms as u64) } else { UNIX_EPOCH - Duration::from_millis(ms.unsigned_abs()) })
    }
}
```

3. In `Event`, change the field to:
```rust
    #[serde(with = "ms_since_epoch")]
    pub at: SystemTime,
```
   (`with` needs both `serialize` and `deserialize` only if `Deserialize` is derived; the pair above is kept so adding `Deserialize` later needs no change.) `Fired { event: Event, .. }` and the app's `trigger://fired` payload then carry `at` as a number. In `src/ipc/types.ts`: `at: number` (ms since epoch, usable with `new Date(at)`); `platforms: ("macos" | "windows" | "linux")[]`; `permission: "accessibility" | "screenRecording" | "location" | "calendar" | "reminders" | "microphone" | "camera" | "notifications" | { folder: string } | null`.
4. Add to `testkit.rs` (clippy `new_without_default`):
```rust
impl Default for RecordingSink { fn default() -> Self { Self::new() } }
```
5. `FakeSource::fire(&self, event)` needs somewhere to keep the `Sink` it was started with. The Shape has no such field, so add `pub sink: Mutex<Option<Sink>>` (set in `start`, used in `fire`). `Sender<Event>` is `Sync` on current Rust, so `FakeSource: Sync` holds.
6. Add a unit test in `trigger-core` asserting `serde_json::to_value(Platforms::ALL)` is `["macos","windows","linux"]`, `Event.at` is a JSON number, and `Permission::Folder("Downloads")` is `{"folder":"Downloads"}`, so the frontend contract is pinned.
7. Everything else in the Shape (all other derives, `thiserror` attributes, `Handle`, `Source`, `Sink`) can be copied unchanged.

## Doubts

- The scratch crate was a single crate, not the workspace, and only built on macOS; none of this code is platform specific except `Platforms::here()`, which was only compiled for macOS targets (the `cfg!` form compiles on all).
- `kinds_markdown` was a stub; its content is not covered here.
- Millisecond truncation drops sub millisecond precision of `at`; acceptable for a toast and log.
