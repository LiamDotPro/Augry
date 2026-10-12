---
id: T20
epic: 011-trigger-github
title: Add the Gh trait and the pure gh helpers
status: done
depends_on: [T19]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-github/src/logic/cli.rs
  - src-tauri/crates/trigger-github/src/logic.rs
---

## Goal

The crate has `logic/cli.rs`: the `Gh` trait from Decision 8 (what the poll loop needs from `gh`) and the pure half of running `gh`: where to look for the program (`candidate_dirs`), the arguments of a request (`api_args`), how to read `gh auth status --json hosts` (`signed_in`) and the body of `gh api user` (`login_from_body`). Nine unit tests prove them with plain strings. Nothing starts a process yet and nothing calls these functions yet.

## Read first

- `plans/011-trigger-github/epic.md` Decisions 6, 7 and 8 (where `gh` is looked for, the login check, the `Gh` trait)
- `plans/011-trigger-github/research/R02-rate-limits-and-gh.md` "Availability check" and the `gh auth status --json hosts` results under Evidence (the two JSON shapes `signed_in` must read)
- `src-tauri/crates/trigger-github/src/logic/response.rs` `GhResponse` (the type `Gh::get` returns)
- `src-tauri/crates/trigger-github/src/logic.rs` the file that gets one new `pub mod` line

## Steps

1. Create `src-tauri/crates/trigger-github/src/logic/cli.rs` with exactly this.

How it behaves:

- `candidate_dirs` returns the folders of the PATH first (only absolute ones, so a `.` in the PATH never runs a program from the current folder), then the usual install folders, without repeats. On Windows the usual folder is `C:\Program Files\GitHub CLI`; elsewhere `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin` and `~/.local/bin`. The `windows` argument is a plain `bool` so both lists are tested on any machine.
- `api_args` builds `api -i [-H "If-None-Match: <etag>"] <path>`. The `-i` makes `gh` print the status line and headers on stdout, which `parse_gh_output` reads.
- `signed_in` is true only when `hosts["github.com"]` is a list with a login whose `state` is `success`. Text that is not JSON is false.
- No function here may ask for a token. `AUTH_ARGS` is exactly `auth status --json hosts`.

```rust
//! The pure half of running `gh`: the `Gh` trait the poll loop talks to, where
//! to look for the program, which arguments to give it and how to read the two
//! answers that are not a list (`gh auth status` and `gh api user`). No child
//! processes here: `os.rs` starts them.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::json::{text, Value};
use super::response::GhResponse;

/// What the poll loop needs from `gh`. `os::GhCli` runs the real program; the
/// tests use a fake that serves saved answers.
pub trait Gh: Send {
    /// One `GET`. `path` is relative to the API root, such as
    /// `repos/cli/cli/pulls?per_page=100`. `etag` is sent as `If-None-Match`.
    fn get(&self, path: &str, etag: Option<&str>) -> GhResponse;

    /// The login of the signed in user, from `GET user`. None when it cannot
    /// be read.
    fn login(&self) -> Option<String>;
}

/// The arguments of `gh auth status --json hosts`. It never prints a token and
/// always exits 0 unless something is badly wrong (R02).
pub const AUTH_ARGS: [&str; 4] = ["auth", "status", "--json", "hosts"];

/// The folders `gh` is installed to when the app's own PATH does not have them.
const UNIX_DIRS: [&str; 3] = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"];
const WINDOWS_DIR: &str = r"C:\Program Files\GitHub CLI";

/// The file name of the program.
pub fn exe_name(windows: bool) -> &'static str {
    if windows {
        "gh.exe"
    } else {
        "gh"
    }
}

/// Where to look for `gh`, in order and without repeats: the folders of the
/// PATH (absolute ones only, so a `.` in it never runs a program from the
/// current folder), then the usual install folders. No login shell is asked.
pub fn candidate_dirs(
    path_var: Option<&OsStr>,
    home: Option<&Path>,
    windows: bool,
) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut add = |dir: PathBuf| {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    };
    if let Some(var) = path_var {
        for dir in std::env::split_paths(var).filter(|d| d.is_absolute()) {
            add(dir);
        }
    }
    if windows {
        add(PathBuf::from(WINDOWS_DIR));
    } else {
        for dir in UNIX_DIRS {
            add(PathBuf::from(dir));
        }
        if let Some(home) = home {
            add(home.join(".local").join("bin"));
        }
    }
    dirs
}

/// The arguments of one request: `gh api -i [-H "If-None-Match: <etag>"] <path>`.
/// `-i` makes `gh` print the status line and headers on stdout, which is where
/// `parse_gh_output` reads them from.
pub fn api_args(path: &str, etag: Option<&str>) -> Vec<String> {
    let mut args = vec!["api".to_string(), "-i".to_string()];
    if let Some(etag) = etag {
        args.push("-H".to_string());
        args.push(format!("If-None-Match: {etag}"));
    }
    args.push(path.to_string());
    args
}

/// Reads the stdout of `gh auth status --json hosts`: true when `github.com`
/// has a login in state `success`. Anything else, including text that is not
/// JSON, is false.
pub fn signed_in(stdout: &str) -> bool {
    let hosts = serde_json::from_str::<Value>(stdout).ok().and_then(|v| {
        v.get("hosts")?
            .get("github.com")?
            .as_array()
            .map(|list| list.to_vec())
    });
    hosts
        .unwrap_or_default()
        .iter()
        .any(|login| text(login, "/state") == "success")
}

/// The `login` in the body of `GET user`.
pub fn login_from_body(body: &str) -> Option<String> {
    let user = serde_json::from_str::<Value>(body).ok()?;
    let login = text(&user, "/login");
    (!login.is_empty()).then(|| login.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abs(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    #[test]
    fn the_program_name_depends_on_the_platform() {
        assert_eq!(exe_name(false), "gh");
        assert_eq!(exe_name(true), "gh.exe");
    }

    #[test]
    fn unix_looks_in_path_then_the_usual_folders() {
        let path = std::env::join_paths([abs("a"), abs("b")]).unwrap();
        let dirs = candidate_dirs(Some(&path), Some(Path::new("/home/u")), false);
        assert_eq!(
            dirs,
            [
                abs("a"),
                abs("b"),
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
                PathBuf::from("/usr/bin"),
                Path::new("/home/u").join(".local").join("bin"),
            ]
        );
    }

    #[test]
    fn windows_looks_in_path_then_program_files() {
        let path = std::env::join_paths([abs("a")]).unwrap();
        let dirs = candidate_dirs(Some(&path), Some(Path::new("/home/u")), true);
        assert_eq!(
            dirs,
            [abs("a"), PathBuf::from(r"C:\Program Files\GitHub CLI")]
        );
    }

    #[test]
    fn relative_path_entries_and_repeats_are_dropped() {
        let path = std::env::join_paths([
            PathBuf::from("."),
            PathBuf::from("bin"),
            abs("a"),
            abs("a"),
            PathBuf::from("/usr/bin"),
        ])
        .unwrap();
        let dirs = candidate_dirs(Some(&path), None, false);
        assert_eq!(dirs.iter().filter(|d| **d == abs("a")).count(), 1);
        assert_eq!(
            dirs.iter()
                .filter(|d| d.as_path() == Path::new("/usr/bin"))
                .count(),
            1
        );
        assert!(dirs
            .iter()
            .all(|d| d != Path::new(".") && d != Path::new("bin")));
    }

    #[test]
    fn no_path_and_no_home_still_gives_the_usual_folders() {
        let dirs = candidate_dirs(None, None, false);
        assert_eq!(dirs.len(), 3);
        assert_eq!(dirs[0], PathBuf::from("/opt/homebrew/bin"));
    }

    #[test]
    fn api_args_have_the_etag_header_only_when_there_is_an_etag() {
        assert_eq!(
            api_args("repos/o/r/pulls?per_page=100", None),
            ["api", "-i", "repos/o/r/pulls?per_page=100"]
        );
        assert_eq!(
            api_args("repos/o/r/pulls", Some("W/\"093a24\"")),
            [
                "api",
                "-i",
                "-H",
                "If-None-Match: W/\"093a24\"",
                "repos/o/r/pulls"
            ]
        );
    }

    #[test]
    fn auth_args_never_ask_for_a_token() {
        assert_eq!(AUTH_ARGS.join(" "), "auth status --json hosts");
    }

    #[test]
    fn signed_in_needs_a_successful_github_com_login() {
        let yes = r#"{"hosts":{"github.com":[{"state":"success","active":true,"host":"github.com","login":"octocat"}]}}"#;
        assert!(signed_in(yes));
        let second = r#"{"hosts":{"github.com":[{"state":"error"},{"state":"success"}]}}"#;
        assert!(signed_in(second));
        assert!(!signed_in(r#"{"hosts":{}}"#));
        assert!(!signed_in(
            r#"{"hosts":{"github.com":[{"state":"error"}]}}"#
        ));
        assert!(!signed_in(
            r#"{"hosts":{"example.com":[{"state":"success"}]}}"#
        ));
        assert!(!signed_in(r#"{"hosts":{"github.com":"success"}}"#));
        assert!(!signed_in("You are not logged into any GitHub hosts."));
        assert!(!signed_in(""));
    }

    #[test]
    fn login_is_read_from_the_user_body() {
        assert_eq!(
            login_from_body(r#"{"login":"octocat","id":1}"#),
            Some("octocat".to_string())
        );
        assert_eq!(login_from_body(r#"{"login":""}"#), None);
        assert_eq!(login_from_body(r#"{"message":"Bad credentials"}"#), None);
        assert_eq!(login_from_body("not json"), None);
    }
}
```

2. In `src-tauri/crates/trigger-github/src/logic.rs`, make this edit.

Find this text, which appears once:

```rust
pub mod backoff;
```

Replace it with:

```rust
pub mod backoff;
pub mod cli;
```

3. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib cli` passes with 9 tests
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (the lib reports 115 tests)
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (warnings that `lib.rs` and `os.rs` still have template code are expected until later cards)
- [ ] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- Running `gh` or any child process (the next cards).
- Reading the real PATH or `HOME`: `candidate_dirs` takes them as arguments, `os.rs` will pass the real ones.
- A login shell probe for the PATH. Decision 6 says no.
- Removing `#![allow(dead_code)]` from `logic.rs`. The last card of the slice does that.
- README changes. Nothing the user sees has changed yet.

## If stuck

- If a test in `cli.rs` fails, compare the file with step 1 character by character. Do not change a test.
- If clippy reports `cmp_owned` or similar on a line of `cli.rs`, the file differs from step 1. Copy it again.
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again.
- If the find text in step 2 is not in `logic.rs`, slice B is not as the cards left it. Set status blocked and say so.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
