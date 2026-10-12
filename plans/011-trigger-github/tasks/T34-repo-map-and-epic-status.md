---
id: T34
epic: 011-trigger-github
title: Add trigger-github to the repo map and mark 011 done
status: todo
depends_on: [T33]
size: S
model: haiku
files:
  - plans/_context/repo-map.md
  - plans/_context/architecture.md
  - plans/011-trigger-github/epic.md
---

## Goal

The repo map describes the `trigger-github` crate, the feature that wires it in, the `${repo.remote}` variable and the live test command. The Epics table in `architecture.md` shows 011 as done, and the epic's `status:` line says `done`.

## Read first

- `plans/_context/repo-map.md` the `src-tauri/Cargo.toml` row, the `src-tauri/src/triggers/` row, the `trigger-fs` row, the Commands table, and item 2 of "Process triggers (epic 001)"
- `plans/_context/architecture.md:73-78` the Epics table
- `src-tauri/crates/trigger-github/README.md` the Kinds table and the Develop section

## Steps

1. In `plans/_context/repo-map.md`, in the `src-tauri/Cargo.toml` row, replace the text ``Feature `trigger-fs` (on by default) wires in the file source.`` with ``Features `trigger-fs` and `trigger-github` (both on by default) wire in the file and GitHub sources.``

2. In the same file, in the `src-tauri/src/triggers/` row, replace the text ``(`${home}`, `${downloads}`, `${notes.root}`, `${screenshots}`, `${repo.root}`)`` with ``(`${home}`, `${downloads}`, `${notes.root}`, `${screenshots}`, `${repo.root}`, `${repo.remote}`)``

3. In the same file, in item 2 of "Process triggers (epic 001)", replace the text ``with `#<n>` added when `${repo.root}` fans out`` with ``with `#<n>` added when `${repo.root}` or `${repo.remote}` fans out``

4. In the same file, directly after the row that starts ``| `src-tauri/crates/trigger-fs/` |``, add this row:

```
| `src-tauri/crates/trigger-github/` | Fires on what happens on GitHub for the signed in user, by polling with the user's own `gh api` login (no token is handled). Kinds `github.issue-assigned`, `github.review-requested`, `github.ci-failed`, `github.ci-passed`, `github.pr-commented`, `github.pr-merged`, `github.release-published`, `github.dependency-pr-opened`. `rule.rs` (`repo`, `branch`, `interval_s`, `author`), `logic/` (pure: parsing `gh api -i` output, `Backoff`, the diff per endpoint group, `poll.rs` the `Poller`), `os.rs` (finding `gh`, `GhCli`, the login check), `run.rs` (the `trigger-github` thread). `when.repo` may be `${repo.remote}` |
```

5. In the same file, add this row to the Commands table directly after the row that starts ``| Live file watching tests |``:

```
| Live GitHub tests | `cd src-tauri && cargo test -p trigger-github -- --ignored --nocapture` (needs `gh` installed and signed in; each test prints `skipped:` and passes without it; read only requests; CI does not run them) |
```

6. In `plans/_context/architecture.md`, in the Epics table, replace the row `| 003 to 015 | one crate per integration, all depending on 001 only | draft |` with these two rows, in this order:

```
| 011-trigger-github | `trigger-github`: issue assigned, review requested, CI failed or passed, PR comment, PR merged, release published, Dependabot or Renovate PR, polled with `gh` | done |
| 003 to 010, 012 to 015 | one crate per integration, all depending on 001 only | draft |
```

If the file already has a row starting `| 011`, set its Status cell to `done` instead and make no other change to the table. If the `003 to 015` row was already split by another epic, remove only the `011` mention from the range text on that row (for example `011 to 015` becomes `012 to 015`) and add only the 011 row.

7. In `plans/011-trigger-github/epic.md`, change the front matter line `status: carding` (or whatever status it shows, such as `building`) to `status: done`. Change nothing else in that file.

## Acceptance

- [ ] `grep -c "trigger-github" plans/_context/repo-map.md` prints `4` or more
- [ ] `grep -c "repo.remote" plans/_context/repo-map.md` prints `3` or more
- [ ] `grep -n "011-trigger-github" plans/_context/architecture.md` prints one line ending in `| done |`
- [ ] `grep -c "003 to 010, 012 to 015" plans/_context/architecture.md` prints `1`
- [ ] `grep -n "^status:" plans/011-trigger-github/epic.md` prints `status: done`
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `node plans/_tools/lint-cards.mjs 011-trigger-github` shows 0 errors
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/011-trigger-github/tasks/`

## Out of scope

- The CI row and the Rust lints row of the repo map (T35).
- Any code, the epic body, its Decisions or research.
- Changing other epics' rows in the Epics table.

## If stuck

- If a text in steps 1 to 3 or 6 is not found exactly, search with `grep -n "trigger-fs" plans/_context/repo-map.md` or `grep -n "003 to" plans/_context/architecture.md`, make the equivalent edit on the line you find, and note it in the Log.
- If `check-crates.mjs` fails, it is not about these files; set status blocked and paste its first error line in the Log.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
