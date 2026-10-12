# trigger-git

Fires when something happens in a git repository.

Source id `git`. One crate, one source: everything it fires starts with
`git.`.

## Kinds

Generated from `KINDS` in `src/lib.rs`. Refresh with
`cargo run -p trigger-git --example readme` and paste the output between
the markers; `tests/readme.rs` fails while this table is stale.

<!-- kinds -->
| Kind | Title | Platforms | Permission | When | Payload |
| --- | --- | --- | --- | --- | --- |
| git.commit | Commit made | macOS, Windows, Linux | none | repo*, branch | repo, branch, commit, message |
| git.branch-switched | Branch switched | macOS, Windows, Linux | none | repo*, branch | repo, branch, commit, from, to |
| git.pulled | Pulled | macOS, Windows, Linux | none | repo*, branch | repo, branch, commit, message |
| git.merge-conflict | Merge conflict | macOS, Windows, Linux | none | repo*, branch | repo, branch, files |
| git.rebase-stopped | Rebase stopped | macOS, Windows, Linux | none | repo*, branch | repo, branch, commit |
| git.tag-created | Tag created | macOS, Windows, Linux | none | repo*, branch | repo, branch, tag |
| git.stash-created | Stash created | macOS, Windows, Linux | none | repo*, branch | repo, branch |
| git.detached-head | HEAD detached | macOS, Windows, Linux | none | repo*, branch | repo, branch, commit |
| git.behind-remote | Behind remote | macOS, Windows, Linux | none | repo*, branch, interval_s, fetch | repo, branch, behind |
| git.uncommitted-older-than | Uncommitted work is old | macOS, Windows, Linux | none | repo*, branch, interval_s, older_than_min | repo, branch, minutes |
| git.repo-cloned | Repository cloned | macOS, Windows, Linux | none | folder*, interval_s | path |
<!-- /kinds -->

## Rule fields

The `when` object of a trigger, parsed by `src/rule.rs`. Unknown fields are an
error, so a typo shows up in the editor.

| Field | Required | Default | Meaning |
|---|---|---|---|
| `repo` | yes, for every kind except `git.repo-cloned` | | Absolute path of the repository. Use `${repo.root}` so it follows the terminal's repository. `git.repo-cloned` rejects it |
| `folder` | only for `git.repo-cloned` | | Absolute path of the folder whose new children are watched |
| `branch` | no | any branch | Glob for the branch name. `*` does not cross `/`, so `release/*` matches `release/1.0` and `feature/**` matches `feature/a/b`. Case sensitive. Matched against the new branch for `git.branch-switched` and the branch left for `git.detached-head`. A rule with a glob never fires when there is no branch name |
| `interval_s` | no | 300 | Seconds between checks for `git.behind-remote`, `git.uncommitted-older-than` and `git.repo-cloned`. At least 30. Ignored by the other kinds |
| `older_than_min` | no | 60 | `git.uncommitted-older-than` only: minutes the oldest uncommitted change must have waited |
| `fetch` | no | false | `git.behind-remote` only: run `git fetch` before each check. Without it the check uses the remote refs already on disk |

## Payload

What an event carries, available to steps as `${trigger.<field>}`. Paths, names,
ids and numbers only. Never file contents or secrets, never more than 4 KiB.

| Field | Kinds | Meaning |
|---|---|---|
| `repo` | every kind but `git.repo-cloned` | Absolute path of the repository |
| `branch` | every kind but `git.repo-cloned` | Branch name. Empty when HEAD is detached. For `git.branch-switched` it is the new branch; for `git.detached-head` it is the branch left |
| `commit` | commit, branch-switched, pulled, rebase-stopped, detached-head | Short hash of HEAD |
| `message` | commit, pulled | First line of the HEAD commit message |
| `from`, `to` | branch-switched | The branch left and the branch entered |
| `files` | merge-conflict | Number of files with conflicts (a number) |
| `tag` | tag-created | Name of the new tag |
| `behind` | behind-remote | Commits the branch is behind its upstream (a number) |
| `minutes` | uncommitted-older-than | Minutes the oldest uncommitted change has waited (a number) |
| `path` | repo-cloned | Absolute path of the new repository |

## Platforms

| Platform | How it is detected | Limits |
|---|---|---|
| macOS | FSEvents through `notify`, on the repository's git folders only. Then the `git` command line | git comes from the PATH, then `/opt/homebrew/bin/git`, then `/usr/local/bin/git`. The live tests pass here |
| Windows | `ReadDirectoryChangesW` through `notify`. Then the `git` command line, run with no console window | git must be on the PATH (Git for Windows puts it there). The live tests have not been run on Windows yet |
| Linux | inotify through `notify`. Then the `git` command line | Each watched repository uses a few inotify watches (the git folder and every folder under `refs/`). The live tests have not been run on Linux yet |

## Permissions

None. The crate reads the repository folders named in the rules and runs the `git` command line; it asks the OS for no permission. macOS may ask the user once before an app reads a repository inside Documents, Desktop or Downloads. That prompt belongs to the OS, not to this crate.

## Example

```json
{ "id": "on-commit", "kind": "git.commit", "when": { "repo": "${repo.root}", "branch": "main" } }
```

## Develop

| File | What it holds |
|---|---|
| `src/lib.rs` | `KINDS`, `GitSource` and `start_rules`: parses the rules, groups them by repository, takes each baseline snapshot, starts the file watchers and the `trigger-git` thread. Glue only |
| `src/rule.rs` | The typed `When` and `parse`. Pure, unit tested |
| `src/logic.rs` | Pure: `RepoSnapshot`, `Change`, `step` and `observe` (which changes two snapshots show) and the event payloads. Unit tested in `src/logic/tests.rs` |
| `src/git.rs` | The git command line, std only: `run` (timeout, no console window on Windows, a fallback to the Homebrew paths on macOS) and `value`. A copy of the app's pattern, since a trigger crate cannot depend on the app |
| `src/git/repo.rs` | `GitRepo`, the real `Repo`: builds a `RepoSnapshot` from read only git commands, counts commits behind the upstream, finds the age of the oldest uncommitted change |
| `src/logic/repo.rs` | The `Repo` trait: what the glue asks about a repository. `src/logic/fake.rs` is the fake used in tests |
| `src/logic/polled.rs` | Pure transitions for the polled kinds: `BehindWatch`, `UncommittedWatch`, `CloneScan`, and their payloads |
| `src/logic/settle.rs` | Pure: `Settle`, the 300 ms quiet time before a snapshot, with up to 10 retries while `index.lock` or `HEAD.lock` exists |
| `src/worker.rs` | The `trigger-git` thread: `run` sleeps until the next deadline or message, `spawn` starts it. `src/worker/repo.rs` holds `RepoSlot` (one repository: snapshot, diff, rule filter, polled kinds) and `src/worker/folder.rs` holds `FolderSlot` (one `git.repo-cloned` folder). Both are tested with a fake `Repo` and no threads |
| `src/os.rs` | The only file besides `git.rs` that talks to the OS: `GitDirs` (where a repository keeps its files and which of them matter), `RepoWatch` (the `notify` watchers), `scan_folder` and `availability()` |
| `tests/live.rs` | `#[ignore]` tests against the real OS |
| `tests/readme.rs` | Fails when the Kinds table above is stale |

```
cd src-tauri
cargo test -p trigger-git                       # pure and readme tests, any OS, offline
cargo test -p trigger-git -- --ignored          # live tests on this machine
cargo clippy -p trigger-git --all-targets -- -D warnings
```

## Known limits

What does not work, on which platform, and why.

- git must be installed. On macOS, if `git` is not on the PATH, `/opt/homebrew/bin/git` and then `/usr/local/bin/git` are tried.
- `repo` must be the root of the work tree, which is what `${repo.root}` gives.
- Every git call has a 3 second limit (30 seconds for `git fetch`). A call that times out leaves its part of the snapshot empty.
- Remote refs are not read, so a `git fetch` fires nothing. With `fetch: true` the fetch runs with `--no-tags`, so tags on the remote do not become local tags and do not fire `git.tag-created`. A fetch that fails (offline) is ignored and the count uses the remote refs already on disk.
- `git.uncommitted-older-than` uses the modified time of each tracked file that `git status` lists. Untracked files and deleted files are not counted, and a file saved again looks younger.
- The polled kinds check once per `interval_s`, not on file events, so an event can arrive up to one interval late.
- `git.behind-remote` fires when the count goes from 0 to above 0. The first check is only a baseline, and behind by 2 and then by 5 is one event. A repository with no upstream, or a detached HEAD, never fires it.
- `git.uncommitted-older-than` fires once and waits for a clean tree before it can fire again. Work that is already old when the rule starts fires on the first check.
- `git.repo-cloned` sees only direct children of `folder` that contain `.git`. The first scan is a baseline, so repositories already there fire nothing.
