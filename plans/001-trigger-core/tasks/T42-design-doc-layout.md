---
id: T42
epic: 001-trigger-core
title: Document the workspace and triggers module in design section 2
status: todo
depends_on: [T39]
size: S
model: haiku
files:
  - docs/technical-design.md
---

## Goal

Section 2 of `docs/technical-design.md` shows the repository layout as it is after epic 001: the `triggers/` module in the app, the `crates/` folder with the three trigger crates, and the Cargo workspace.

## Read first

- `docs/technical-design.md:44-106` section 2, the file tree and the Cargo block you add to
- `plans/001-trigger-core/epic.md` the "Workspace" block of the Shape and Decisions 1, 2 and 13
- `plans/_context/conventions.md` the "Writing" section (British spelling, no dashes as punctuation)

## Steps

1. In `docs/technical-design.md`, in the file tree of section 2, find these two lines:

```
      state/
        mod.rs              # Config + Session persistence (app config dir)
    resources/
```

and replace them with:

```
      state/
        mod.rs              # Config + Session persistence (app config dir)
      triggers/
        mod.rs              # TriggerService: the hub, refresh(), the forward thread, the four triggers_* commands
        store.rs            # local process definitions, one JSON file each in <app config dir>/processes/
        variables.rs        # ${home}, ${downloads}, ${notes.root}, ${screenshots}, ${repo.root}
        rules.rs            # definitions + armed list + open repositories -> the rules the hub runs
        status.rs           # the triggers_status answer and the trigger://fired payload
    crates/                 # trigger crates, one per integration, all built from plans/_templates/trigger-crate
      trigger-core/         # the contract (Source, Rule, Event, KindInfo) and the test kit
      trigger-hub/          # registry, rule checks, cool down, one stream of Fired events
      trigger-fs/           # the reference source: fs.file-saved, fs.file-created, fs.file-removed, fs.file-renamed, fs.download-finished
    resources/
```

2. Find the line that starts `Windows only dependency for process creation flags is already in std`. Add this text directly after it, with a blank line before it:

````markdown
`src-tauri` is a Cargo workspace. The app crate `terminal-grid` is the root package; every folder under `crates/` is a member. Shared dependency versions live in `[workspace.dependencies]` and the crates use them with `{ workspace = true }`:

```toml
[workspace]
members = ["crates/*"]
default-members = [".", "crates/*"]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
notify = "8.2"
uuid = { version = "1", features = ["v4"] }
globset = "0.4.20"

[features]
default = ["trigger-fs"]
trigger-fs = ["dep:trigger-fs"]

[dependencies]
trigger-core = { path = "crates/trigger-core" }
trigger-hub = { path = "crates/trigger-hub" }
trigger-fs = { path = "crates/trigger-fs", optional = true }
```

A trigger crate depends on `trigger-core` and on outside crates only, never on another trigger crate, on `tauri` or on the app. `node plans/_tools/check-crates.mjs` checks this. The app wires each source in through one line in `sources()` in `src-tauri/src/triggers/mod.rs`, behind its own cargo feature. The rules for crates are in `docs/process-triggers.md` Part 2.
````

3. Run the acceptance commands.

## Acceptance

- [ ] `grep -n "triggers/" docs/technical-design.md` prints the tree line for `triggers/`
- [ ] `grep -c "trigger-core/\|trigger-hub/\|trigger-fs/" docs/technical-design.md` prints at least `3`
- [ ] `grep -n "default-members" docs/technical-design.md` prints one line
- [ ] `grep -n "check-crates.mjs" docs/technical-design.md` prints one line
- [ ] `git diff --stat` shows only `docs/technical-design.md`
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Sections 3, 9, 10 and 14 (other cards; section 10 was done in T36).
- Correcting other parts of section 2 that no longer match the code, such as the dependency versions in the old `[dependencies]` block.
- Editing the "Process model" diagram.

## If stuck

- If the two-line anchor in step 1 is not found, the tree was edited since this card was written. Find the `state/` entry and put the new block after its `mod.rs` line, before `resources/`. If there is no such entry, set status blocked.
- If the line in step 2 is not found, add the text at the end of the code block's following paragraph, and say so in the Log.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
