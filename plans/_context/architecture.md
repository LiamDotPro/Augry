# App architecture

The big picture for the Terminal Grid desktop app, as it matters to the
epics planned here. Owned by the architect (Opus). Epics refer to the
numbered principles; an epic that changes one updates this file in the same
commit. The detailed design of the core is `docs/technical-design.md`.

## What the app is

A terminal grid for people who run several coding agents at once: panes,
git labels and worktrees, markdown notes, a review panel. It is free, cross
platform (macOS, Windows, Linux) and fully usable with no account and no
network. The cloud (`Terminal-Grid-Cloud`, the Augry API) adds optional
extras: sign in, teams, team sync of notes, team processes and a template
catalogue.

**Processes** are the next big feature: a process is a definition (lanes of
stages of agent and shell steps, `ProcessDefinitionV1` in the API contract)
that the app runs on the user's machine, in panes. The cloud only stores and
shares definitions. Triggers are what starts a process without the user
pressing a button; `docs/process-triggers.md` is their catalogue and design.

## Principles

1. **Local first.** Everything about running processes (triggers, the run
   engine, history, logs) lives on the machine. The cloud is optional and the
   app behaves the same with it unreachable.
2. **Rust owns the OS, React owns the screen.** Anything that touches files,
   processes, sockets, devices or OS notifications is Rust in `src-tauri/`.
   The frontend renders state and sends actions over IPC. No OS access from
   the webview.
3. **One contract per boundary.** IPC: `src/ipc/types.ts` and section 10 of
   the technical design. Cloud: `libs/api-contract/openapi.yaml`, with both
   clients generated from it. Triggers: `trigger-core`. A change to a contract
   is a Decision in an epic, never a side effect of a card.
4. **One crate per integration.** Each source of triggers is a crate under
   `src-tauri/crates/` that depends on `trigger-core` only, compiles on every
   platform, and reports what it cannot do at runtime. The app composes them
   through `trigger-hub`. Part 2 of `docs/process-triggers.md` is the design;
   this principle is why.
5. **Pure core, thin shell.** OS specific code is a thin layer over pure
   functions with unit tests, so the logic is proven on every platform in CI
   and the live tests only confirm the OS layer.
6. **Nothing shared carries a machine.** Definitions a team shares use
   variables (`${repo.root}`, `${downloads}`) and names, never absolute paths,
   tokens or anything that only exists on one computer. The API refuses
   definitions with secrets; the app must not create any.
7. **Off until switched on.** New features that watch the machine (triggers,
   clipboard, window titles) are off by default, are enabled per process on
   this machine only, and ask for OS permissions when first armed, never at
   start.
8. **The app stays fast.** One poller per concern with a sensible interval
   (`AgentWatcher` is the model), events are debounced at the source and
   cooled down in the hub, and nothing blocks the main thread.

## Where the trigger work sits

```
process definition (local file, later synced)      ${variables} filled in
        │  "triggers": [ {kind, when} ]                     by the app
        ▼
  trigger-hub ──▶ trigger-fs, trigger-git, ... (crates)   ──▶ the OS
        │  Fired { process, event }
        ▼
  TriggerService (app)  ──▶ trigger://fired  ──▶ toast now, run engine later
```

## Epics

| Epic | What | Status |
|---|---|---|
| 001-trigger-core | workspace, contract, hub, `trigger-fs`, app wiring, Settings switch | draft |
| 002-trigger-internal | triggers from inside the app | draft |
| 003 to 015 | one crate per integration, all depending on 001 only | draft |
| later | process editor, run engine, trigger picker UI, team sync of definitions | not planned |
