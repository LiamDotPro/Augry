---
id: 003-trigger-git
title: trigger-git
status: carding
depends_on: [001-trigger-core]
---

# 003 trigger-git

## Goal

The crate `src-tauri/crates/trigger-git` fires for what happens in a
repository: a commit, a branch switch, a pull, a merge conflict, a stopped
rebase, a tag, a stash, a detached HEAD, a branch falling behind its remote,
uncommitted work getting old, and a new clone appearing in a projects folder.
It works on all three platforms with the git CLI and file watching only.

## Non goals

- Git hooks that call the app. That is `launch.cli` with a recipe in
  `trigger-launch`; this crate watches the repository and nothing else.
- Anything that needs GitHub: `trigger-github`.

## Decisions

1. **Crate** `trigger-git`, source id `git`, depends on `trigger-core`,
   `notify` (workspace) and the git CLI already required by the app.
2. **Kinds**: `git.commit`, `git.branch-switched`, `git.pulled`,
   `git.merge-conflict`, `git.rebase-stopped`, `git.tag-created`,
   `git.stash-created`, `git.detached-head`, `git.behind-remote`,
   `git.uncommitted-older-than`, `git.repo-cloned`.
3. **Mechanism**: watch `.git/HEAD`, `.git/refs/`, `.git/packed-refs`,
   `.git/MERGE_HEAD`, `.git/rebase-merge/` and `.git/rebase-apply/` with
   `notify`, then read state with `git` commands (`rev-parse`, `status
   --porcelain=v2 --branch`, `log -1`). `git.behind-remote` and
   `git.uncommitted-older-than` poll on an interval from `when`
   (default 5 minutes), with `git fetch` only when the rule says
   `fetch: true`.
4. **`when`**: `repo` (required, usually `${repo.root}`), `branch` (glob,
   optional), `interval_s`, `older_than_min`, `fetch`, and for
   `git.repo-cloned` a `folder` instead of `repo`.
5. **Payload**: `repo`, `branch`, `commit` (short sha), `message` (first
   line), `from` and `to` for a switch, `files` count for a conflict.

6. **Watching** follows R01, not the file list in Decision 3: watch the git
   dir (`git rev-parse --absolute-git-dir`) non recursively and
   `<common dir>/refs` recursively, plus `<common dir>/reftable` when it
   exists, and both dirs for a linked worktree. Filter by name; ignore
   anything ending `.lock`, `index`, `COMMIT_EDITMSG`, `FETCH_HEAD`, `logs/`,
   `objects/`. Settle 300 ms after the last event; if `index.lock` or
   `HEAD.lock` exists then, re-arm and retry up to 10 times, then snapshot.
7. **Git CLI** is `src/git.rs`, a std only copy of the app's `git/cli.rs`
   pattern from R02 (bare `git`, `GIT_OPTIONAL_LOCKS=0`,
   `GIT_TERMINAL_PROMPT=0`, `GIT_PAGER=cat`, `LC_ALL=C`, null stdin,
   `CREATE_NO_WINDOW`, 3 s timeout, 30 s for `fetch`, non zero exit is data).
   On macOS a `NotFound` spawn retries `/opt/homebrew/bin/git` then
   `/usr/local/bin/git`. It and `os.rs` are the crate's OS layer.
8. **`RepoSnapshot`** (pure type in `logic.rs`): `head: Option<String>`,
   `branch: Option<String>` (None when detached), `merge_head:
   Option<String>`, `rebase: bool`, `unmerged: u32` (read only when
   `merge_head` is set), `reflog_subject: Option<String>` (`git reflog -1
   --format=%gs HEAD`), `short: Option<String>`, `message: Option<String>`,
   `refs: BTreeMap<String, String>` for `refs/heads/*`, `refs/tags/*` and
   `refs/stash` only, capped at 2000 (`for-each-ref --count=2000`). Remote
   refs are never in it, so a fetch fires nothing.
9. **The diff** `step(prev: &RepoSnapshot, next: &RepoSnapshot) -> Vec<Change>`
   is pure and follows R01's rules and order, with these exact choices: no
   event from the first snapshot; while `next.rebase` only `rebase-stopped`
   can fire; a moved head on the same branch is `commit` when the reflog
   subject starts with `commit` or `cherry-pick`, `pulled` when it starts with
   `pull` or `merge`, and nothing otherwise (reset, rebase finish).
10. **Polled kinds** run on the rule's `interval_s` (default 300) in the same
    thread, with pure transition logic and tests: `behind-remote` fires when
    `git rev-list --count HEAD..@{u}` goes from 0 to above 0 (after `git fetch`
    only when `fetch: true`); `uncommitted-older-than` fires once when the
    oldest modified tracked file (status v2, `--untracked-files=no`, file
    mtimes) is older than `older_than_min` (default 60) and re-arms when the
    tree is clean; `repo-cloned` fires for a new child of `folder` that
    contains `.git` (first scan is the baseline).
11. **`when` per kind**: `repo` is required for every kind except
    `repo-cloned`, which requires `folder` and rejects `repo`. `branch` is a
    globset glob (as in `trigger-fs`). `interval_s` at least 30.
12. **Behind a trait**: `trait Repo: Send { fn snapshot(&self) -> Result<RepoSnapshot, String>; fn behind(&self, fetch: bool) -> Option<u32>; fn oldest_change_secs(&self) -> Option<u64>; }`
    with `GitRepo` (real, in `git.rs`) and a fake in tests, so the glue is
    tested without git.
13. **Registration**: cargo feature `trigger-git` (default on) and one line in
    `sources()`, exactly like `trigger-fs`.

## Shape

```
src-tauri/crates/trigger-git/
  src/lib.rs     KINDS (11), GitSource, start, thread "trigger-git"
  src/rule.rs    When { repo, folder, branch, interval_s, older_than_min, fetch }, parse
  src/logic.rs   RepoSnapshot, Change, step(prev, next), polled transitions, payloads
  src/logic/tests.rs
  src/git.rs     run, value, GitRepo: Repo
  src/os.rs      watch dirs for a repo, settle and lock retry, scan a folder
  tests/live.rs  #[ignore] tests on a temp repo (commit, switch, tag, conflict)
```

Payload per Decision 5: `repo`, `branch`, `commit` (short sha), `message`
(first line), `from` and `to` for a switch, `files` for a conflict, `behind`
for behind-remote, `minutes` for uncommitted-older-than, `path` for
repo-cloned, `tag` for a tag.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A (T01 to T09) | scaffold, KINDS, `rule.rs`, `RepoSnapshot` and the pure `step` with R01's cases as tests | |
| B (T10 to T19) | `git.rs` and `GitRepo`, polled transition logic with tests, the `Repo` trait and fake | A |
| C (T20 to T29) | `os.rs` watcher with settle and lock retry, the glue thread, live tests on a temp repo, README | B |
| D (T30 to T39) | app: feature and `sources()` line, repo map row | C |

Ruling (slice C): `start` fails the whole source when any rule's `repo` is
missing or not a git repository, as `trigger-fs` does for a missing folder.
The watch filter is a whitelist (`HEAD`, `MERGE_HEAD`, `packed-refs`, rebase
folders, `refs/` except `refs/remotes`, `reftable/`), which behaves like
Decision 6's ignore list. The glue lives in `src/worker*.rs` to keep `lib.rs`
under 400 lines.

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | Which files under `.git/` change for a commit, a checkout, a pull, a merge conflict and a rebase stop, on git 2.40 and newer, including packed refs | The watch list and the diff rules | Ran on git 2.50: watch the git dir as a directory plus `refs/`, settle 300 ms, snapshot by `rev-parse`, `symbolic-ref`, `for-each-ref`. Folded into Decisions 6, 8, 9. |
| R02 | How the app's `git/cli.rs` runs git (flags, `CREATE_NO_WINDOW`, timeouts), so the crate can copy the pattern without depending on the app | Decision 3 | Bare `git` with `GIT_OPTIONAL_LOCKS=0` and friends, 3 s timeout; copy as `git.rs`. Folded into Decision 7. |

## Risks

- Repos with many refs make the snapshot slow; cap the ref list and compare
  HEAD and the current branch first.
