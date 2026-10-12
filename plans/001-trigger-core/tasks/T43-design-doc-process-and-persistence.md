---
id: T43
epic: 001-trigger-core
title: Document triggers in design sections 3 and 9
status: done
depends_on: [T42]
size: S
model: haiku
files:
  - docs/technical-design.md
---

## Goal

Section 3 (process model) lists the `TriggerService` in the managed state and the trigger threads. Section 9 (persistence) lists the `triggers` block in `config.json` and describes the `processes/` folder of definition files.

## Read first

- `docs/technical-design.md:109-153` section 3, the managed state block and the thread list
- `docs/technical-design.md:501-543` section 9, the `config.json` example and the text after it
- `plans/001-trigger-core/tasks/T30-triggers-config-and-store.md` what a definition file must contain (the `parse` function)
- `plans/001-trigger-core/tasks/T35-trigger-service-and-startup.md` the thread names `trigger-forward` and `trigger-hub`

## Steps

1. Section 3. In the managed state block, find the line:

```
    pub config: Arc<RwLock<Config>>,
```

and add a line directly after it:

```
    pub triggers: TriggerService, // the trigger hub and the rules worked out from process definitions
```

2. Section 3. Find the line `- One optional `notify` watcher thread for the notes root.` and add these three bullets directly after it:

```markdown
- One `trigger-hub` thread: drains events from the trigger sources, applies the two second cool down per rule, and publishes `Fired`.
- One `trigger-forward` thread: turns each `Fired` into the `trigger://fired` event and one log line (ids and the kind only, never the payload).
- One thread per running trigger source (for `trigger-fs`, named `trigger-fs`, plus `notify`'s own). `TriggerService::refresh()` starts and stops them whenever the rules change; it is cheap when nothing changed, so it runs after every pane change and config save.
```

3. Section 9. In the `config.json` example find the line `  "restoreSessionOnLaunch": true` and replace it with these two lines (note the comma):

```
  "restoreSessionOnLaunch": true,
  "triggers": { "enabled": false, "armed": [] }
```

4. Section 9. Find the line that starts `Commands: `config_get`, `config_set { patch }`, `session_get`` and add this text directly after it, with a blank line before it. The `triggers` block is switched with `config_set` (`{ triggers: { enabled } }`) and `armed` is changed by `triggers_arm`:

````markdown
### 9.1 Process definitions and arming

`config.json` has a `triggers` block: `enabled` is the master switch (off by default) and `armed` lists the ids of the process definitions the user has armed. It is local only and never goes in `session.json` or to the cloud.

Local process definitions are JSON files in `processes/` inside the same folder, one per process; the file name without `.json` is the process id. The folder is created on first use. Only the fields triggers need are read, the rest of the file is left alone:

```json
{
  "schemaVersion": 1,
  "name": "Keep the docs tidy",
  "triggers": [
    { "id": "saved", "kind": "fs.file-saved", "when": { "path": "${notes.root}", "glob": "**/*.md" } }
  ]
}
```

A file is usable when it is a JSON object with `schemaVersion` 1 and a non empty `name`, and `triggers`, when present, is an array of `{ id, kind, when }` with unique ids. A file that is not usable is still listed by `triggers_status`, with a `problem` text, and has no rules. Files over 1 MB are not read.

Strings in `when` may use `${home}`, `${downloads}`, `${notes.root}`, `${screenshots}` and `${repo.root}`. `${repo.root}` makes one rule per distinct repository root among the open panes. A rule whose variable has no value on this machine is `skipped` with a reason. Rule ids are `<processId>/<triggerId>`, with `#<n>` added (n from 1) when `${repo.root}` produced several.
````

5. Run the acceptance commands.

## Acceptance

- [ ] `grep -n "pub triggers: TriggerService" docs/technical-design.md` prints one line
- [ ] `grep -n "trigger-forward" docs/technical-design.md` prints one line
- [ ] `grep -n "### 9.1 Process definitions and arming" docs/technical-design.md` prints one line
- [ ] `grep -n '"triggers": { "enabled": false, "armed": \[\] }' docs/technical-design.md` prints one line
- [ ] `grep -n "processId>/<triggerId>" docs/technical-design.md` prints one line
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Section 10 (done by T36), section 14 (T44), section 2 (T42).
- Fixing other drift in these sections, such as the config example missing newer fields.
- Changing any code.

## If stuck

- If an anchor line is not found, the document changed. Search for the nearest equivalent (`grep -n "notify" docs/technical-design.md`) and put the text in the same place; if the place is unclear, set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 architect: named the trigger-fs-watch thread in the section 3 bullet, from the checker's note
- 2026-10-12 checker: pass
