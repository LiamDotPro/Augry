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
| `example` | yes | | Replace with the real fields |

## Payload

What an event carries, available to steps as `${trigger.<field>}`. Paths, names,
ids and numbers only. Never file contents or secrets, never more than 4 KiB.

| Field | Meaning |
|---|---|
| `example` | Replace with the real fields |

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
{ "id": "example", "kind": "git.example", "when": { "example": "value" } }
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
