---
id: R01
epic: 003-trigger-git
question: Which files under `.git/` change for a commit, a checkout, a pull, a merge conflict and a rebase stop, on git 2.40 and newer, including packed refs
status: answered
---

## Answer

Watch the git dir non recursively plus `refs/` recursively, debounce 300 ms after the last event, then read state with git. A commit rewrites `refs/heads/<b>` (or `HEAD` when detached) plus `index` and `logs/HEAD`; a switch rewrites only `HEAD`; a tag adds `refs/tags/<t>`; a stash adds `refs/stash`; a merge conflict adds `MERGE_HEAD`; a stopped rebase adds `rebase-merge/` (or `rebase-apply/`) and detaches `HEAD`; `git gc` moves loose refs into `packed-refs`.
Decision 3's list is nearly right but needs the git dir watched as a directory (not single files) and `reftable/` for git 2.45+ repos created with `--ref-format=reftable`.
Decide kinds from a two snapshot diff (rules below), never from which file fired.

## Evidence

Ran on git 2.50.1 (Apple Git-155), macOS, 2026-10-12, in a scratch dir. Method: md5 of every file under `.git` (objects excluded) before and after each command, plus a 1 ms python poller listing `*.lock` files during a loop of commits, tags, branches, stashes, pulls and a gc. Notation: `+` created, `~` changed, `-` removed. `logs/*` omitted below except `logs/HEAD`.

| Operation | Files that changed |
|---|---|
| commit | `refs/heads/<b>`, `logs/HEAD`, `logs/refs/heads/<b>`, `index`, `COMMIT_EDITMSG` |
| commit --amend | same as commit |
| `git add` only | `index` only |
| `checkout -b new` | `HEAD`, `refs/heads/new` (created), `logs/HEAD` |
| switch to existing branch | `HEAD`, `logs/HEAD`, (`index` if files differ) |
| detached HEAD (`checkout --detach X`) | `HEAD` (now a sha), `logs/HEAD`, `index` |
| lightweight tag, annotated tag | `refs/tags/<t>` created. Nothing else (annotated adds an object only) |
| stash | `refs/stash`, `logs/refs/stash`, `ORIG_HEAD`, `index`, `logs/HEAD` |
| stash pop / drop | `refs/stash` and `logs/refs/stash` removed (last entry), `index`, `AUTO_MERGE` |
| fetch | `FETCH_HEAD`, `refs/remotes/origin/<b>`, its log. HEAD and local branch untouched |
| pull fast forward | `refs/heads/<b>`, `refs/remotes/origin/<b>`, `FETCH_HEAD`, `ORIG_HEAD`, `index`, `logs/HEAD` |
| pull merge, clean | same as ff, a new merge commit on `refs/heads/<b>`. `MERGE_HEAD` is NOT left behind (it exists only mid operation) |
| pull merge, conflict | `MERGE_HEAD`, `MERGE_MODE`, `MERGE_MSG`, `AUTO_MERGE` created; `ORIG_HEAD`, `FETCH_HEAD`, `index`, `refs/remotes/origin/<b>` changed. `HEAD` and `refs/heads/<b>` do NOT change |
| `merge --abort` | `MERGE_HEAD`, `MERGE_MODE`, `MERGE_MSG`, `AUTO_MERGE` removed, `index`, `logs/HEAD` |
| pull --rebase, conflict (stop) | `HEAD` becomes a bare sha (detached), `REBASE_HEAD`, `MERGE_MSG`, `AUTO_MERGE` created, `rebase-merge/` created with ~17 files (`head-name`, `onto`, `done`, `git-rebase-todo`, `stopped-sha`, `msgnum`, `end`, ...). No `MERGE_HEAD` |
| rebase --continue (finishes) | `rebase-merge/*` and `REBASE_HEAD` removed, `HEAD` rewritten back to `ref: refs/heads/<b>`, `refs/heads/<b>` changed, `index` |
| rebase --abort | `rebase-merge/*`, `REBASE_HEAD`, `AUTO_MERGE`, `MERGE_MSG` removed, `HEAD` back to the branch ref |
| interactive rebase stopped at `edit` | `rebase-merge/` (incl. `amend`, `stopped-sha`), `REBASE_HEAD`, `HEAD` detached |
| `rebase --apply` stop | `rebase-apply/` (about 25 files, `next`, `last`, `head-name`, `rebasing`) and `REBASE_HEAD`; `HEAD` detached |
| cherry-pick conflict / abort | `CHERRY_PICK_HEAD`, `MERGE_MSG`, `AUTO_MERGE` created / removed |
| `reset --hard HEAD~1` | `refs/heads/<b>`, `index`, `logs/HEAD` |
| `git gc` | `packed-refs` rewritten; loose `refs/heads/*`, `refs/tags/*`, `refs/remotes/*` files removed; `info/refs` created. HEAD, the branch values and tag values do not change |
| tag after gc | `refs/tags/<t>` (loose again) |
| delete a packed tag or branch | only `packed-refs` changes (no loose file to delete) |
| delete a loose branch | the loose ref file removed |

Linked worktree (`git worktree add ../wt -b wtb`):
- `wt/.git` is a file: `gitdir: <main>/.git/worktrees/wt`.
- Per worktree files live in `<main>/.git/worktrees/wt/`: `HEAD`, `index`, `ORIG_HEAD`, `logs/HEAD`, `COMMIT_EDITMSG`, plus `commondir` (`../..`) and `gitdir` (back pointer). Its MERGE_HEAD and `rebase-merge/` also go there.
- Shared files stay in the main `.git`: `refs/heads/*`, `refs/tags/*`, `refs/remotes/*`, `packed-refs`, `refs/stash`, config.
- A commit in the worktree changed `<main>/.git/refs/heads/wtb` and `worktrees/wt/{index,logs/HEAD}`. A detach changed only `worktrees/wt/HEAD`. A tag changed `<main>/.git/refs/tags/`.
- `git rev-parse --absolute-git-dir` in the worktree prints the per worktree dir; `--git-common-dir` prints the main `.git`; `--show-toplevel` the work tree. `git rev-parse --git-path MERGE_HEAD` (or `rebase-merge`, `HEAD`) resolves to the correct location in either layout, so use it rather than joining paths.

Lock files seen by the poller (all appear for a few ms, then are renamed over the target): `index.lock`, `HEAD.lock`, `ORIG_HEAD.lock`, `AUTO_MERGE.lock`, `refs/heads/<b>.lock`, `refs/tags/<t>.lock`, `refs/stash.lock`, `logs/refs/heads/<b>.lock`, `packed-refs.lock`, `index.stash.<pid>.lock`. Git writes a ref by creating `<ref>.lock` and renaming it onto `<ref>`, so a watcher sees create `.lock`, then rename (a Modify(Name) or Create of the real name). The real file is whole once the rename event arrives.

Reftable (git 2.45+, `git init --ref-format=reftable`, tested): there is no `refs/` content or `packed-refs`; `HEAD` holds `ref: refs/heads/.invalid`, and every ref change rewrites `reftable/tables.list` and adds `reftable/*.ref`. Default format is still files in 2.50.

## Implications for the cards

Watch list (all relative to `git rev-parse --absolute-git-dir`, and also the common dir when different):
1. git dir, NonRecursive: catches `HEAD`, `MERGE_HEAD`, `packed-refs`, `ORIG_HEAD`, `CHERRY_PICK_HEAD`, `REBASE_HEAD`, and creation or removal of `rebase-merge`, `rebase-apply`. Watching the directory is required: notify on a single file breaks when git replaces it via rename, and `MERGE_HEAD` and the rebase dirs may not exist yet. Filter events by file name in the handler.
2. `<common>/refs`, Recursive (covers heads, tags, remotes, `refs/stash`).
3. `rebase-merge` and `rebase-apply` need no separate watch: creation shows in (1), and a stop changes `HEAD` as well.
4. Optional: `<common>/reftable` NonRecursive, only if `<common>/reftable` exists.
5. Do not watch `index`, `objects/` or `logs/` (index fires on every `git status`; this is why `git status` never needs to be suppressed). Ignore names ending `.lock`, `index`, `COMMIT_EDITMSG`, `FETCH_HEAD`, `logs/`, `objects/`.
6. For a linked worktree, `<repo>/.git` is a file: resolve with `git rev-parse --absolute-git-dir` (per worktree dir, for `HEAD`, `MERGE_HEAD`, rebase dirs) and `--git-common-dir` (for `refs/`, `packed-refs`). Watch both.

Settle: on any relevant event start or restart a 300 ms timer; when it fires with no newer event, take a snapshot. If `<git-dir>/index.lock` or `<git-dir>/HEAD.lock` exists at that moment (check with a plain `exists`), re-arm 300 ms and retry, up to about 10 times (3 s), then snapshot anyway. Never react to the `.lock` events themselves.

Cheap reads for the snapshot (3 git calls at most, run with `-C <repo>`):
- `git rev-parse HEAD` : head sha (fails with exit 128 on an unborn branch, treat as empty).
- `git symbolic-ref -q --short HEAD` : branch name, exit 1 when detached (cheaper than parsing status). Alternatively `git status --porcelain=v2 --branch` gives `# branch.oid`, `# branch.head` (`(detached)` when detached), `# branch.upstream`, `# branch.ab +A -B` and `u ` lines for unmerged paths (count = `files` for a conflict). Use status only for the conflict count and the behind check, since it refreshes the index.
- merge_head: `std::path::Path::exists` on `git rev-parse --git-path MERGE_HEAD`, then read it (sha). Resolve the path once per repo and cache.
- rebase_dir: exists of `--git-path rebase-merge` or `--git-path rebase-apply`.
- refs: `git for-each-ref --format=%(refname) %(objectname)` limited with `--count=` or a cap (say 2000), restricted to `refs/tags refs/stash refs/heads`. This handles loose and packed refs the same way, so `git gc` needs no special case. Do not parse `packed-refs` yourself.
- Message and short sha: `git log -1 --format=%h%x00%s HEAD`.

`RepoSnapshot` fields: `head: Option<String>` (full sha), `branch: Option<String>` (None = detached), `merge_head: Option<String>`, `rebase_dir: Option<RebaseKind>` (Merge, Apply) or bool, `refs: BTreeMap<String, String>` (refname to sha, only `refs/heads/*`, `refs/tags/*`, `refs/stash`; do not include `refs/remotes/*` unless behind-remote needs it, and keep it out of the commit diff because a fetch changes it). Compare `head`, `branch`, `merge_head`, `rebase_dir` first and only diff `refs` if those leave an ambiguity or a tag or stash is possible.

Diff rules, `step(prev, next) -> Vec<Event>`. Suppress everything when `next.rebase_dir` is set, except the rebase-stopped event; during a rebase `HEAD` is detached and moves each step.
- `git.merge-conflict`: `prev.merge_head` is None and `next.merge_head` is Some, and unmerged count (status `u` lines) is above 0 (payload `files` from it). If the count is 0 (merge with `--no-commit`), fire nothing.
- `git.rebase-stopped`: `prev.rebase_dir` None and `next.rebase_dir` Some. A rebase that does not stop is detected at the next settle as a `git.commit` or nothing; a short rebase may be coalesced by the debounce so no stop is seen (accepted). Note: a rebase that stops is also visible with `REBASE_HEAD` present; use that to tell "stopped" from "in progress but between picks" when fine control is wanted.
- `git.branch-switched`: both snapshots have a branch, `prev.branch != next.branch`, `rebase_dir` None. Payload `from`, `to`. A new branch via `checkout -b` is a switch too.
- `git.detached-head`: `prev.branch` Some, `next.branch` None, `next.rebase_dir` None, `next.merge_head` None.
- `git.commit`: same branch (or both detached, rebase_dir None), `prev.head != next.head`, `next.head` is not an ancestor-forward from a pull (see next), so decide by reflog: `git reflog -1 --format=%gs` begins `commit` (also `commit (amend)`, `commit (merge)`), or by the pull test below. Simplest rule: same branch, head changed, and `merge-base --is-ancestor prev.head next.head` true and `rev-list --count prev..next` is 1 is a commit.
- `git.pulled`: same branch, head changed, not a commit by the rule above, and either the reflog subject starts `pull` or `merge` (git writes `pull: Fast-forward`, `pull: Merge made by the 'ort' strategy`), or `FETCH_HEAD` mtime is newer than the previous snapshot. Prefer the reflog subject; read it with `git reflog -1 --format=%gs HEAD`. For `pull --rebase` the subject starts `pull (finish)` or `rebase (finish)`.
- `git.tag-created`: a key in `next.refs` under `refs/tags/` absent from `prev.refs`. Payload the tag name (`branch` field not needed).
- `git.stash-created`: `next.refs["refs/stash"]` exists and (`prev` has none or the sha differs). A sha change on drop plus push of one is rare; pop and drop remove the key or move it back, which does not fire.
- Order when several apply in one window: rebase-stopped, merge-conflict, branch-switched, detached-head, pulled, commit, tag, stash. Emit each at most once per diff. `git.behind-remote`, `git.uncommitted-older-than`, `git.repo-cloned` do not use the snapshot (poll: `git rev-list --count HEAD..@{u}` after the optional fetch, `git status --porcelain=v2` plus the oldest mtime, and a directory scan of `folder` for a new child containing `.git`).
- First snapshot after start is a baseline: never emit from `step(None, first)`.
- A `git gc` or `git pack-refs` yields no change in the `refs` map, so it correctly fires nothing.
- `git fetch` changes only remote refs and `FETCH_HEAD`: it must fire nothing, which holds because `refs/remotes/*` is excluded from the snapshot.

## Doubts

- Tested only git 2.50.1 (Apple Git) on macOS. Behaviour of `ORIG_HEAD`, `AUTO_MERGE` (added in 2.39) and the file set inside `rebase-merge/` is stable across 2.40 to 2.50 to my knowledge, but not run on 2.40 itself. Windows and Linux inotify rename event shapes not verified; the design (watch dirs, debounce, then read) does not depend on them.
- notify itself was not run; the watch behaviour is inferred from the file changes and lock polling. The first card for the watcher should confirm that a rename onto `HEAD` is reported on a non recursive directory watch on all three platforms.
- The commit versus pulled split depends on the reflog subject text (`pull: ...`), which is not locale translated but is not a documented stable interface. The count based fallback (`rev-list --count`) is deterministic and should be kept as the tiebreak.
- The ancestor tests in the commit rule were not run as code; they are straightforward plumbing (`merge-base --is-ancestor` exits 0 or 1).
- Sparse worktrees, submodules (`.git/modules/<name>`) and bare repos were not tested.
