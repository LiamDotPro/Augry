---
id: T31
epic: 003-trigger-git
title: Add trigger-git to the repo map and mark 003 done
status: todo
depends_on: [T30]
size: S
model: haiku
files:
  - plans/_context/repo-map.md
  - plans/_context/architecture.md
  - plans/003-trigger-git/epic.md
---

## Goal

The repo map describes the `trigger-git` crate and the feature that wires it in, the Epics table in `architecture.md` shows 003 as done, and the epic's `status:` line says `done`.

## Read first

- `plans/_context/repo-map.md:28` the `src-tauri/Cargo.toml` row
- `plans/_context/repo-map.md:39` the `trigger-fs` row
- `plans/_context/repo-map.md:61` the Commands table row for live tests
- `plans/_context/architecture.md:71-79` the Epics table
- `src-tauri/crates/trigger-git/README.md` the Kinds table and Develop table

## Steps

1. In `plans/_context/repo-map.md`, in the `src-tauri/Cargo.toml` row, replace the text `Feature `trigger-fs` (on by default) wires in the file source.` with `Features `trigger-fs` and `trigger-git` (both on by default) wire in the file and git sources.`

2. In the same file, directly after the row that starts `| `src-tauri/crates/trigger-fs/` |`, add this row:

```
| `src-tauri/crates/trigger-git/` | Fires on what happens in a repository: commit, branch switch, pull, merge conflict, stopped rebase, tag, stash, detached HEAD, behind remote, old uncommitted work, new clone in a folder. Kinds `git.commit`, `git.branch-switched`, `git.pulled`, `git.merge-conflict`, `git.rebase-stopped`, `git.tag-created`, `git.stash-created`, `git.detached-head`, `git.behind-remote`, `git.uncommitted-older-than`, `git.repo-cloned`. `logic.rs` (pure `RepoSnapshot` and `step`), `rule.rs`, `git.rs` (the git CLI and `GitRepo`), `os.rs` (`notify` watching, settle and lock retry, folder scan), `worker*.rs` (the `trigger-git` thread) |
```

3. In the same file, add this row to the Commands table directly after the row that starts `| Live file watching tests |`:

```
| Live git tests | `cd src-tauri && cargo test -p trigger-git -- --ignored --test-threads=1` (needs `git` on the PATH; each test makes its own temporary repository) |
```

4. In `plans/_context/architecture.md`, in the Epics table, replace the row `| 003 to 015 | one crate per integration, all depending on 001 only | draft |` with these two rows, in this order:

```
| 003-trigger-git | `trigger-git`: commit, branch switch, pull, merge conflict, stopped rebase, tag, stash, detached HEAD, behind remote, old uncommitted work and new clones | done |
| 004 to 015 | one crate per integration, all depending on 001 only | draft |
```

If the file already has a row starting `| 003`, set its Status cell to `done` instead and make no other change to the table. If the `003 to 015` row has already been split by another epic, remove `003, ` or `003 to 003` style mentions of 003 from the range text on that row if it is named there, and add only the 003 row.

5. In `plans/003-trigger-git/epic.md`, change the front matter line `status: carding` (or whatever status it shows now, such as `building`) to `status: done`. Change nothing else in that file.

## Acceptance

- [ ] `grep -c "trigger-git" plans/_context/repo-map.md` prints `3` (the Cargo.toml row, the crate row and the Commands row)
- [ ] `grep -n "003-trigger-git" plans/_context/architecture.md` prints one line ending in `| done |`
- [ ] `grep -n "^status:" plans/003-trigger-git/epic.md` prints `status: done`
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `node plans/_tools/lint-cards.mjs 003-trigger-git` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/003-trigger-git/tasks/`

## Out of scope

- Editing the "Process triggers (epic 001)" section of the repo map, the CI row, the Rust lints row (T32), or any code.
- Changing other epics' rows in the Epics table.
- Editing the epic body, Decisions or research.

## If stuck

- If a row text from steps 1 or 4 is not found exactly, search with `grep -n "trigger-fs" plans/_context/repo-map.md` or `grep -n "003 to" plans/_context/architecture.md`, make the equivalent edit on the line you find, and note it in the Log.
- If `check-crates.mjs` fails, it is not about these files; set status blocked and paste its first error line in the Log.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
