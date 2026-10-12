---
id: T22
epic: 011-trigger-github
title: Find gh once and report availability
status: done
depends_on: [T21]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-github/src/os.rs
  - src-tauri/crates/trigger-github/src/os/find_tests.rs
  - src-tauri/crates/trigger-github/README.md
---

## Goal

`os.rs` has `find_gh()`: the first `gh` file in the folders of `candidate_dirs`, looked up once and kept in a `OnceLock` (Decision 6), and `availability()` now says `Available` when `gh` was found and `Unsupported` with the download address when it was not. It starts no process, because the catalogue command calls it on the UI thread. The README Platforms table says how each platform finds `gh`.

## Read first

- `plans/011-trigger-github/epic.md` Decision 6 (the folders, the `OnceLock`, why `availability()` only looks for the binary)
- `src-tauri/crates/trigger-github/src/logic/cli.rs` `candidate_dirs`, `exe_name` (created by T20)
- `src-tauri/crates/trigger-github/src/os.rs` the file to edit (T21)
- `src-tauri/crates/trigger-github/README.md` the Platforms table (a stub with empty cells)

## Steps

1. In `src-tauri/crates/trigger-github/src/os.rs`, make this edit.

Find this text, which appears once:

```rust
use std::path::Path;
```

Replace it with:

```rust
use std::path::{Path, PathBuf};
```

2. In `src-tauri/crates/trigger-github/src/os.rs`, make this edit.

Find this text, which appears once:

```rust
use std::sync::mpsc;
```

Replace it with:

```rust
use std::sync::{mpsc, OnceLock};
```

3. In `src-tauri/crates/trigger-github/src/os.rs`, make this edit. It replaces the stub `availability`.

`find_gh` reads `PATH` and the home folder (`HOME`, or `USERPROFILE` on Windows) and asks `candidate_dirs` for the folders. A negative answer is kept too, so a machine without `gh` pays for the lookup once. `first_file` only counts files, not folders called `gh`.

Find this text, which appears once:

```rust
use trigger_core::Availability;

pub fn availability() -> Availability {
    Availability::Unsupported {
        reason: "not implemented yet".into(),
    }
}
```

Replace it with:

```rust
use trigger_core::Availability;

use crate::logic::cli::{candidate_dirs, exe_name};

/// Why the source is unavailable when `gh` is not installed.
pub const NOT_FOUND: &str = "GitHub CLI not found. Install it from https://cli.github.com";

static GH: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Where `gh` is: the first `gh` file in the folders of `candidate_dirs`. It
/// is looked up once and remembered, found or not, so it costs nothing after
/// the first call. Installing `gh` while the app runs is noticed on restart.
pub fn find_gh() -> Option<&'static Path> {
    GH.get_or_init(|| {
        let path_var = std::env::var_os("PATH");
        let home =
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
        let dirs = candidate_dirs(path_var.as_deref(), home.as_deref(), cfg!(windows));
        first_file(&dirs, exe_name(cfg!(windows)))
    })
    .as_deref()
}

/// The first of `dirs` that holds a file called `name`.
fn first_file(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    dirs.iter().map(|dir| dir.join(name)).find(|p| p.is_file())
}

/// Whether `gh` was found. It starts no child process, because the catalogue
/// command calls it on the UI thread. Whether `gh` is signed in is checked
/// when the source starts.
pub fn availability() -> Availability {
    match find_gh() {
        Some(_) => Availability::Available,
        None => Availability::Unsupported {
            reason: NOT_FOUND.into(),
        },
    }
}
```

4. In `src-tauri/crates/trigger-github/src/os.rs`, make this edit. It loads the new test file.

Find this text, which appears once:

```rust
#[cfg(test)]
mod run_tests;
```

Replace it with:

```rust
#[cfg(test)]
mod find_tests;
#[cfg(test)]
mod run_tests;
```

5. Create `src-tauri/crates/trigger-github/src/os/find_tests.rs` with exactly this. These tests make no assumption about whether the machine has `gh`.

```rust
//! Tests for `find_gh`, `first_file` and `availability`.

use super::*;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tg-github-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_first_folder_with_the_file_wins() {
    let (a, b, c) = (temp("find-a"), temp("find-b"), temp("find-c"));
    std::fs::write(b.join("gh"), "").unwrap();
    std::fs::write(c.join("gh"), "").unwrap();
    let got = first_file(&[a.clone(), b.clone(), c.clone()], "gh");
    assert_eq!(got, Some(b.join("gh")));
    for dir in [a, b, c] {
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn a_folder_called_gh_is_not_the_program() {
    let a = temp("dir-a");
    std::fs::create_dir(a.join("gh")).unwrap();
    assert_eq!(first_file(std::slice::from_ref(&a), "gh"), None);
    assert_eq!(first_file(&[], "gh"), None);
    std::fs::remove_dir_all(a).unwrap();
}

#[test]
fn availability_follows_whether_gh_was_found() {
    match find_gh() {
        Some(path) => {
            assert!(path.is_file());
            assert_eq!(availability(), Availability::Available);
        }
        None => assert_eq!(
            availability(),
            Availability::Unsupported {
                reason: NOT_FOUND.into()
            }
        ),
    }
}

#[test]
fn the_lookup_is_remembered() {
    assert_eq!(find_gh(), find_gh());
}

#[test]
fn the_not_found_text_names_the_download_page() {
    assert!(NOT_FOUND.contains("https://cli.github.com"));
}
```

6. In `src-tauri/crates/trigger-github/README.md`, make this edit. It fills the Platforms table. Do not touch any other section.

Find this text, which appears once:

```
| macOS | | |
| Windows | | |
| Linux | | |
```

Replace it with:

```
| macOS | `gh` on the PATH, else in `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin` or `~/.local/bin`. A desktop app starts with a short PATH, which is why Homebrew's folder is listed | No login shell is asked for its PATH. The login itself (keychain) does not need the shell's PATH |
| Windows | `gh.exe` on the PATH, else in `C:\Program Files\GitHub CLI` | `gh` runs without a console window |
| Linux | `gh` on the PATH, else in `/usr/local/bin`, `/usr/bin` or `~/.local/bin` | |

Where `gh` is looked up once per run of the app and remembered. Installing it
while the app runs is noticed after a restart. Availability only says whether
`gh` was found; whether it is signed in is checked when the first rule starts.
```

7. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib os::find_tests` passes with 5 tests
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (the lib reports 128 tests on macOS and Linux)
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (a warning that `lib.rs` still has template code is expected until a later card)
- [ ] `grep -c "not implemented yet" src-tauri/crates/trigger-github/src/os.rs` prints `0`
- [ ] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- Asking whether `gh` is signed in (`auth_ok`, the next card). `availability()` must not start a process.
- A login shell probe, or asking the app for the PATH.
- Re-checking for `gh` on a timer. It is looked up once.
- The other README sections. Permissions, Develop and Known limits are filled in by later cards.

## If stuck

- If a find text in steps 1 to 4 is not in `os.rs`, T21 did not leave the file as its card says. Set status blocked and say which edit.
- If the find text in step 6 is not in the README, the Platforms table was already filled in. Set status blocked and say so.
- If `cargo clippy` reports `cloned_ref_to_slice_refs` in `find_tests.rs`, the file differs from step 5. Copy it again.
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
