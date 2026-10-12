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
| macOS | | |
| Windows | | |
| Linux | | |

## Permissions

What the OS asks the user for, and when. "None" is a fine answer.

## Example

```json
{ "id": "on-commit", "kind": "git.commit", "when": { "repo": "${repo.root}", "branch": "main" } }
```

## Develop

| File | What it holds |
|---|---|
| `src/lib.rs` | `KINDS`, `GitSource` and the thread that joins the OS layer to the logic. Glue only |
| `src/rule.rs` | The typed `When` and `parse`. Pure, unit tested |
| `src/logic.rs` | Pure: which rules a `Signal` fires, with what payload, and any debounce state. Unit tested |
| `src/os.rs` | The only file that talks to the OS: `availability()` and `listen()`, per platform behind `cfg` |
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
