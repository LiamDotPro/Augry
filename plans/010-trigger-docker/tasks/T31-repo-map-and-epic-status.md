---
id: T31
epic: 010-trigger-docker
title: Add trigger-docker to the repo map and mark 010 done
status: todo
depends_on: [T30]
size: S
model: haiku
files:
  - plans/_context/repo-map.md
  - plans/_context/architecture.md
---

## Goal

The repo map describes the `trigger-docker` crate and the feature that wires it in, and the Epics table in `architecture.md` shows 010 as done.

## Read first

- `plans/_context/repo-map.md:28` the `src-tauri/Cargo.toml` row
- `plans/_context/repo-map.md:39` the `trigger-fs` row
- `plans/_context/repo-map.md:44` the Epics-style CI row and `:61-63` the Commands table
- `plans/_context/architecture.md:71-79` the Epics table
- `src-tauri/crates/trigger-docker/README.md` the Kinds table and Develop table

## Steps

1. In `plans/_context/repo-map.md`, in the `src-tauri/Cargo.toml` row, replace the text `Feature `trigger-fs` (on by default) wires in the file source.` with `Features `trigger-fs` and `trigger-docker` (both on by default) wire in the file and container sources.`

2. In the same file, directly after the row that starts `| `src-tauri/crates/trigger-fs/` |`, add this row:

```
| `src-tauri/crates/trigger-docker/` | Fires on container events from Docker and Podman by running `<engine> events` as a child and parsing its lines. Kinds `docker.container-started`, `docker.container-stopped`, `docker.container-healthy`, `docker.container-unhealthy`, `docker.image-built`. `logic.rs` (pure parser and matcher), `supervisor.rs` (restart with backoff, kill on stop), `os.rs` (finding the binary, the `version` check) |
```

3. In the same file, add this row to the Commands table directly after the row `| Live file watching tests |`:

```
| Live container tests | `cd src-tauri && cargo test -p trigger-docker -- --ignored --test-threads=1` (needs a running Docker; each test prints `skipping:` and passes without one) |
```

4. In `plans/_context/architecture.md`, in the Epics table, the 010 epic has no row of its own today (it sits inside `003 to 015`). Replace the row `| 003 to 015 | one crate per integration, all depending on 001 only | draft |` with these two rows, in this order:

```
| 010-trigger-docker | `trigger-docker`: container started, stopped, healthy, unhealthy and image built, from Docker and Podman | done |
| 003 to 009, 011 to 015 | one crate per integration, all depending on 001 only | draft |
```

If the file already has a row starting `| 010`, set its Status cell to `done` instead and make no other change to the table.

## Acceptance

- [ ] `grep -c "trigger-docker" plans/_context/repo-map.md` prints `4` or more
- [ ] `grep -n "010-trigger-docker" plans/_context/architecture.md` prints one line ending in `| done |`
- [ ] `grep -c "003 to 009, 011 to 015" plans/_context/architecture.md` prints `1`
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `node plans/_tools/lint-cards.mjs 010-trigger-docker` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/010-trigger-docker/tasks/`

## Out of scope

- Editing the "Process triggers (epic 001)" section of the repo map, the CI row, or any code.
- Changing other epics' rows in the Epics table.
- Editing `epic.md`; its status is the architect's.

## If stuck

- If a row text from steps 1 or 4 is not found exactly, search with `grep -n "trigger-fs" plans/_context/repo-map.md` or `grep -n "003 to" plans/_context/architecture.md`, make the equivalent edit on the line you find, and note it in the Log.
- If `check-crates.mjs` fails, it is not about these files; set status blocked and paste its first error line in the Log.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
