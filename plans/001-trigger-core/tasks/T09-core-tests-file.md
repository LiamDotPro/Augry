---
id: T09
epic: 001-trigger-core
title: Move trigger-core's unit tests into src/tests.rs
status: todo
depends_on: [T07]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-core/src/lib.rs
  - src-tauri/crates/trigger-core/src/tests.rs
---

## Goal

`trigger-core/src/lib.rs` drops under the 400 line limit that
`check-crates.mjs` warns about, by moving its four test modules, unchanged
apart from their `use` paths, into a new `src/tests.rs`. No behaviour changes.

## Read first

- `src-tauri/crates/trigger-core/src/lib.rs` the four `#[cfg(test)] mod ..._tests` modules: `platforms_tests`, `json_tests`, `handle_tests`, `markdown_tests`
- `src-tauri/crates/trigger-hub/` does not matter here; only `trigger-core` changes

## Steps

1. Create `src-tauri/crates/trigger-core/src/tests.rs`. Its first line is:

```rust
//! Unit tests for the contract. Kept out of lib.rs so that file stays small.
```

   followed by a blank line.

2. Move each of the four test modules from `lib.rs` into `tests.rs`, in the
   order they appear in `lib.rs` (`platforms_tests`, `json_tests`,
   `handle_tests`, `markdown_tests`), with one blank line between modules.
   For each module:
   - Cut it from `lib.rs` including its `#[cfg(test)]` line and its whole
     `mod name { ... }` block.
   - In `tests.rs`, drop the `#[cfg(test)]` line (the whole file is test only,
     see step 3) and keep `mod name { ... }` exactly as it was, except:
     - every `use super::*;` becomes `use crate::*;`
     - `#[serde(with = "super::ms_since_epoch")]` becomes
       `#[serde(with = "crate::ms_since_epoch")]`
   Do not change any test body, name or expected value.

3. In `lib.rs`, directly below the `pub mod testkit;` line, add:

```rust
#[cfg(test)]
mod tests;
```

4. Make sure `lib.rs` no longer contains `#[cfg(test)]` anywhere except the
   line added in step 3: `grep -c "cfg(test)" src-tauri/crates/trigger-core/src/lib.rs`
   prints `1`.

5. Run `cd src-tauri && cargo fmt -p trigger-core`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-core` passes and the unit test line reads `test result: ok. 21 passed` (13 moved tests plus the 8 in `testkit.rs`, the same as before this card)
- [ ] `cd src-tauri && cargo test -p trigger-core tests::markdown_tests` passes (2 tests)
- [ ] `cd src-tauri && cargo clippy -p trigger-core --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-core --check` exits 0
- [ ] `wc -l < src-tauri/crates/trigger-core/src/lib.rs` prints a number under 400
- [ ] `node plans/_tools/check-crates.mjs` exits 0 and prints no line starting with `warn  trigger-core`
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Moving `testkit.rs`'s own test modules. They stay where they are.
- Changing any non test code in `lib.rs`, or the order of the items in it.
- Renaming test modules or tests, or adding tests.

## If stuck

- If a test fails to compile with "cannot find" a private function such as `platforms_label`, check the module has `use crate::*;`; glob imports from the crate root include private items visible to child modules. If it still fails, set status blocked and quote the error.
- If `lib.rs` is still 400 lines or more after the move, set status blocked and give the line count.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
