---
id: T28
epic: 003-trigger-git
title: Write the live tests on real repositories
status: todo
depends_on: [T27]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/tests/live.rs
  - src-tauri/crates/trigger-git/README.md
---

## Goal

`tests/live.rs` has seven `#[ignore]` tests that start `GitSource` on real temporary repositories with the real file watcher: a commit, a new branch and a detach, a tag, a conflicted merge, a pull next to a fetch that must fire nothing, a branch glob that does not match, and a commit in a linked worktree. The README says how to run them.

## Read first

- `plans/003-trigger-git/epic.md` the Shape section (the live tests line)
- `src-tauri/crates/trigger-fs/tests/live.rs` the pattern this follows: `arm`, `expect`, a sentinel instead of a sleep
- `src-tauri/crates/trigger-git/tests/live.rs` the template test you replace
- `src-tauri/crates/trigger-core/src/testkit.rs:20-60` `RecordingSink::wait_for` and `drain`
- `src-tauri/crates/trigger-git/README.md` the `## Develop` section

## Steps

1. Replace the whole of `src-tauri/crates/trigger-git/tests/live.rs` with exactly this. An integration test cannot see the crate's test helpers, so `Repo` here is its own small copy of `TempRepo`. It uses `uuid` (already a dev dependency) and `serde_json` (already a dependency).

```rust
//! Tests against a real repository and the real file watcher. Not run by
//! `cargo test`; run them on a machine of each supported platform with:
//!
//!     cargo test -p trigger-git -- --ignored --test-threads=1
//!
//! Each test makes its own repository under the temp dir, arms rules on it,
//! does the real git operation, and waits up to 10 s for the event with
//! `RecordingSink::wait_for`. A "nothing else happened" check first waits for
//! an event that must come after (a sentinel), never a bare sleep. The
//! polled kinds (`behind-remote`, `uncommitted-older-than`, `repo-cloned`)
//! check at most every 30 s, so their logic is tested in `src/` with a fake
//! and not here.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::json;
use trigger_core::testkit::{rule, RecordingSink};
use trigger_core::{Event, Handle, Rule, Source};
use trigger_git::GitSource;

const CEILING: Duration = Duration::from_secs(10);

/// A repository on branch `main` under the temp dir, removed when dropped.
/// Declare it before the `Handle` so the watcher stops first. An integration
/// test cannot see the crate's test helpers, so this is its own copy.
struct Repo {
    path: PathBuf,
}

impl Repo {
    fn new() -> Repo {
        let dir = std::env::temp_dir().join(format!("tg-git-live-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let repo = Repo {
            path: std::fs::canonicalize(&dir).unwrap(),
        };
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

    /// Runs git here and returns whether it exited zero.
    fn try_git(&self, args: &[&str]) -> bool {
        Command::new("git")
            .args(args)
            .current_dir(&self.path)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_PAGER", "cat")
            .env("LC_ALL", "C")
            .output()
            .unwrap()
            .status
            .success()
    }

    /// Runs git here and panics if it fails.
    fn git(&self, args: &[&str]) {
        assert!(self.try_git(args), "git {args:?} failed");
    }

    /// Writes `message` into `name`, stages it and commits with that message.
    fn commit(&self, name: &str, message: &str) {
        std::fs::write(self.path.join(name), format!("{message}\n")).unwrap();
        self.git(&["add", name]);
        self.git(&["commit", "-q", "-m", message]);
    }

    /// Clones this repository. The clone tracks `origin/main`.
    fn clone_repo(&self) -> Repo {
        let target = std::env::temp_dir().join(format!("tg-git-live-{}", uuid::Uuid::new_v4()));
        let ok = Command::new("git")
            .args(["clone", "-q"])
            .arg(&self.path)
            .arg(&target)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git clone failed");
        let repo = Repo {
            path: std::fs::canonicalize(&target).unwrap(),
        };
        repo.configure();
        repo
    }

    fn text(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn git_rule(id: &str, kind: &str, repo: &Path, extra: serde_json::Value) -> Rule {
    let mut when = json!({ "repo": repo });
    when.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    rule(id, kind, when)
}

/// Arms the rules and waits 500 ms, because some watchers start
/// asynchronously and lose events from the first moments. The baseline
/// snapshot is already taken when `start` returns.
fn arm(rules: Vec<Rule>) -> (RecordingSink, Handle) {
    let sink = RecordingSink::new();
    let handle = GitSource.start(rules, sink.tx.clone()).expect("start");
    std::thread::sleep(Duration::from_millis(500));
    (sink, handle)
}

fn expect(sink: &RecordingSink, kind: &str) -> Event {
    sink.wait_for(kind, CEILING)
        .unwrap_or_else(|| panic!("no {kind} within 10 s"))
}

fn text(event: &Event, field: &str) -> String {
    event.payload[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not text in {event:?}"))
        .to_string()
}

#[test]
#[ignore]
fn a_commit_fires_git_commit() {
    let repo = Repo::new();
    repo.commit("a.txt", "first");
    let (sink, _handle) = arm(vec![git_rule("p/t", "git.commit", &repo.path, json!({}))]);

    repo.commit("b.txt", "second");
    let event = expect(&sink, "git.commit");
    assert_eq!(event.rule, "p/t");
    assert_eq!(text(&event, "repo"), repo.text());
    assert_eq!(text(&event, "branch"), "main");
    assert_eq!(text(&event, "message"), "second");
    assert!(!text(&event, "commit").is_empty());
}

#[test]
#[ignore]
fn a_new_branch_fires_branch_switched_and_a_detach_fires_detached_head() {
    let repo = Repo::new();
    repo.commit("a.txt", "first");
    let (sink, _handle) = arm(vec![
        git_rule("p/s", "git.branch-switched", &repo.path, json!({})),
        git_rule("p/d", "git.detached-head", &repo.path, json!({})),
    ]);

    repo.git(&["checkout", "-q", "-b", "feature"]);
    let event = expect(&sink, "git.branch-switched");
    assert_eq!(text(&event, "from"), "main");
    assert_eq!(text(&event, "to"), "feature");
    assert_eq!(text(&event, "branch"), "feature");

    repo.git(&["checkout", "-q", "--detach"]);
    let event = expect(&sink, "git.detached-head");
    assert_eq!(text(&event, "branch"), "feature");
}

#[test]
#[ignore]
fn a_tag_fires_tag_created() {
    let repo = Repo::new();
    repo.commit("a.txt", "first");
    let (sink, _handle) = arm(vec![git_rule(
        "p/t",
        "git.tag-created",
        &repo.path,
        json!({}),
    )]);

    repo.git(&["tag", "v1.0"]);
    let event = expect(&sink, "git.tag-created");
    assert_eq!(text(&event, "tag"), "v1.0");
    assert_eq!(text(&event, "branch"), "main");
}

#[test]
#[ignore]
fn a_conflicted_merge_fires_merge_conflict_with_the_file_count() {
    let repo = Repo::new();
    repo.commit("a.txt", "base");
    repo.git(&["checkout", "-q", "-b", "other"]);
    repo.commit("a.txt", "other side");
    repo.git(&["checkout", "-q", "main"]);
    repo.commit("a.txt", "main side");
    let (sink, _handle) = arm(vec![git_rule(
        "p/m",
        "git.merge-conflict",
        &repo.path,
        json!({}),
    )]);

    // The merge stops with a conflict, so git exits non zero.
    assert!(!repo.try_git(&["merge", "other"]));
    let event = expect(&sink, "git.merge-conflict");
    assert_eq!(event.payload["files"], json!(1));
    assert_eq!(text(&event, "branch"), "main");
}

#[test]
#[ignore]
fn a_pull_fires_pulled_but_a_fetch_alone_fires_nothing() {
    let origin = Repo::new();
    origin.commit("a.txt", "first");
    let clone = origin.clone_repo();
    let (sink, _handle) = arm(vec![
        git_rule("p/p", "git.pulled", &clone.path, json!({})),
        git_rule("p/c", "git.commit", &clone.path, json!({})),
        git_rule("p/t", "git.tag-created", &clone.path, json!({})),
    ]);

    // A fetch moves only remote refs and FETCH_HEAD. The tag after it is the
    // sentinel: when it arrives, the fetch has been seen and ignored.
    origin.commit("b.txt", "second");
    clone.git(&["fetch", "-q"]);
    clone.git(&["tag", "sentinel"]);
    expect(&sink, "git.tag-created");
    assert!(sink
        .drain()
        .iter()
        .all(|e| e.kind != "git.pulled" && e.kind != "git.commit"));

    clone.git(&["pull", "-q", "--ff-only"]);
    let event = expect(&sink, "git.pulled");
    assert_eq!(text(&event, "message"), "second");
}

#[test]
#[ignore]
fn a_branch_glob_that_does_not_match_stays_quiet() {
    let repo = Repo::new();
    repo.commit("a.txt", "first");
    let (sink, _handle) = arm(vec![
        git_rule(
            "p/c",
            "git.commit",
            &repo.path,
            json!({ "branch": "release/*" }),
        ),
        git_rule("p/t", "git.tag-created", &repo.path, json!({})),
    ]);

    repo.commit("b.txt", "second");
    repo.git(&["tag", "sentinel"]);
    expect(&sink, "git.tag-created");
    assert!(sink.drain().iter().all(|e| e.kind != "git.commit"));
}

#[test]
#[ignore]
fn a_commit_in_a_linked_worktree_fires_for_that_worktree() {
    let repo = Repo::new();
    repo.commit("a.txt", "first");
    let wt_path = std::env::temp_dir().join(format!("tg-git-live-{}", uuid::Uuid::new_v4()));
    repo.git(&["worktree", "add", "-q", wt_path.to_str().unwrap(), "-b", "wtb"]);
    let wt = Repo {
        path: std::fs::canonicalize(&wt_path).unwrap(),
    };
    let (sink, _handle) = arm(vec![git_rule("p/w", "git.commit", &wt.path, json!({}))]);

    wt.commit("w.txt", "in the worktree");
    let event = expect(&sink, "git.commit");
    assert_eq!(text(&event, "branch"), "wtb");
    assert_eq!(text(&event, "message"), "in the worktree");
}
```

2. Run `cd src-tauri && cargo fmt -p trigger-git`.

3. Run the live tests on this machine: `cd src-tauri && cargo test -p trigger-git --test live -- --ignored --test-threads=1`. They take about 10 seconds. They must pass here; the card is not done until they do.

4. README. In `src-tauri/crates/trigger-git/README.md`, under `## Develop`:

   a. Replace the whole row that starts ``| `tests/live.rs` |`` with:

```text
| `tests/live.rs` | `#[ignore]` tests on real temporary repositories: commit, branch switch, detach, tag, merge conflict, pull, fetch, a branch glob and a linked worktree |
```

   b. In the code block under the table, replace the line `cargo test -p trigger-git -- --ignored          # live tests on this machine` with:

```text
cargo test -p trigger-git -- --ignored --test-threads=1   # live tests on this machine
```

Note on `check-crates.mjs`: it prints warnings, not errors. A warning about `src/logic/tests.rs` being long is expected and stays. Only errors fail the check.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes, and the `live` line says `0 passed; 0 failed; 7 ignored`
- [ ] `cd src-tauri && cargo test -p trigger-git --test live -- --ignored --test-threads=1` passes: `7 passed`
- [ ] `cd src-tauri && cargo test -p trigger-git -- --ignored --test-threads=1` passes (the library line shows `1 passed`, the `live` line `7 passed`)
- [ ] `grep -c "test-threads=1" src-tauri/crates/trigger-git/README.md` prints `1`
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- Live tests for `behind-remote`, `uncommitted-older-than` or `repo-cloned`. They poll at most every 30 seconds; their logic is tested with fakes.
- A live test for a stopped rebase (a short rebase is coalesced by the settle time, a Known limit that T29 writes).
- Making the timeouts longer than 10 seconds or adding bare `sleep` calls to wait for an event. Use `expect` and a sentinel.
- Changing anything in `src/`.
- Running the live tests in parallel. They run with `--test-threads=1`.

## If stuck

- If a live test fails with `no <kind> within 10 s`, run that test alone once more: `cd src-tauri && cargo test -p trigger-git --test live <test name> -- --ignored --test-threads=1`. If it fails again set status blocked and quote the output. Do not change the test or `src/`.
- If `git init -q -b main` fails (git older than 2.28), run `git --version`, quote it in the Log, and set status blocked.
- If `git commit` fails because of a global hook or signing setting, quote the message and set status blocked.
- If `git pull -q --ff-only` fails in `a_pull_fires_pulled_but_a_fetch_alone_fires_nothing`, quote the message and set status blocked.
- If the README has no row starting ``| `tests/live.rs` |``, T27 was built differently from its card. Set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
