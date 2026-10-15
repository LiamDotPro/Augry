---
id: T28
epic: 001-trigger-core
title: Add the five live tests for trigger-fs
status: done
depends_on: [T27]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-fs/tests/live.rs
---

## Goal

`tests/live.rs` has the five `#[ignore]` tests that watch a real folder and check the events, and they pass on this machine, three runs in a row.

## Read first

- `plans/001-trigger-core/research/R04-ci-live-tests.md` why 400 ms, 10 s and `--test-threads=1`
- `plans/001-trigger-core/epic.md` Decision 17, the paragraph about live tests
- `src-tauri/crates/trigger-core/src/testkit.rs` `RecordingSink::wait_for` (returns the first event of that kind and keeps the others for `drain`)
- `src-tauri/crates/trigger-fs/tests/live.rs` the template test you replace

## Steps

1. Replace the whole of `src-tauri/crates/trigger-fs/tests/live.rs` with this. These are the five live tests of Decision 17: created, saved, removed, glob excluded, atomic save. Each has its own canonical folder under `std::env::temp_dir()` named with a uuid, waits 400 ms after `start` before touching files, and waits for events with `RecordingSink::wait_for` and a 10 s ceiling. The "nothing else happened" checks first wait for an event on a sentinel file (the glob test) or wait out the settle time (the atomic save test), then drain the sink. The tests use `settle_ms: 100`, except the atomic save which keeps 500 so a late burst of events from the OS cannot split one save in two.

```rust
//! Tests against the real file system watcher. Not run by `cargo test`; run
//! them on a machine of each supported platform with:
//!
//!     cargo test -p trigger-fs -- --ignored --test-threads=1
//!
//! Each test has its own folder under the temp dir, arms one rule, changes
//! files, and waits up to 10 s for the event with `RecordingSink::wait_for`.
//! A "nothing else happened" check first waits for an event on a sentinel
//! file, so it never relies on a bare sleep.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::json;
use trigger_core::testkit::{rule, RecordingSink};
use trigger_core::{Event, Handle, Source};
use trigger_fs::FsSource;

const CEILING: Duration = Duration::from_secs(10);

/// A canonical folder under the temp dir, removed when dropped. Declare it
/// before the `Handle` so the watcher stops first.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        let dir = std::env::temp_dir().join(format!("tg-fs-live-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(std::fs::canonicalize(&dir).unwrap())
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Arms one rule on the folder and waits 400 ms, because some watchers start
/// asynchronously and lose events from the first moments.
fn arm(dir: &TempDir, kind: &str, extra: serde_json::Value) -> (RecordingSink, Handle) {
    let mut when = json!({ "path": dir.0, "settle_ms": 100 });
    when.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let sink = RecordingSink::new();
    let handle = FsSource
        .start(vec![rule("p/t", kind, when)], sink.tx.clone())
        .expect("start");
    std::thread::sleep(Duration::from_millis(400));
    (sink, handle)
}

fn expect(sink: &RecordingSink, kind: &str) -> Event {
    sink.wait_for(kind, CEILING)
        .unwrap_or_else(|| panic!("no {kind} within 10 s"))
}

fn path_of(event: &Event) -> &Path {
    Path::new(event.payload["path"].as_str().unwrap())
}

#[test]
#[ignore]
fn a_new_file_fires_created() {
    let dir = TempDir::new();
    let (sink, _handle) = arm(&dir, "fs.file-created", json!({}));
    std::fs::write(dir.path("new.txt"), "hello").unwrap();
    let event = expect(&sink, "fs.file-created");
    assert_eq!(path_of(&event), dir.path("new.txt"));
    assert_eq!(event.payload["name"], json!("new.txt"));
    assert_eq!(event.payload["relative"], json!("new.txt"));
    assert_eq!(event.payload["ext"], json!("txt"));
}

#[test]
#[ignore]
fn saving_an_existing_file_fires_saved() {
    let dir = TempDir::new();
    std::fs::write(dir.path("notes.md"), "one").unwrap();
    let (sink, _handle) = arm(&dir, "fs.file-saved", json!({}));
    std::fs::write(dir.path("notes.md"), "two").unwrap();
    let event = expect(&sink, "fs.file-saved");
    assert_eq!(path_of(&event), dir.path("notes.md"));
    assert_eq!(event.payload["ext"], json!("md"));
}

#[test]
#[ignore]
fn deleting_a_file_fires_removed() {
    let dir = TempDir::new();
    std::fs::write(dir.path("gone.txt"), "bye").unwrap();
    let (sink, _handle) = arm(&dir, "fs.file-removed", json!({}));
    std::fs::remove_file(dir.path("gone.txt")).unwrap();
    let event = expect(&sink, "fs.file-removed");
    assert_eq!(path_of(&event), dir.path("gone.txt"));
}

#[test]
#[ignore]
fn a_file_the_glob_excludes_fires_nothing() {
    let dir = TempDir::new();
    let (sink, _handle) = arm(&dir, "fs.file-created", json!({ "glob": "**/*.md" }));
    std::fs::write(dir.path("skip.txt"), "x").unwrap();
    std::fs::write(dir.path("sentinel.md"), "y").unwrap();
    let event = expect(&sink, "fs.file-created");
    assert_eq!(event.payload["name"], json!("sentinel.md"));
    std::thread::sleep(Duration::from_millis(500));
    assert!(sink.drain().is_empty(), "skip.txt must not fire");
}

#[test]
#[ignore]
fn an_atomic_save_fires_one_saved_for_the_target() {
    let dir = TempDir::new();
    std::fs::write(dir.path("a.txt"), "one").unwrap();
    let (sink, _handle) = arm(&dir, "fs.file-saved", json!({ "settle_ms": 500 }));
    // The way editors save: write a temp file, then rename it over the target.
    std::fs::write(dir.path("a.txt.new"), "two").unwrap();
    std::fs::rename(dir.path("a.txt.new"), dir.path("a.txt")).unwrap();
    let event = expect(&sink, "fs.file-saved");
    assert_eq!(path_of(&event), dir.path("a.txt"));
    std::thread::sleep(Duration::from_millis(1500));
    assert!(sink.drain().is_empty(), "one save, one event");
}
```

2. Run `cd src-tauri && cargo fmt -p trigger-fs`.

3. Run the live tests three times in a row: `cd src-tauri && for i in 1 2 3; do cargo test -p trigger-fs -- --ignored --test-threads=1 || break; done`. Each round runs the five tests here plus the ignored `listen` test in `os.rs`, and must pass. A test that passes twice and fails once is a failure: set status blocked (see If stuck).

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-fs -- --ignored --test-threads=1` passes: 5 tests in `live` and 1 in the library
- [ ] `cd src-tauri && cargo test -p trigger-fs` passes (the live tests show as 5 ignored)
- [ ] `find "$(node -p "require('os').tmpdir()")" -maxdepth 1 -name 'tg-fs-*'` prints nothing (every test removed its folder)
- [ ] `cd src-tauri && cargo clippy -p trigger-fs --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-fs --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- More live tests (rename, download, rapid burst). Five is the number in Decision 17.
- Longer sleeps, retries, or a `#[cfg]` that skips a test on one platform.
- Changing the 400 ms start wait, the 10 s ceiling or `--test-threads=1`.
- Touching CI (slice E adds the live test step).
- Any change in `src/`.

## If stuck

- If a live test times out on macOS the first time, check whether macOS showed a permission prompt for the temp folder; allow it and run again. Temp folders do not normally need one.
- If a test fails once in three rounds, do not add sleeps. Set status blocked and paste the failing test name and message in the Log.
- If `tests/live.rs` does not compile because `uuid` is missing, T20 step 3 was skipped. Set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (live tests 3 rounds green, 5/5 each round; check-crates exit 0 with accepted warnings only)
- 2026-10-12 checker: pass
