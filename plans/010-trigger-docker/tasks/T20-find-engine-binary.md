---
id: T20
epic: 010-trigger-docker
title: Find the engine binary and report availability
status: done
depends_on: [T14]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-docker/src/os.rs
  - src-tauri/crates/trigger-docker/src/os/tests.rs
---

## Goal

`src/os.rs` can find the engine's command line tool and say whether there is one. `find_engine(Engine)` looks in the process `PATH` and then in a fixed list of usual folders per OS, takes the first real file, and remembers the answer for the rest of the run. `availability()` reports `Available` when a docker or podman tool was found. Nothing runs a program yet. The folder lists are data in one `cfg` pair of functions, so this is one card for all three platforms.

## Read first

- `plans/010-trigger-docker/epic.md` Decision 7 (finding the binary) and Decision 4
- `plans/010-trigger-docker/research/R02-engine-binary.md` "Implications", items 1 to 3 (the folder lists). The login shell probe in item 1 is not used: Decision 7 says no.
- `src-tauri/crates/trigger-docker/src/os.rs` the stub this card replaces
- `src-tauri/crates/trigger-docker/src/logic.rs:26-40` the `Engine` enum (`Docker`, `Podman`, `as_str`)

## Steps

1. Run `mkdir -p src-tauri/crates/trigger-docker/src/os`.

2. Replace the whole of `src-tauri/crates/trigger-docker/src/os.rs` with this. `candidate_dirs` has one version per platform family. `search_dirs` and `find_in` are pure so the tests need no engine. `find_engine` keeps its answer in a `OnceLock` per engine, even a "not found", because the catalogue asks on the UI thread and must not scan the disk each time; the cost is that a tool installed while the app runs is seen after a restart.

```rust
//! The only file that talks to the operating system: finding the engine
//! binary, asking it whether it is up, and running its event stream. The
//! decisions about events are all in `logic.rs`.

// Removed when lib.rs uses everything in this file (slice C).
#![allow(dead_code)]

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::OnceLock;

use trigger_core::Availability;

use crate::logic::Engine;

/// The file name of the engine's command line tool.
fn binary_name(engine: Engine) -> String {
    if cfg!(windows) {
        format!("{}.exe", engine.as_str())
    } else {
        engine.as_str().to_string()
    }
}

/// Folders the engine's tool usually lives in, searched after `PATH`. A
/// program started from the Dock or the Start menu may have a short `PATH`
/// that misses them. A wrong entry costs one failed file check.
#[cfg(not(windows))]
fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = [
        "/usr/local/bin",
        "/opt/homebrew/bin",
        "/opt/podman/bin",
        "/usr/bin",
        "/Applications/Docker.app/Contents/Resources/bin",
        "/snap/bin",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for sub in [".orbstack/bin", ".rd/bin", ".docker/bin"] {
            dirs.push(home.join(sub));
        }
    }
    dirs
}

/// Same as above for Windows. The folders come from environment variables,
/// never from a hard coded drive letter.
#[cfg(windows)]
fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for (var, sub) in [
        ("ProgramFiles", r"Docker\Docker\resources\bin"),
        ("ProgramFiles", r"RedHat\Podman"),
        ("ProgramFiles", "Podman"),
        ("LOCALAPPDATA", r"Programs\Podman"),
        ("USERPROFILE", r".rd\bin"),
    ] {
        if let Some(base) = std::env::var_os(var) {
            dirs.push(PathBuf::from(base).join(sub));
        }
    }
    dirs
}

/// `PATH` first, then the extra folders, without empty entries or repeats.
fn search_dirs(path: Option<OsString>, extra: Vec<PathBuf>) -> Vec<PathBuf> {
    let from_path = path
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut dirs: Vec<PathBuf> = Vec::new();
    for dir in from_path.into_iter().chain(extra) {
        if !dir.as_os_str().is_empty() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// The first folder that holds a file with this name wins. A folder with that
/// name does not count.
fn find_in(dirs: &[PathBuf], file_name: &str) -> Option<PathBuf> {
    dirs.iter()
        .map(|dir| dir.join(file_name))
        .find(|candidate| candidate.is_file())
}

/// Where the engine's tool is, or `None`. Looked up once per run of the app
/// and kept, because the catalogue asks on the UI thread. No login shell is
/// started to widen `PATH`.
pub fn find_engine(engine: Engine) -> Option<PathBuf> {
    static DOCKER: OnceLock<Option<PathBuf>> = OnceLock::new();
    static PODMAN: OnceLock<Option<PathBuf>> = OnceLock::new();
    let slot = match engine {
        Engine::Docker => &DOCKER,
        Engine::Podman => &PODMAN,
    };
    slot.get_or_init(|| {
        let dirs = search_dirs(std::env::var_os("PATH"), candidate_dirs());
        find_in(&dirs, &binary_name(engine))
    })
    .clone()
}

/// Whether there is an engine to ask. It only checks that a tool was found;
/// whether the engine answers is checked by `start`.
pub fn availability() -> Availability {
    if find_engine(Engine::Docker).is_some() || find_engine(Engine::Podman).is_some() {
        Availability::Available
    } else {
        Availability::Unsupported {
            reason: "neither docker nor podman was found".into(),
        }
    }
}

#[cfg(test)]
mod tests;
```

3. Create `src-tauri/crates/trigger-docker/src/os/tests.rs` with exactly this content.

```rust
use super::*;

/// A fresh folder under the system temp folder. The tests remove it.
fn temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "trigger-docker-{label}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_binary_name_has_the_platform_extension() {
    let expected = if cfg!(windows) {
        "docker.exe"
    } else {
        "docker"
    };
    assert_eq!(binary_name(Engine::Docker), expected);
    let expected = if cfg!(windows) {
        "podman.exe"
    } else {
        "podman"
    };
    assert_eq!(binary_name(Engine::Podman), expected);
}

#[test]
fn path_comes_first_then_the_extra_folders() {
    let path = std::env::join_paths(["/a", "/b"]).unwrap();
    let dirs = search_dirs(Some(path), vec![PathBuf::from("/c"), PathBuf::from("/d")]);
    let expected: Vec<PathBuf> = ["/a", "/b", "/c", "/d"].iter().map(PathBuf::from).collect();
    assert_eq!(dirs, expected);
}

#[test]
fn repeats_and_empty_entries_are_dropped() {
    let path = std::env::join_paths(["/a", "", "/a"]).unwrap();
    let dirs = search_dirs(Some(path), vec![PathBuf::from("/a"), PathBuf::from("/c")]);
    let expected: Vec<PathBuf> = ["/a", "/c"].iter().map(PathBuf::from).collect();
    assert_eq!(dirs, expected);
}

#[test]
fn a_missing_path_still_gives_the_extra_folders() {
    let dirs = search_dirs(None, vec![PathBuf::from("/c")]);
    assert_eq!(dirs, vec![PathBuf::from("/c")]);
}

#[test]
fn find_in_returns_the_first_folder_with_the_file() {
    let first = temp_dir("first");
    let second = temp_dir("second");
    std::fs::write(second.join("docker"), b"").unwrap();
    let dirs = vec![first.clone(), second.clone()];
    assert_eq!(find_in(&dirs, "docker"), Some(second.join("docker")));
    std::fs::write(first.join("docker"), b"").unwrap();
    assert_eq!(find_in(&dirs, "docker"), Some(first.join("docker")));
    assert_eq!(find_in(&dirs, "podman"), None);
    std::fs::remove_dir_all(first).unwrap();
    std::fs::remove_dir_all(second).unwrap();
}

#[test]
fn a_folder_named_like_the_tool_does_not_count() {
    let dir = temp_dir("folder");
    std::fs::create_dir(dir.join("docker")).unwrap();
    assert_eq!(find_in(std::slice::from_ref(&dir), "docker"), None);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_candidate_folders_are_not_empty_and_have_no_blanks() {
    let dirs = candidate_dirs();
    assert!(!dirs.is_empty());
    assert!(dirs.iter().all(|d| !d.as_os_str().is_empty()));
}

#[cfg(not(windows))]
#[test]
fn the_unix_candidates_include_the_usual_places() {
    let dirs = candidate_dirs();
    assert!(dirs.contains(&PathBuf::from("/usr/local/bin")));
    assert!(dirs.contains(&PathBuf::from("/opt/homebrew/bin")));
}

#[test]
fn find_engine_gives_the_same_answer_twice() {
    assert_eq!(find_engine(Engine::Docker), find_engine(Engine::Docker));
}

#[test]
fn availability_is_one_of_the_two_answers_it_can_give() {
    match availability() {
        Availability::Available => {}
        Availability::Unsupported { reason } => assert!(reason.contains("docker")),
        other => panic!("unexpected {other:?}"),
    }
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-docker`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker os::` prints `10 passed` for the library (on Windows 9: `the_unix_candidates_include_the_usual_places` is unix only)
- [ ] `cd src-tauri && cargo test -p trigger-docker` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (it may print warnings for template text in `README.md` and `src/lib.rs`; T24 and T27 remove them)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/010-trigger-docker/tasks/`

## Out of scope

- Running any program, the `version` check and the real engine (T21, T22).
- A login shell probe for `PATH`, `PATHEXT` handling, or reading `DOCKER_HOST` or docker contexts.
- Adding folders to the lists, or dropping any. They come from R02.
- Adding dependencies (`dirs`, `which`). The folders come from `std::env`.
- The README. Internal code only; the card that writes the README is T27, which describes this work.
- Removing the `#![allow(dead_code)]` line in `os.rs`. T24 removes it.
- Editing `lib.rs`. It still calls `os::availability()` and needs no change.

## If stuck

- If `os.rs` is not the stub (it already contains `find_engine`), set status blocked and say so.
- If clippy says `unused import` for a name in `os.rs`, delete just that name from the `use` line. Do not change anything else.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
