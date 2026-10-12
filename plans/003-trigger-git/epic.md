---
id: 003-trigger-git
title: trigger-git
status: draft
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

## Shape

Written during planning: `KINDS`, the typed `When`, the pure `step` over a
`RepoSnapshot { head, branch, merge_head, rebase_dir, refs }` diff.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A | kinds, `When`, `RepoSnapshot`, pure diff `step` with tests | |
| B | the watcher and git CLI layer behind a trait, with a fake | A |
| C | live tests with a temp repo, README, feature flag, app registration | B |
| D | repo map | C |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | Which files under `.git/` change for a commit, a checkout, a pull, a merge conflict and a rebase stop, on git 2.40 and newer, including packed refs | The watch list and the diff rules | |
| R02 | How the app's `git/cli.rs` runs git (flags, `CREATE_NO_WINDOW`, timeouts), so the crate can copy the pattern without depending on the app | Decision 3 | |

## Risks

- Repos with many refs make the snapshot slow; cap the ref list and compare
  HEAD and the current branch first.
