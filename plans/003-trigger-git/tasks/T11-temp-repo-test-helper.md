---
id: T11
epic: 003-trigger-git
title: Add a temporary repository helper for git tests
status: todo
depends_on: [T10]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/git.rs
  - src-tauri/crates/trigger-git/src/git/testutil.rs
  - src-tauri/crates/trigger-git/src/git/testutil_tests.rs
---

## Goal

Tests in `src/git/` can make a real throwaway repository (`TempRepo`), commit to it, clone it, and have it removed when the test ends. It only exists in test builds.

## Read first

- `plans/003-trigger-git/epic.md` the Shape section
- `plans/_context/conventions.md` the Rust section (temp files under `temp_dir()` with a uuid, removed at the end)
- `src-tauri/crates/trigger-git/src/git/testutil.rs` the file you replace
- `src-tauri/crates/trigger-git/src/git.rs:150-170` the end of the file, where the test modules are declared

## Steps

1. Replace the whole of `src-tauri/crates/trigger-git/src/git/testutil.rs` with this. `git_available` is unchanged from T10.

```rust
//! Helpers for the tests of `git.rs`. Tests that need git skip themselves with
//! a message when it is not installed, so `cargo test` passes on any machine.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{run, GitOutput, TIMEOUT};

/// Commands in tests (clone, fetch, merge) get longer than the 3 s default so
/// a busy machine does not make them flaky.
const TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// True when `git --version` runs. A test that needs git starts with
/// `if !git_available() { return; }`; the message goes to stderr.
pub fn git_available() -> bool {
    let ok = run(&std::env::temp_dir(), &["--version"], TIMEOUT)
        .map(|o| o.ok)
        .unwrap_or(false);
    if !ok {
        eprintln!("skipped: git is not installed");
    }
    ok
}

/// A path for a scratch directory inside the system temp dir. Not created.
pub fn scratch_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("trigger-git-{label}-{}", uuid::Uuid::new_v4()))
}

/// A throwaway repository on branch `main` with a committer set, removed when
/// dropped. Tests build the situation they need with `git`, `write` and
/// `commit`.
pub struct TempRepo {
    pub path: PathBuf,
}

impl TempRepo {
    pub fn new() -> TempRepo {
        let path = scratch_path("repo");
        std::fs::create_dir_all(&path).unwrap();
        let repo = TempRepo { path };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.configure();
        repo
    }

    fn configure(&self) {
        for (key, value) in [
            ("user.name", "Test"),
            ("user.email", "test@example.com"),
            ("commit.gpgsign", "false"),
            ("core.autocrlf", "false"),
        ] {
            self.git(&["config", key, value]);
        }
    }

    /// Runs git here and returns the output even when it exits non zero.
    pub fn try_git(&self, args: &[&str]) -> GitOutput {
        run(&self.path, args, TEST_TIMEOUT).unwrap()
    }

    /// Runs git here, panics if it fails, returns stdout without the final
    /// line break.
    pub fn git(&self, args: &[&str]) -> String {
        let out = self.try_git(args);
        assert!(out.ok, "git {args:?} failed: {}", out.stderr);
        out.stdout.trim_end().to_string()
    }

    /// Writes `content` to `name` under the work tree, making folders.
    pub fn write(&self, name: &str, content: &str) {
        let file = self.path.join(name);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, content).unwrap();
    }

    /// Writes `name` with the message as its content (so every commit really
    /// changes something), stages it and commits.
    pub fn commit(&self, name: &str, message: &str) {
        self.write(name, &format!("{message}\n"));
        self.git(&["add", name]);
        self.git(&["commit", "-q", "-m", message]);
    }

    /// Clones this repository into a new temporary one. The clone tracks
    /// `origin/main`. Keep `self` alive while the clone is used.
    pub fn clone_repo(&self) -> TempRepo {
        let path = scratch_path("clone");
        let source = self.path.to_str().unwrap();
        let target = path.to_str().unwrap();
        let out = run(
            &std::env::temp_dir(),
            &["clone", "-q", source, target],
            TEST_TIMEOUT,
        )
        .unwrap();
        assert!(out.ok, "clone failed: {}", out.stderr);
        let repo = TempRepo { path };
        repo.configure();
        repo
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
```

2. Create `src-tauri/crates/trigger-git/src/git/testutil_tests.rs` with this.

```rust
use super::testutil::{git_available, scratch_path, TempRepo};

#[test]
fn a_temp_repo_starts_on_main_and_can_commit() {
    if !git_available() {
        return;
    }
    let repo = TempRepo::new();
    assert_eq!(repo.git(&["symbolic-ref", "--short", "HEAD"]), "main");
    repo.commit("a.txt", "first");
    assert_eq!(repo.git(&["rev-parse", "HEAD"]).len(), 40);
    assert_eq!(repo.git(&["log", "-1", "--format=%s"]), "first");
}

#[test]
fn try_git_reports_failure_without_panicking() {
    if !git_available() {
        return;
    }
    let repo = TempRepo::new();
    assert!(!repo.try_git(&["no-such-subcommand"]).ok);
}

#[test]
fn a_clone_tracks_origin_and_both_are_removed_on_drop() {
    if !git_available() {
        return;
    }
    let origin = TempRepo::new();
    origin.commit("a.txt", "first");
    let clone = origin.clone_repo();
    assert_eq!(
        clone.git(&["rev-parse", "--abbrev-ref", "@{u}"]),
        "origin/main"
    );
    let (origin_path, clone_path) = (origin.path.clone(), clone.path.clone());
    drop(clone);
    drop(origin);
    assert!(!origin_path.exists());
    assert!(!clone_path.exists());
}

#[test]
fn scratch_paths_are_unique_and_not_created() {
    let (a, b) = (scratch_path("x"), scratch_path("x"));
    assert_ne!(a, b);
    assert!(!a.exists());
}
```

3. In `src-tauri/crates/trigger-git/src/git.rs`, add these two lines at the very end of the file, after a blank line.

```rust
#[cfg(test)]
mod testutil_tests;
```

4. Run `cd src-tauri && cargo fmt -p trigger-git`. It may reorder the `#[cfg(test)]` module lines at the end of `git.rs`; that is fine.

Note on `check-crates.mjs`: it prints warnings, not errors, about template text. Until slice C it may print `README.md still has template text`, `src/lib.rs still has template code`, `src/os.rs still has template code` and a length warning for `src/logic/tests.rs`. Warnings are fine; only errors fail the check. Do not try to remove them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes: 51 tests in the library, 1 in `readme`, `live` shows 1 ignored
- [ ] `cd src-tauri && cargo test -p trigger-git testutil_tests` runs 4 tests and they pass
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- Any method on `TempRepo` that a test in this card does not use. Later cards add what they need (T13 to T16 use only what is here).
- Using `TempRepo` outside `src/git/`. The live tests in `tests/live.rs` (slice C) write their own helper, because an integration test cannot see crate private test code.
- Changing `run`, `value` or the production part of `git.rs`.
- README changes.

## If stuck

- If `git init -q -b main` fails with `unknown switch`, the machine has git older than 2.28. Set status blocked and say which version `git --version` printed.
- If `a_clone_tracks_origin_and_both_are_removed_on_drop` fails because `@{u}` has no upstream, set status blocked and quote the output.
- If git is not installed, the tests print `skipped: git is not installed` and pass. Add a Log line saying so.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
