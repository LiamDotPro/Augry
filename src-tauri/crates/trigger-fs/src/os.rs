//! The only file that talks to the operating system: the clock, the file
//! system and the `notify` watcher. `notify` hides the differences between
//! FSEvents, ReadDirectoryChangesW and inotify, so there is one file for all
//! three platforms. Everything else is in `logic.rs`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use globset::GlobSet;
use notify::event::{EventKind, ModifyKind, RenameMode};
use notify::{RecursiveMode, Watcher};
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

/// Starts reporting signals for the armed rules: one `notify` watcher over
/// every distinct root, recursive. Returns at once. The watcher lives on a
/// named thread until `stop` is set, then it is dropped, which stops it.
pub fn listen(
    armed: &[Armed],
    stop: Arc<AtomicBool>,
    tx: Sender<Signal>,
) -> Result<(), SourceError> {
    let mut roots: Vec<PathBuf> = armed.iter().map(|a| a.root.clone()).collect();
    roots.sort();
    roots.dedup();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        // A watcher error carries no path to act on; the next event or the
        // next rescan puts things right.
        let Ok(event) = res else { return };
        let kind = raw_kind(&event);
        if kind == RawKind::Ignore {
            return;
        }
        let _ = tx.send(Signal::Raw {
            at: now_ms(),
            kind,
            paths: event.paths,
        });
    })
    .map_err(|e| SourceError::Os(e.to_string()))?;
    for root in &roots {
        watcher
            .watch(root, RecursiveMode::Recursive)
            .map_err(|e| SourceError::Os(format!("cannot watch {}: {e}", root.display())))?;
    }

    std::thread::Builder::new()
        .name("trigger-fs-watch".into())
        .spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(50));
            }
            drop(watcher);
        })
        .map_err(|e| SourceError::Os(e.to_string()))?;
    Ok(())
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

    /// Needs a real file system watcher. Run with:
    /// `cargo test -p trigger-fs -- --ignored --test-threads=1`
    #[test]
    #[ignore]
    fn listen_reports_a_created_file_and_stops() {
        use std::sync::mpsc::channel;

        let dir = TempDir::new();
        let parsed = crate::rule::parse(&trigger_core::testkit::rule(
            "p/t",
            "fs.file-created",
            serde_json::json!({ "path": dir.0 }),
        ))
        .unwrap();
        let armed = Armed {
            id: "p/t".into(),
            kind: "fs.file-created".into(),
            root: dir.0.clone(),
            when: parsed.when,
            glob: parsed.glob,
            ignore: parsed.ignore,
        };
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        listen(&[armed], stop.clone(), tx).unwrap();
        std::thread::sleep(Duration::from_millis(400));
        let file = dir.0.join("hello.txt");
        std::fs::write(&file, "hi").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut seen = false;
        while std::time::Instant::now() < deadline && !seen {
            if let Ok(Signal::Raw { paths, .. }) = rx.recv_timeout(Duration::from_millis(100)) {
                seen = paths.contains(&file);
            }
        }
        stop.store(true, Ordering::Relaxed);
        assert!(seen, "no raw event for the new file within 10 s");
    }
}
