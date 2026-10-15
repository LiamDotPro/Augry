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
