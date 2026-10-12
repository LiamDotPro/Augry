use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use notify::event::{AccessKind, CreateKind, DataChange, Flag, ModifyKind, RemoveKind, RenameMode};
use notify::{Event, EventKind, RecursiveMode};

use super::*;
use crate::git::testutil::{git_available, scratch_path, TempRepo};

fn plain() -> GitDirs {
    GitDirs {
        git_dir: PathBuf::from("/r/.git"),
        common_dir: PathBuf::from("/r/.git"),
    }
}

fn linked() -> GitDirs {
    GitDirs {
        git_dir: PathBuf::from("/main/.git/worktrees/wt"),
        common_dir: PathBuf::from("/main/.git"),
    }
}

fn relevant(dirs: &GitDirs, path: &str) -> bool {
    dirs.is_relevant(&PathBuf::from(path))
}

#[test]
fn the_files_that_change_the_snapshot_are_relevant() {
    let d = plain();
    for path in [
        "/r/.git/HEAD",
        "/r/.git/MERGE_HEAD",
        "/r/.git/packed-refs",
        "/r/.git/rebase-merge",
        "/r/.git/rebase-merge/done",
        "/r/.git/rebase-apply",
        "/r/.git/refs/heads/main",
        "/r/.git/refs/heads/feature/a",
        "/r/.git/refs/tags/v1",
        "/r/.git/refs/stash",
        "/r/.git/reftable/tables.list",
    ] {
        assert!(relevant(&d, path), "{path}");
    }
}

#[test]
fn noise_is_not_relevant() {
    let d = plain();
    for path in [
        "/r/.git/index",
        "/r/.git/COMMIT_EDITMSG",
        "/r/.git/FETCH_HEAD",
        "/r/.git/ORIG_HEAD",
        "/r/.git/logs/HEAD",
        "/r/.git/logs/refs/heads/main",
        "/r/.git/objects/ab/cdef",
        "/r/.git/config",
        "/r/.git/refs/remotes/origin/main",
        "/r/.git",
        "/r/src/main.rs",
        "/elsewhere/HEAD",
    ] {
        assert!(!relevant(&d, path), "{path}");
    }
}

#[test]
fn lock_files_are_never_relevant() {
    let d = plain();
    for path in [
        "/r/.git/HEAD.lock",
        "/r/.git/index.lock",
        "/r/.git/refs/heads/main.lock",
        "/r/.git/refs/stash.lock",
        "/r/.git/packed-refs.lock",
        "/r/.git/reftable/tables.list.lock",
    ] {
        assert!(!relevant(&d, path), "{path}");
    }
}

#[test]
fn a_linked_worktree_uses_both_folders() {
    let d = linked();
    assert!(relevant(&d, "/main/.git/worktrees/wt/HEAD"));
    assert!(relevant(&d, "/main/.git/worktrees/wt/MERGE_HEAD"));
    assert!(relevant(&d, "/main/.git/worktrees/wt/rebase-merge/done"));
    assert!(relevant(&d, "/main/.git/refs/heads/wtb"));
    assert!(relevant(&d, "/main/.git/packed-refs"));
    assert!(!relevant(&d, "/main/.git/worktrees/wt/index"));
    assert!(!relevant(&d, "/main/.git/worktrees/wt/logs/HEAD"));
    // Another worktree's files are not ours.
    assert!(!relevant(&d, "/main/.git/worktrees/other/HEAD"));
    assert!(!relevant(&d, "/main/.git/HEAD"));
}

#[test]
fn the_watch_list_is_folders_with_refs_recursive() {
    let dir = scratch_path("dirs");
    std::fs::create_dir_all(dir.join("refs/heads")).unwrap();
    let d = GitDirs {
        git_dir: dir.clone(),
        common_dir: dir.clone(),
    };
    assert_eq!(
        d.watch_list(),
        vec![
            (dir.clone(), RecursiveMode::NonRecursive),
            (dir.join("refs"), RecursiveMode::Recursive),
        ]
    );
    std::fs::create_dir_all(dir.join("reftable")).unwrap();
    assert_eq!(
        d.watch_list().last(),
        Some(&(dir.join("reftable"), RecursiveMode::NonRecursive))
    );
    let other = GitDirs {
        git_dir: dir.join("worktrees/wt"),
        common_dir: dir.clone(),
    };
    let list = other.watch_list();
    assert_eq!(list[0].0, dir.join("worktrees/wt"));
    assert_eq!(list[1].0, dir);
    assert_eq!(list[2], (dir.join("refs"), RecursiveMode::Recursive));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lock_held_sees_index_and_head_locks_only() {
    let dir = scratch_path("lock");
    std::fs::create_dir_all(&dir).unwrap();
    let d = GitDirs {
        git_dir: dir.clone(),
        common_dir: dir.clone(),
    };
    assert!(!d.lock_held());
    std::fs::write(dir.join("ORIG_HEAD.lock"), "").unwrap();
    assert!(!d.lock_held());
    std::fs::write(dir.join("index.lock"), "").unwrap();
    assert!(d.lock_held());
    std::fs::remove_file(dir.join("index.lock")).unwrap();
    std::fs::write(dir.join("HEAD.lock"), "").unwrap();
    assert!(d.lock_held());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn resolve_finds_the_git_dir_of_a_normal_repository() {
    if !git_available() {
        return;
    }
    let repo = TempRepo::new();
    let dirs = GitDirs::resolve(repo.path()).unwrap();
    let expected = std::fs::canonicalize(repo.path().join(".git")).unwrap();
    assert_eq!(dirs.git_dir, expected);
    assert_eq!(dirs.common_dir, expected);
}

#[test]
fn resolve_splits_the_two_folders_of_a_linked_worktree() {
    if !git_available() {
        return;
    }
    let repo = TempRepo::new();
    repo.commit("a.txt", "first");
    let wt = scratch_path("wt");
    repo.git(&["worktree", "add", "-q", wt.to_str().unwrap(), "-b", "wtb"]);
    let dirs = GitDirs::resolve(&wt).unwrap();
    let main_git = std::fs::canonicalize(repo.path().join(".git")).unwrap();
    assert_eq!(dirs.common_dir, main_git);
    assert!(dirs.git_dir.starts_with(main_git.join("worktrees")));
    assert_ne!(dirs.git_dir, dirs.common_dir);
    let _ = std::fs::remove_dir_all(&wt);
}

#[test]
fn resolve_fails_for_a_folder_that_is_not_a_repository() {
    if !git_available() {
        return;
    }
    let dir = scratch_path("plain");
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(
        GitDirs::resolve(&dir),
        Err("not a git repository".to_string())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn git_is_available_on_a_machine_that_has_it() {
    if !git_available() {
        return;
    }
    assert_eq!(availability(), Availability::Available);
}

fn event(kind: EventKind, paths: &[&str]) -> Event {
    let mut event = Event::new(kind);
    for path in paths {
        event = event.add_path(PathBuf::from(path));
    }
    event
}

#[test]
fn events_matter_by_path_and_kind() {
    let d = plain();
    let create = EventKind::Create(CreateKind::File);
    let data = EventKind::Modify(ModifyKind::Data(DataChange::Content));
    let rename = EventKind::Modify(ModifyKind::Name(RenameMode::Both));
    let remove = EventKind::Remove(RemoveKind::File);
    assert!(event_matters(&d, &event(create, &["/r/.git/refs/tags/v1"])));
    assert!(event_matters(&d, &event(data, &["/r/.git/HEAD"])));
    assert!(event_matters(&d, &event(remove, &["/r/.git/MERGE_HEAD"])));
    // A rename of the lock file onto HEAD names both paths.
    let onto = event(rename, &["/r/.git/HEAD.lock", "/r/.git/HEAD"]);
    assert!(event_matters(&d, &onto));
    assert!(!event_matters(&d, &event(data, &["/r/.git/index"])));
    assert!(!event_matters(&d, &event(create, &["/r/.git/HEAD.lock"])));
    let access = EventKind::Access(AccessKind::Any);
    assert!(!event_matters(&d, &event(access, &["/r/.git/HEAD"])));
    assert!(!event_matters(
        &d,
        &event(EventKind::Other, &["/r/.git/HEAD"])
    ));
    assert!(!event_matters(&d, &event(data, &[])));
}

#[test]
fn a_rescan_flag_always_matters() {
    let flagged = Event::new(EventKind::Other).set_flag(Flag::Rescan);
    assert!(event_matters(&plain(), &flagged));
}

#[test]
fn scan_folder_lists_children_that_hold_a_repository() {
    let dir = scratch_path("scan");
    std::fs::create_dir_all(dir.join("app/.git")).unwrap();
    std::fs::create_dir_all(dir.join("plain")).unwrap();
    std::fs::create_dir_all(dir.join("outer/inner/.git")).unwrap();
    std::fs::create_dir_all(dir.join("linked")).unwrap();
    std::fs::write(dir.join("linked/.git"), "gitdir: /elsewhere").unwrap();
    std::fs::write(dir.join("notes.txt"), "x").unwrap();
    let found: Vec<String> = scan_folder(&dir).into_iter().collect();
    assert_eq!(found, vec!["app".to_string(), "linked".to_string()]);
    assert!(scan_folder(&dir.join("missing")).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

fn expect_signal(rx: &Receiver<()>, what: &str) {
    rx.recv_timeout(Duration::from_secs(10))
        .unwrap_or_else(|_| panic!("no signal for {what} within 10 s"));
}

/// Needs a real file system watcher. Run with:
/// `cargo test -p trigger-git os::tests::the_watcher -- --ignored --test-threads=1`
#[test]
#[ignore]
fn the_watcher_hears_a_commit_a_switch_and_a_tag_but_not_an_add() {
    if !git_available() {
        return;
    }
    let repo = TempRepo::new();
    repo.commit("a.txt", "first");
    let dirs = GitDirs::resolve(repo.path()).unwrap();
    let (tx, rx) = channel::<()>();
    let _watch = RepoWatch::start(&dirs, move || {
        let _ = tx.send(());
    })
    .unwrap();
    std::thread::sleep(Duration::from_millis(500));

    // `status` and `add` rewrite only the index, which is not watched.
    repo.git(&["status"]);
    repo.write("b.txt", "b");
    repo.git(&["add", "b.txt"]);
    assert!(rx.recv_timeout(Duration::from_secs(1)).is_err());

    repo.git(&["commit", "-q", "-m", "second"]);
    expect_signal(&rx, "a commit");
    std::thread::sleep(Duration::from_millis(500));
    while rx.try_recv().is_ok() {}

    repo.git(&["checkout", "-q", "-b", "other"]);
    expect_signal(&rx, "a branch switch");
    std::thread::sleep(Duration::from_millis(500));
    while rx.try_recv().is_ok() {}

    repo.git(&["tag", "v1"]);
    expect_signal(&rx, "a tag");
}
