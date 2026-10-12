---
id: T23
epic: 011-trigger-github
title: Add GhCli and the sign in check
status: done
depends_on: [T22]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-github/src/os.rs
  - src-tauri/crates/trigger-github/src/os/cli_tests.rs
---

## Goal

`os.rs` has `GhCli`, the real implementation of the `Gh` trait (`get` runs `gh api -i ...` with a 30 second timeout and parses stdout with `parse_gh_output`; `login` reads `gh api user`), and `auth_ok(exe)`, which runs `gh auth status --json hosts` with a 10 second timeout (Decision 7). Seven tests run both against a stand in for `gh`: a shell script that prints what `gh` prints. Nothing calls them yet.

## Read first

- `plans/011-trigger-github/epic.md` Decisions 7 and 8 (the timeouts, what a timeout or spawn failure means, never `gh auth token`)
- `plans/011-trigger-github/research/R02-rate-limits-and-gh.md` Evidence: the 304, 404, bad token, not logged in and `gh auth status --json hosts` results (what the stand in scripts imitate)
- `src-tauri/crates/trigger-github/src/os.rs` the file to edit: `run`, `RunError`, `spawn_lock` (T21, T22)
- `src-tauri/crates/trigger-github/src/logic/cli.rs` `Gh`, `api_args`, `signed_in`, `login_from_body`, `AUTH_ARGS` (T20)
- `src-tauri/crates/trigger-github/src/logic/response.rs` `parse_gh_output`, `GhResponse`, `NOT_LOGGED_IN` (T10)

## Steps

1. In `src-tauri/crates/trigger-github/src/os.rs`, make this edit. It brings in what `GhCli` needs.

Find this text, which appears once:

```rust
use crate::logic::cli::{candidate_dirs, exe_name};
```

Replace it with:

```rust
use crate::logic::cli::{
    api_args, candidate_dirs, exe_name, login_from_body, signed_in, Gh, AUTH_ARGS,
};
use crate::logic::response::{parse_gh_output, GhResponse};
```

2. In `src-tauri/crates/trigger-github/src/os.rs`, make this edit. It adds `GhCli` and `auth_ok` just above `no_window`.

What they do:

- `GhCli::get` turns every way a request can fail to give an answer into `GhResponse::Transport` with a fixed sentence: a timeout, or a program that would not start. The sentences never contain anything `gh` printed.
- `GhCli::login` is `get("user")` and reads `login` from a 200 body. Anything else is `None`.
- `auth_ok` is `Ok(true)` only for exit 0 with a successful `github.com` login in the JSON. `Ok(false)` is "not signed in". A timeout, a program that will not start or an exit code other than 0 is `Err`, because none of those mean "not signed in". The output is read and thrown away; it is never logged.

Find this text, which appears once:

```rust
#[cfg(windows)]
fn no_window(command: &mut Command) {
```

Replace it with:

```rust
/// How long one request may take before it counts as no answer.
const GET_TIMEOUT: Duration = Duration::from_secs(30);
/// `gh auth status` makes a network call to check the token, so it gets less
/// time than a request but more than a local command.
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);

/// The real `gh`: one child process per request, one at a time because the
/// poll loop is serial.
pub struct GhCli {
    exe: PathBuf,
}

impl GhCli {
    pub fn at(exe: &Path) -> GhCli {
        GhCli {
            exe: exe.to_path_buf(),
        }
    }
}

impl Gh for GhCli {
    fn get(&self, path: &str, etag: Option<&str>) -> GhResponse {
        match run(&self.exe, &api_args(path, etag), GET_TIMEOUT) {
            Ok(done) => parse_gh_output(&done.stdout, done.exit),
            Err(RunError::TimedOut) => {
                GhResponse::Transport("gh did not answer within 30 seconds".into())
            }
            Err(RunError::Spawn(why)) => GhResponse::Transport(format!("could not run gh: {why}")),
        }
    }

    fn login(&self) -> Option<String> {
        match self.get("user", None) {
            GhResponse::Ok { body, .. } => login_from_body(&body),
            _ => None,
        }
    }
}

/// Whether `gh` is signed in to github.com, from `gh auth status --json
/// hosts`. Ok(false) means no login. Err is anything that stops the question
/// being answered (could not start, too slow, exit code other than 0), which
/// is not the same thing and must not be shown as "not signed in". The output
/// is read but never logged or kept.
pub fn auth_ok(exe: &Path) -> Result<bool, String> {
    let args: Vec<String> = AUTH_ARGS.iter().map(|s| s.to_string()).collect();
    let done = run(exe, &args, AUTH_TIMEOUT).map_err(|e| match e {
        RunError::TimedOut => "gh auth status did not answer within 10 seconds".to_string(),
        RunError::Spawn(why) => format!("could not run gh: {why}"),
    })?;
    match done.exit {
        Some(0) => Ok(signed_in(&String::from_utf8_lossy(&done.stdout))),
        Some(code) => Err(format!("gh auth status exited with code {code}")),
        None => Err("gh auth status was stopped before it answered".to_string()),
    }
}

#[cfg(windows)]
fn no_window(command: &mut Command) {
```

3. In `src-tauri/crates/trigger-github/src/os.rs`, make this edit. It loads the new test file.

Find this text, which appears once:

```rust
#[cfg(test)]
mod find_tests;
```

Replace it with:

```rust
#[cfg(test)]
mod cli_tests;
#[cfg(test)]
mod find_tests;
```

4. Create `src-tauri/crates/trigger-github/src/os/cli_tests.rs` with exactly this. The stand in is a shell script written to a temp folder, so these tests only exist on Unix. Each takes `spawn_lock()` first.

```rust
//! `GhCli` and `auth_ok` against a stand in for `gh`: a shell script that
//! prints what `gh api -i` and `gh auth status` print. Unix only.

#![cfg(unix)]

use super::*;
use std::os::unix::fs::PermissionsExt;

/// Writes `gh` into a fresh folder and returns the folder and the file.
fn fake_gh(name: &str, script: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("tg-github-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("gh");
    std::fs::write(&exe, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, exe)
}

#[test]
fn a_200_is_parsed_and_gh_is_given_the_right_arguments_and_environment() {
    let _guard = spawn_lock();
    let (dir, exe) = fake_gh(
        "get-200",
        r#"printf 'HTTP/2.0 200 OK\r\nEtag: "e1"\r\n\r\n%s|%s|%s' "$GH_PROMPT_DISABLED" "$GH_NO_UPDATE_NOTIFIER" "$*""#,
    );
    let got = GhCli::at(&exe).get("repos/o/r/pulls?per_page=100", Some("W/\"a\""));
    std::fs::remove_dir_all(dir).unwrap();
    match got {
        GhResponse::Ok { etag, body, .. } => {
            assert_eq!(etag.as_deref(), Some("\"e1\""));
            assert_eq!(
                body,
                "1|1|api -i -H If-None-Match: W/\"a\" repos/o/r/pulls?per_page=100"
            );
        }
        other => panic!("expected Ok, got {other:?}"),
    }
}

#[test]
fn a_304_that_gh_reports_with_exit_1_is_not_modified() {
    let _guard = spawn_lock();
    let (dir, exe) = fake_gh(
        "get-304",
        r#"printf 'HTTP/2.0 304 Not Modified\r\nEtag: "e1"\r\n\r\n'; exit 1"#,
    );
    let got = GhCli::at(&exe).get("repos/o/r/pulls", Some("\"e1\""));
    std::fs::remove_dir_all(dir).unwrap();
    assert!(matches!(got, GhResponse::NotModified { .. }), "{got:?}");
}

#[test]
fn no_login_is_the_not_logged_in_transport_error() {
    let _guard = spawn_lock();
    let (dir, exe) = fake_gh("get-exit4", "echo 'run gh auth login' >&2; exit 4");
    let got = GhCli::at(&exe).get("user", None);
    std::fs::remove_dir_all(dir).unwrap();
    assert_eq!(
        got,
        GhResponse::Transport(crate::logic::response::NOT_LOGGED_IN.into())
    );
}

#[test]
fn a_program_that_cannot_start_is_a_transport_error() {
    let _guard = spawn_lock();
    let missing = std::env::temp_dir().join("tg-github-missing-gh");
    let got = GhCli::at(&missing).get("user", None);
    match got {
        GhResponse::Transport(message) => assert!(message.starts_with("could not run gh")),
        other => panic!("expected Transport, got {other:?}"),
    }
}

#[test]
fn login_reads_the_user_and_gives_none_for_anything_else() {
    let _guard = spawn_lock();
    let (dir, exe) = fake_gh(
        "login-ok",
        r#"printf 'HTTP/2.0 200 OK\r\n\r\n{"login":"octocat"}'"#,
    );
    assert_eq!(GhCli::at(&exe).login(), Some("octocat".to_string()));
    std::fs::remove_dir_all(dir).unwrap();

    let (dir, exe) = fake_gh(
        "login-401",
        r#"printf 'HTTP/2.0 401 Unauthorized\r\n\r\n{"message":"Bad credentials"}'; exit 1"#,
    );
    assert_eq!(GhCli::at(&exe).login(), None);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn auth_ok_asks_for_hosts_json_and_reads_the_state() {
    let _guard = spawn_lock();
    let check = r#"[ "$*" = "auth status --json hosts" ] || exit 9"#;
    let (dir, exe) = fake_gh(
        "auth-yes",
        &format!(
            "{check}\nprintf '%s' '{}'",
            r#"{"hosts":{"github.com":[{"state":"success","login":"octocat"}]}}"#
        ),
    );
    assert_eq!(auth_ok(&exe), Ok(true));
    std::fs::remove_dir_all(dir).unwrap();

    let (dir, exe) = fake_gh(
        "auth-no",
        &format!("{check}\nprintf '%s' '{{\"hosts\":{{}}}}'"),
    );
    assert_eq!(auth_ok(&exe), Ok(false));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn auth_ok_is_an_error_when_the_question_cannot_be_answered() {
    let _guard = spawn_lock();
    let (dir, exe) = fake_gh("auth-exit1", "exit 1");
    let got = auth_ok(&exe);
    std::fs::remove_dir_all(dir).unwrap();
    assert_eq!(got, Err("gh auth status exited with code 1".to_string()));

    let missing = std::env::temp_dir().join("tg-github-missing-gh");
    let got = auth_ok(&missing);
    assert!(got.unwrap_err().starts_with("could not run gh"));
}
```

5. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib os::cli_tests` passes with 7 tests (none on Windows)
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (the lib reports 135 tests on macOS and Linux)
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (a warning that `lib.rs` still has template code is expected until a later card)
- [ ] `grep -rn "auth token\|show-token" src-tauri/crates/trigger-github/src` prints nothing
- [ ] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- Calling `auth_ok` or `GhCli` from `start` (T28).
- Running the real `gh` in these tests. The live tests do that (T27).
- Making the 30 second timeout configurable, or testing it. `run` already tests timeouts with a short one.
- Reading or logging stdout or stderr of `gh auth status`. It prints a masked token prefix in its plain mode.
- Removing `#![allow(dead_code)]` from `os.rs`.

## If stuck

- If a find text in steps 1 to 3 is not in `os.rs`, T21 or T22 did not leave the file as its card says. Set status blocked and say which edit.
- If a test fails with "text file busy", it is missing `let _guard = spawn_lock();`. Compare `cli_tests.rs` with step 4.
- If a test fails with "Permission denied", the script was not made executable. Compare `fake_gh` in `cli_tests.rs` with step 4 (the `set_permissions` line).
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
