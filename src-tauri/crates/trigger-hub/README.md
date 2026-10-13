# trigger-hub

Runs trigger sources: keeps the rule registry, starts and stops sources so
that exactly the applied rules are live, cools down noisy rules and fans fired
events out to subscribers. It depends on `trigger-core` only. Design:
`docs/process-triggers.md` Part 2.

## What it does

- `Hub::new(sources)`: takes the sources. It starts one thread, named
  `trigger-hub`, that stops when the `Hub` is dropped. Every method takes
  `&self`, and `Hub` is `Send + Sync`.
- `catalogue()`: every source with its kinds and availability.
- `check(rule)`: the source that owns the rule asks whether the rule is valid.
  A rule belongs to the source whose id is the part of its kind before the
  first dot, so `fs.file-saved` belongs to `fs`.
- `apply(rules)`: makes exactly these rules live and returns a `RuleState`
  for every rule id passed in.
  - `Rejected`: no source owns the kind, or the source's `check` failed.
  - `SourceFailed`: the source is not available here, or its `start` failed.
    Other sources are not affected.
  - `Active`: the rule is running.
  - A source whose rules did not change is left running. A source whose
    rules changed is stopped and started again with all its rules. A source
    left with no rules is stopped.
- `publish(event)`: sends an event into the same channel the sources use, for
  triggers that come from inside the app.
- `subscribe()`: a receiver of `Fired { event, coalesced }`.
- `with_cool_down(duration)`: the default is two seconds.

## Cool down

Per rule, measured on `event.at`, not on a clock. An event inside the cool
down that follows the last event let through is dropped and counted. The next
`Fired` for that rule carries the count in `coalesced`, and the count starts
again from zero.

## Develop

Run from `src-tauri`:

- `cargo test -p trigger-hub`
- `cargo clippy -p trigger-hub --all-targets -- -D warnings`
- `cargo fmt -p trigger-hub --check`

From the repo root: `node plans/_tools/check-crates.mjs`.

Tests use `trigger_core::testkit::FakeSource`, wrapped by the helper in
`src/test_support.rs` so a test can read what the source recorded after the hub
took ownership of it.
