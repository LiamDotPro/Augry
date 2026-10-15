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
    repo.git(&[
        "worktree",
        "add",
        "-q",
        wt_path.to_str().unwrap(),
        "-b",
        "wtb",
    ]);
    let wt = Repo {
        path: std::fs::canonicalize(&wt_path).unwrap(),
    };
    let (sink, _handle) = arm(vec![git_rule("p/w", "git.commit", &wt.path, json!({}))]);

    wt.commit("w.txt", "in the worktree");
    let event = expect(&sink, "git.commit");
    assert_eq!(text(&event, "branch"), "wtb");
    assert_eq!(text(&event, "message"), "in the worktree");
}
