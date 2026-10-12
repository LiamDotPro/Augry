---
id: T29
epic: 001-trigger-core
title: Write the trigger-fs README and run the full check
status: todo
depends_on: [T28]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-fs/README.md
---

## Goal

`trigger-fs` is complete: its README has all eight sections for the crate as it now behaves (kinds, rule fields, payload, platforms, permissions, example, develop, known limits), and the crate passes every check including the live tests on this machine.

## Read first

- `src-tauri/crates/trigger-fs/README.md` the file you replace
- `src-tauri/crates/trigger-fs/src/lib.rs` `KINDS`, to check the table and field docs
- `plans/_tools/check-crates.mjs` which sections and markers it requires
- `plans/001-trigger-core/epic.md` Decisions 6 to 9 and the Risks section (macOS folder prompt)

## Steps

1. Replace the whole of `src-tauri/crates/trigger-fs/README.md` with this text. It has the eight sections `check-crates.mjs` requires, in this order, and no template text.

`````markdown
# trigger-fs

Fires when files change under a watched folder.

Source id `fs`. One crate, one source: everything it fires starts with `fs.`.

## Kinds

<!-- kinds -->
| Kind | Title | Platforms | Permission | When | Payload |
| --- | --- | --- | --- | --- | --- |
| fs.file-saved | File saved | macOS, Windows, Linux | none | path*, glob, ignore, settle_ms | path, name, relative, ext |
| fs.file-created | File created | macOS, Windows, Linux | none | path*, glob, ignore, settle_ms | path, name, relative, ext |
| fs.file-removed | File removed | macOS, Windows, Linux | none | path*, glob, ignore, settle_ms | path, name, relative, ext |
| fs.file-renamed | File renamed | macOS, Windows, Linux | none | path*, glob, ignore, settle_ms | path, name, relative, ext, from |
| fs.download-finished | Download finished | macOS, Windows, Linux | none | path*, glob, ignore, settle_ms | path, name, relative, ext |
<!-- /kinds -->

Generated from `KINDS` in `src/lib.rs`. Refresh with
`cargo run -p trigger-fs --example readme` and paste the output between the
markers; `tests/readme.rs` fails while this table is stale.

What each kind means:

- `fs.file-created`: a file appeared that was not there when the rule started.
- `fs.file-saved`: a file that was already there was written to. An editor's
  "write a temp file, then rename it over the target" counts as one save.
- `fs.file-removed`: a file that was there is gone.
- `fs.file-renamed`: one file was renamed or moved inside the watched folder.
  When the two halves cannot be matched, you get a removed and a created
  instead.
- `fs.download-finished`: a browser finished a download. The file also fires
  `fs.file-created` (or `fs.file-saved`).

Only changes after the rule starts fire. A file that already exists is not
"created". Directories never fire.

## Rule fields

The `when` object of a trigger, parsed by `src/rule.rs`. Unknown fields are an
error, so a typo shows up in the editor.

| Field | Required | Default | Meaning |
|---|---|---|---|
| `path` | yes | | Folder to watch. Absolute, after variables such as `${downloads}` are expanded. Everything below it is watched. |
| `glob` | no | `**/*` | Which files count. Matched against the path below `path`, with `/` separators, ignoring case. `*` does not cross a `/`, so use `**/*.md` for markdown files at any depth. |
| `ignore` | no | `[]` | More globs to skip, added to the built in ones. |
| `settle_ms` | no | `500` | How long a file must be quiet before it counts as changed. |

Always ignored: `.git`, `node_modules` and `target` folders, `.DS_Store`, and
files ending in `.crdownload`, `.part`, `.download`, `.tmp`, `~`, `.swp` or
`.swx`.

## Payload

What an event carries, available to steps as `${trigger.<field>}`. Paths, names,
ids and numbers only. Never file contents or secrets, never more than 4 KiB.

| Field | Meaning |
|---|---|
| `path` | Absolute path of the file, in canonical form (on macOS `/private/var/...` for `/var/...`) |
| `name` | File name |
| `relative` | Path below the watched folder, with `/` separators |
| `ext` | Extension without the dot, empty when there is none |
| `from` | Only `fs.file-renamed`: the absolute path before the rename |

## Platforms

The `notify` library does the watching, so one file (`src/os.rs`) covers all
three platforms.

| Platform | How it is detected | Limits |
|---|---|---|
| macOS | FSEvents. A plain save arrives looking like a create, and the two halves of a rename arrive unpaired, so the crate decides from the file system, not from the event kind | Rename pairing is best effort |
| Windows | ReadDirectoryChangesW | Rename pairing is best effort |
| Linux | inotify. A rename arrives with both halves paired | A very large tree can hit the inotify watch limit |

## Permissions

None on Windows and Linux. On macOS the OS itself asks the first time a rule
watches Desktop, Documents or Downloads (or a removable or network volume).
Until the user allows it, no events arrive and the folder looks empty. The
crate does not ask and cannot tell.

## Example

```json
{
  "id": "docs-saved",
  "kind": "fs.file-saved",
  "when": { "path": "${repo.root}/docs", "glob": "**/*.md", "ignore": ["drafts/**"] }
}
```

## Develop

| File | What it holds |
|---|---|
| `src/lib.rs` | `KINDS`, `FsSource` and the thread that joins the OS layer to the logic. Glue only |
| `src/rule.rs` | The typed `When`, the default ignores and `parse`, which also compiles the globs. Pure, unit tested |
| `src/logic.rs` | Pure: the settle state machine. Raw events mark a path pending; once it is quiet for `settle_ms` a stat and the set of known files decide created, saved, removed, renamed or download finished. Time and "is this a file" are passed in |
| `src/logic/tests.rs` | The state machine's tests, replaying the event sequences each OS produces |
| `src/os.rs` | The only file that talks to the OS: the clock, the start scan, `notify` and its event kinds |
| `tests/live.rs` | `#[ignore]` tests against the real file system |
| `tests/readme.rs` | Fails when the Kinds table above is stale |

```
cd src-tauri
cargo test -p trigger-fs                                  # pure and readme tests, any OS, offline
cargo test -p trigger-fs -- --ignored --test-threads=1    # live tests on this machine
cargo clippy -p trigger-fs --all-targets -- -D warnings
```

A bug in what fires goes into `src/logic/tests.rs` first, as a replay of the
events that caused it.

## Known limits

- The first scan of a very large tree is slow, and so is watching it. The
  crate does not cap it. Watch a narrower folder.
- Rename pairing is best effort on macOS and Windows: it needs exactly one file
  to vanish and exactly one to appear in the same moment. Anything else is a
  removed plus a created.
- Safari downloads into a `.download` folder, not a file. The folder is
  ignored, but the files inside it are not, so a Safari download may fire
  `fs.file-created` for them, and `fs.download-finished` does not fire for it.
- Symlinks and directories never fire.
- A change that only touches metadata (permissions, tags) counts as a save.
- A rule fires some 100 ms after `settle_ms` has passed, not at the moment of
  the change.
- The built in ignores are fixed. A file called `notes.tmp` is never seen.
`````

2. The Kinds table between the markers must match the code exactly. Run `cd src-tauri && cargo run -q -p trigger-fs --example readme` and compare. If the output differs from the table in step 1 (it should not), replace the lines between `<!-- kinds -->` and `<!-- /kinds -->` with the program output, and say so in the Log.

3. Run the acceptance commands. The last one runs the live tests on this machine: macOS may show a permission prompt for the folder, which is the same prompt the README's Permissions section describes.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-fs` passes (the README table test included)
- [ ] `cd src-tauri && cargo test -p trigger-fs -- --ignored --test-threads=1` passes (5 live tests and 1 in the library)
- [ ] `cd src-tauri && cargo test -p trigger-core -p trigger-fs` passes
- [ ] `for s in Kinds "Rule fields" Payload Platforms Permissions Example Develop "Known limits"; do grep -c "^## $s$" src-tauri/crates/trigger-fs/README.md; done` prints eight lines, each `1`
- [ ] `grep -rn "Replace with\|not implemented yet\|{{" src-tauri/crates/trigger-fs/README.md src-tauri/crates/trigger-fs/src src-tauri/crates/trigger-fs/tests` prints nothing
- [ ] `node plans/_tools/check-crates.mjs` exits 0 and its summary line says `0 error(s)` (a warning that `logic/tests.rs` is over 400 lines is acceptable)
- [ ] `cd src-tauri && cargo clippy -p trigger-fs --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-fs --check` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Any code change. If the code and the README disagree, the README is wrong unless a test says otherwise; stop and use If stuck.
- Splitting `logic/tests.rs` to silence the size warning.
- Updating `plans/_context/repo-map.md` or `docs/` (slice E).
- Adding README sections or reordering them.

## If stuck

- If the table test fails, step 2 tells you how to fix the table. Do not edit `KINDS`.
- If `check-crates.mjs` reports an error for `trigger-fs`, quote the line in the Log and set status blocked.
- If a live test fails here but passed in T28, run it twice more; if it still fails, set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
