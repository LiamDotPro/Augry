---
id: T09
epic: 003-trigger-git
title: Write the README Kinds, Rule fields, Payload and Example
status: done
depends_on: [T08]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-git/README.md
---

## Goal

The `trigger-git` README describes the kinds, the `when` fields, the payload and an example, matching the code from T02 to T08. This closes slice A. The Platforms, Permissions, Develop and Known limits sections stay for slices B and C.

## Read first

- `src-tauri/crates/trigger-git/README.md` the file you edit
- `src-tauri/crates/trigger-fs/README.md` how a finished README reads
- `src-tauri/crates/trigger-git/src/rule.rs` the fields and defaults the table must match
- `src-tauri/crates/trigger-git/src/lib.rs:60-110` the payload fields

## Steps

1. Run `cd src-tauri && cargo run -q -p trigger-git --example readme`. Check that the text between `<!-- kinds -->` and `<!-- /kinds -->` in `src-tauri/crates/trigger-git/README.md` is identical to that output (it should be, since T02). If it differs, replace it with the output.

2. In `src-tauri/crates/trigger-git/README.md`, under `## Rule fields`, replace the template's table (the header row, the separator and the one `example` row) with exactly this table. Keep the paragraph above it.

| Field | Required | Default | Meaning |
|---|---|---|---|
| `repo` | yes, for every kind except `git.repo-cloned` | | Absolute path of the repository. Use `${repo.root}` so it follows the terminal's repository. `git.repo-cloned` rejects it |
| `folder` | only for `git.repo-cloned` | | Absolute path of the folder whose new children are watched |
| `branch` | no | any branch | Glob for the branch name. `*` does not cross `/`, so `release/*` matches `release/1.0` and `feature/**` matches `feature/a/b`. Case sensitive. Matched against the new branch for `git.branch-switched` and the branch left for `git.detached-head`. A rule with a glob never fires when there is no branch name |
| `interval_s` | no | 300 | Seconds between checks for `git.behind-remote`, `git.uncommitted-older-than` and `git.repo-cloned`. At least 30. Ignored by the other kinds |
| `older_than_min` | no | 60 | `git.uncommitted-older-than` only: minutes the oldest uncommitted change must have waited |
| `fetch` | no | false | `git.behind-remote` only: run `git fetch` before each check. Without it the check uses the remote refs already on disk |

3. Under `## Payload`, replace the template's table with exactly this table. Keep the paragraph above it.

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

4. Under `## Example`, replace the template's fenced block with exactly this one.

```json
{ "id": "on-commit", "kind": "git.commit", "when": { "repo": "${repo.root}", "branch": "main" } }
```

5. Leave `## Platforms`, `## Permissions`, `## Develop` and `## Known limits` as they are. Slices B and C write them from the real `git.rs`, `os.rs` and glue.

Note on `check-crates.mjs`: `check-crates.mjs` prints warnings, not errors, about template text. The ones it may print for this crate until slice C are: `README.md still has template text`, `src/lib.rs still has template code`, `src/os.rs still has template code`. Warnings are fine here; only errors fail the check. Do not try to remove them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes: 39 tests in the library, 1 in `readme`, `live` shows 1 ignored
- [ ] `grep -c "git.example" src-tauri/crates/trigger-git/README.md` prints `0`
- [ ] `grep -c "^| .fetch. | no | false" src-tauri/crates/trigger-git/README.md` prints `1`
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (warnings about template text and `src/logic/tests.rs` length are expected)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Writing the Platforms, Permissions, Develop or Known limits sections (slices B and C).
- Changing any code. If the tables disagree with the code, set the card blocked and say where.
- Editing the template README.

## If stuck

- If the Kinds table differs from the program output in a way that is not just T02 not having landed, replace it with the output and note it in the Log.
- If `## Rule fields`, `## Payload` or `## Example` is missing from the README, set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (Kinds block already matched readme example output, no change needed; check-crates 0 errors, 5 accepted warnings)
- 2026-10-12 checker: pass
