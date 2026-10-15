---
id: T26
epic: 001-trigger-core
title: Add the OS helpers: clock, stat, scan and event kinds
status: done
depends_on: [T25]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-fs/src/os.rs
---

## Goal

`os.rs` has the helper functions the glue needs, tested on a real temp folder: the millisecond clock, canonical paths, the regular-file check, the start scan, and the mapping from `notify` event kinds to `RawKind`.

## Read first

- `plans/001-trigger-core/epic.md` Decisions 6 (canonical path) and 22 (start walk, rescan)
- `plans/001-trigger-core/research/R03-notify-events.md` what each OS reports
- `src-tauri/crates/trigger-fs/src/logic.rs` `RawKind` and `Signal`, which `os.rs` produces
- `src-tauri/crates/trigger-fs/src/os.rs` the file you replace

## Steps

1. Replace the whole of `src-tauri/crates/trigger-fs/src/os.rs` with this. `os.rs` is the only file that talks to the OS. This card adds the pieces that need no watcher: `now_ms` (the one clock the logic gets its time from), `canonical` (Decision 6: macOS reports `/private/var/...` for `/var/...` and Windows adds `\\?\`), `is_file` and `scan` (the start walk of Decision 22, which skips ignored folders), and `raw_kind`, which turns a `notify` event into the `RawKind` the logic knows, so `logic.rs` never sees a `notify` type. `is_file` uses `symlink_metadata`, so symlinks and directories are never reported, and `scan` agrees with it. `listen` is still the template stub; T27 writes it. Keep the `#![allow(dead_code)]` line.

```rust
//! The only file that talks to the operating system: the clock, the file
//! system and the `notify` watcher. `notify` hides the differences between
//! FSEvents, ReadDirectoryChangesW and inotify, so there is one file for all
//! three platforms. Everything else is in `logic.rs`.

// Removed in T27, when lib.rs uses everything in this file.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use globset::GlobSet;
use notify::event::{EventKind, ModifyKind, RenameMode};
use trigger_core::{Availability, SourceError};

use crate::logic::{Armed, RawKind, Signal};

pub fn availability() -> Availability {
    Availability::Available
}

/// Milliseconds since the Unix epoch. The logic gets all its time from here.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The canonical form of a watched folder. macOS reports `/private/var/...`
/// for `/var/...` and Windows adds `\\?\`, so every path in this crate is
/// compared in this form.
pub fn canonical(path: &Path) -> Result<PathBuf, SourceError> {
    std::fs::canonicalize(path)
        .map_err(|e| SourceError::Os(format!("cannot watch {}: {e}", path.display())))
}

/// True for a regular file. Directories and symlinks are never reported, so
/// the start scan and the settle check must both answer the same way.
pub fn is_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.is_file())
        .unwrap_or(false)
}

/// Every regular file below `root`, without descending into (or listing)
/// anything the ignore set matches. Unreadable folders are skipped.
pub fn scan(root: &Path, ignore: &GlobSet) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let ignored = path
                .strip_prefix(root)
                .map(|rel| ignore.is_match(rel))
                .unwrap_or(true);
            if ignored {
                continue;
            }
            match entry.file_type() {
                Ok(t) if t.is_dir() => dirs.push(path),
                Ok(t) if t.is_file() => files.push(path),
                _ => {}
            }
        }
    }
    files
}

/// Reduces a `notify` event to what the logic cares about.
pub fn raw_kind(event: &notify::Event) -> RawKind {
    if event.need_rescan() {
        return RawKind::Rescan;
    }
    match event.kind {
        EventKind::Create(_) => RawKind::Create,
        EventKind::Remove(_) => RawKind::Remove,
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => RawKind::Both,
        EventKind::Modify(ModifyKind::Name(_)) => RawKind::Name,
        EventKind::Modify(_) | EventKind::Any => RawKind::Modify,
        EventKind::Access(_) | EventKind::Other => RawKind::Ignore,
    }
}

/// Starts reporting signals for the armed rules. Returns at once; the work
/// happens on a named thread or an OS callback, and ends when `stop` is set.
pub fn listen(
    _armed: &[Armed],
    _stop: Arc<AtomicBool>,
    _tx: Sender<Signal>,
) -> Result<(), SourceError> {
    Err(SourceError::Unavailable("not implemented yet".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, CreateKind, DataChange, Flag, MetadataKind, RemoveKind};
    use notify::Event;

    fn kind_of(kind: EventKind) -> RawKind {
        raw_kind(&Event::new(kind))
    }

    #[test]
    fn notify_kinds_map_to_raw_kinds() {
        assert_eq!(
            kind_of(EventKind::Create(CreateKind::File)),
            RawKind::Create
        );
        assert_eq!(kind_of(EventKind::Remove(RemoveKind::Any)), RawKind::Remove);
        let data = ModifyKind::Data(DataChange::Content);
        assert_eq!(kind_of(EventKind::Modify(data)), RawKind::Modify);
        let meta = ModifyKind::Metadata(MetadataKind::Extended);
        assert_eq!(kind_of(EventKind::Modify(meta)), RawKind::Modify);
        assert_eq!(kind_of(EventKind::Any), RawKind::Modify);
        for mode in [RenameMode::Any, RenameMode::From, RenameMode::To] {
            let name = ModifyKind::Name(mode);
            assert_eq!(kind_of(EventKind::Modify(name)), RawKind::Name);
        }
        let both = ModifyKind::Name(RenameMode::Both);
        assert_eq!(kind_of(EventKind::Modify(both)), RawKind::Both);
        assert_eq!(kind_of(EventKind::Access(AccessKind::Any)), RawKind::Ignore);
        assert_eq!(kind_of(EventKind::Other), RawKind::Ignore);
    }

    #[test]
    fn the_rescan_flag_wins_over_the_kind() {
        let event = Event::new(EventKind::Other).set_flag(Flag::Rescan);
        assert_eq!(raw_kind(&event), RawKind::Rescan);
    }

    /// A folder under the temp dir that is removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> TempDir {
            let dir = std::env::temp_dir().join(format!("tg-fs-os-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(canonical(&dir).unwrap())
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn ignore_set(patterns: &[&str]) -> GlobSet {
        let mut set = globset::GlobSetBuilder::new();
        for p in patterns {
            set.add(globset::Glob::new(p).unwrap());
        }
        set.build().unwrap()
    }

    #[test]
    fn scan_lists_files_and_skips_ignored_folders() {
        let dir = TempDir::new();
        let root = &dir.0;
        std::fs::create_dir_all(root.join("src/deep")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        std::fs::write(root.join("a.txt"), "a").unwrap();
        std::fs::write(root.join("src/deep/b.rs"), "b").unwrap();
        std::fs::write(root.join("node_modules/pkg/c.js"), "c").unwrap();
        std::fs::write(root.join("skip.tmp"), "d").unwrap();
        let ignore = ignore_set(&["**/node_modules", "**/node_modules/**", "**/*.tmp"]);
        let mut found = scan(root, &ignore);
        found.sort();
        assert_eq!(found, vec![root.join("a.txt"), root.join("src/deep/b.rs")]);
    }

    #[test]
    fn is_file_is_true_for_files_only() {
        let dir = TempDir::new();
        let file = dir.0.join("a.txt");
        std::fs::write(&file, "a").unwrap();
        assert!(is_file(&file));
        assert!(!is_file(&dir.0));
        assert!(!is_file(&dir.0.join("missing.txt")));
    }

    #[test]
    fn canonical_resolves_a_real_folder_and_rejects_a_missing_one() {
        let dir = TempDir::new();
        assert_eq!(canonical(&dir.0).unwrap(), dir.0);
        assert!(matches!(
            canonical(&dir.0.join("missing")),
            Err(SourceError::Os(_))
        ));
    }

    #[test]
    fn now_ms_is_a_plausible_unix_time() {
        assert!(now_ms() > 1_700_000_000_000);
    }
}
```

2. Run `cd src-tauri && cargo fmt -p trigger-fs`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-fs --lib os::` passes: 6 tests
- [ ] `cd src-tauri && cargo test -p trigger-fs` passes (43 in the library)
- [ ] `find "$(node -p "require('os').tmpdir()")" -maxdepth 1 -name 'tg-fs-os-*'` prints nothing (the tests removed their folders)
- [ ] `cd src-tauri && cargo clippy -p trigger-fs --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-fs --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- `listen` and the `notify` watcher (T27).
- Calling any of these functions from `lib.rs` (T27).
- Following symlinks in `scan` or `is_file`.
- Reading the paths of `notify::Event` as anything other than the OS gave them.
- Removing the `#![allow(dead_code)]` line.
- Editing the README (T29).

## If stuck

- If `notify::event::Flag`, `Event::set_flag` or `Event::need_rescan` does not exist, the `notify` version is not 8.2. Check `cargo tree -p trigger-fs -i notify` and set status blocked if it is not 8.2.x.
- If the `scan` test fails because of the order of the files, the test already sorts them; re-copy step 1.
- If a test cannot create a folder under the temp dir, set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
