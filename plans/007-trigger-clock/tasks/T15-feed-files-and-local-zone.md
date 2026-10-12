---
id: T15
epic: 007-trigger-clock
title: Add feed file reads and the local zone to os.rs
status: todo
depends_on: [T14]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-clock/src/os.rs
  - src-tauri/crates/trigger-clock/src/os/tests.rs
  - src-tauri/crates/trigger-clock/Cargo.toml
---

## Goal

`os.rs` has `read_feed_file` (a feed given as a path, with the same 10 MiB cap), `zone_from_name` (an IANA name to a calcard zone, floating when unknown) and `local_zone` (the machine's zone through `iana-time-zone`). With these and `feed::expand`, the feed thread in slice C has everything it needs.

## Read first

- `plans/007-trigger-clock/epic.md` Decision 6 (the last sentence: the IANA name feeds calcard's `default_tz`, falling back to floating) and Decision 9 (a path is read with `std::fs`)
- `plans/007-trigger-clock/research/R02-ics-and-http.md` "Implications for the cards", the paragraph on the local zone
- `src-tauri/crates/trigger-clock/src/os.rs` the file you add to
- `src-tauri/crates/trigger-clock/src/os/tests.rs` the test file you append to

## Steps

1. In `src-tauri/crates/trigger-clock/src/os.rs`, find the line

```rust
use std::time::Duration;
```

and replace it with these lines (the existing blank line and `use trigger_core::Availability;` below it stay):

```rust
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use calcard::common::timezone::Tz as CalTz;
```

2. In the same file, find the last two lines

```rust
#[cfg(test)]
mod tests;
```

and replace them with this text:

```rust
/// Reads a feed from a file, refusing one over `MAX_FEED_BYTES`. The error
/// never includes the path.
pub fn read_feed_file(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path)
        .map_err(|e| format!("could not open the feed file: {}", e.kind()))?;
    let mut text = String::new();
    file.take(MAX_FEED_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| format!("could not read the feed file: {}", e.kind()))?;
    if text.len() as u64 > MAX_FEED_BYTES {
        return Err("the feed is too large".to_string());
    }
    Ok(text)
}

/// The zone for an IANA name such as `Europe/London`, or floating when the
/// name is not one `chrono-tz` knows. Floating makes times written without a
/// zone mean the same wall clock time wherever they are read.
pub fn zone_from_name(name: &str) -> CalTz {
    name.parse::<chrono_tz::Tz>()
        .map_or(CalTz::Floating, CalTz::from)
}

/// The machine's time zone, used for times in a feed written without one.
/// Floating when the OS does not say.
pub fn local_zone() -> CalTz {
    iana_time_zone::get_timezone().map_or(CalTz::Floating, |name| zone_from_name(&name))
}

#[cfg(test)]
mod tests;
```

3. Append this to the end of `src-tauri/crates/trigger-clock/src/os/tests.rs`, after a blank line. Change nothing above it. The tests use `uuid`, so step 4 adds it.

```rust
fn temp_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("trigger-clock-{}.ics", uuid::Uuid::new_v4()))
}

#[test]
fn a_feed_file_is_read_as_text() {
    let path = temp_path();
    std::fs::write(&path, "BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n").unwrap();
    let text = read_feed_file(&path);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(text.unwrap(), "BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n");
}

#[test]
fn a_missing_or_binary_feed_file_is_an_error_without_the_path() {
    let missing = temp_path();
    let err = read_feed_file(&missing).unwrap_err();
    assert!(err.starts_with("could not open the feed file"));
    assert!(!err.contains("trigger-clock-"));
    let binary = temp_path();
    std::fs::write(&binary, [0xff, 0xfe, 0x00, 0x80]).unwrap();
    let result = read_feed_file(&binary);
    std::fs::remove_file(&binary).unwrap();
    assert!(result.is_err());
}

#[test]
fn zone_names_parse_and_unknown_names_are_floating() {
    assert_eq!(
        zone_from_name("Europe/London"),
        CalTz::from(chrono_tz::Europe::London)
    );
    assert_eq!(zone_from_name("UTC"), CalTz::UTC);
    assert_eq!(zone_from_name("Not/AZone"), CalTz::Floating);
    assert_eq!(zone_from_name(""), CalTz::Floating);
}

#[test]
fn the_local_zone_can_be_asked_for_on_any_machine() {
    // The answer depends on the machine, so only check that it does not panic.
    let _ = local_zone();
}
```

4. Edit `src-tauri/crates/trigger-clock/Cargo.toml`. Add this at the very end of the file, after a blank line (T10 removed the old `[dev-dependencies]` section, so this is a new one):

```toml

[dev-dependencies]
uuid = { workspace = true }
```

`uuid` is already in `[workspace.dependencies]` of `src-tauri/Cargo.toml`; do not touch that file.

5. Run `cd src-tauri && cargo fmt -p trigger-clock`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock os::` passes: 9 tests
- [ ] `cd src-tauri && cargo test -p trigger-clock` passes (all earlier tests too; `live` shows 1 ignored)
- [ ] `grep -c "^\[dev-dependencies\]" src-tauri/crates/trigger-clock/Cargo.toml` prints `1`
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (the `still has template code` warning for `src/lib.rs` is expected until slice C)
- [ ] `git status --porcelain` lists only paths under `files:` (ignore other card files under `plans/007-trigger-clock/tasks/`)

## Out of scope

- Calling these functions from anywhere: slice C.
- Reading the `TZ` environment variable, or any zone fallback other than floating.
- Watching a feed file for changes (the feed thread re-reads it on its 5 minute poll).
- Platform specific code: `iana-time-zone` is cross platform, so `os.rs` needs no `cfg` blocks.

## If stuck

- If pasted code does not compile or clippy objects, check that you pasted it exactly and in the place the step names. If it still fails, set `status: blocked` and quote the compiler error. Do not change the code.
- If the compiler says `iana_time_zone::get_timezone` is missing, run `cd src-tauri && cargo tree -p trigger-clock -i iana-time-zone`, write the version in the Log, set `status: blocked` and stop.
- If `zone_names_parse_and_unknown_names_are_floating` fails on `"UTC"`, do not change the test; set `status: blocked` and quote the failure.
- If `cargo fmt -p trigger-clock --check` fails, run `cd src-tauri && cargo fmt -p trigger-clock` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
