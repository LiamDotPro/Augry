# trigger-core

The contract between trigger sources and whatever runs them, plus a small
test kit. A source crate depends on this crate only. Design:
`docs/process-triggers.md` Part 2.

## The contract

- `Source`: `id`, `kinds`, `availability`, `check(rule)` and
  `start(rules, sink) -> Handle`. Dropping the `Handle` stops the source.
- `Rule`: `{ id, kind, when }` where `when` is a map of JSON values.
- `Event`: `{ rule, kind, at, payload }`. `at` is whole milliseconds since the
  Unix epoch in JSON. `Sink` is a `std::sync::mpsc::Sender<Event>`.
- `KindInfo` and `FieldInfo` describe a kind: platforms (`Platforms`, a bit set
  that serialises as `["macos","windows","linux"]`), the permission it needs
  and its `when` and `payload` fields.
- `Availability`: `Available`, `Unsupported { reason }` or
  `NeedsPermission { permission, how }`.
- `RuleError` and `SourceError` are the two error types a source returns.
- `kinds_markdown(kinds)` renders the kinds table. Each source crate's
  `tests/readme.rs` checks its README contains it.

A unit test pins the JSON the frontend reads, so changing a field name or a
serde attribute fails a test.

## Test kit

`trigger_core::testkit`, always compiled:

- `RecordingSink`: give `tx.clone()` to `Source::start`, then
  `wait_for(kind, timeout)` or `drain()`.
- `rule(id, kind, json!({...}))`: builds a `Rule`.
- `Clock`, `SystemClock`, `FixedClock`: time that tests control.
- `FakeSource`: a source that records the rules it was started with, can be
  made to fail in `start` (`fail_start`) and fires events with `fire`.

## Develop

Run from `src-tauri`:

- `cargo test -p trigger-core`
- `cargo clippy -p trigger-core --all-targets -- -D warnings`
- `cargo fmt -p trigger-core --check`

From the repo root: `node plans/_tools/check-crates.mjs`.
