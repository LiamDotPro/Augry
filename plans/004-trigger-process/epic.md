---
id: 004-trigger-process
title: trigger-process
status: draft
depends_on: [001-trigger-core]
---

# 004 trigger-process

## Goal

The crate `src-tauri/crates/trigger-process` fires when another program starts,
stops or crashes, when one uses too much CPU or memory for too long, when a
port starts or stops listening, when the frontmost app changes, and when a
dev server prints that it is ready. It is the "another app starting up" trigger
on every platform.

## Non goals

- Containers: `trigger-docker`. Agents inside Terminal Grid's own panes:
  `trigger-internal`.

## Decisions

1. **Crate** `trigger-process`, source id `process`, depends on `trigger-core`
   and `sysinfo` (workspace). Its own poller thread, interval from the rules
   (default 2 seconds), modelled on the app's `AgentWatcher` but independent of
   it: the crate never imports app code and the app's watcher is not changed.
2. **Kinds**: `process.started`, `process.stopped`, `process.crashed`,
   `process.high-cpu`, `process.high-memory`, `process.port-opened`,
   `process.port-closed`, `process.frontmost-changed` (macOS and Windows),
   `process.window-appeared` (macOS and Windows, needs Accessibility on
   macOS).
3. **`when`**: `name` (glob on the executable name or a full command line
   substring), `port`, `percent`, `megabytes`, `for_s`, `title` (glob) for
   windows.
4. **Payload**: `pid`, `name`, `command`, `port`, `cpu_percent`, `mem_bytes`,
   `exit_code` when known, `title` and `app` for windows.
5. **Ports** are read from the OS tables: `/proc/net/tcp` on Linux, `lsof`
   or `netstat` fallbacks elsewhere per research, never by connecting.

## Shape

Written during planning: `KINDS`, `When`, the pure `step` over a
`ProcessSnapshot` diff with the `for_s` timers, the `Frontmost` trait with a
macOS and a Windows implementation.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A | kinds, `When`, snapshot diff `step` with tests | |
| B | the `sysinfo` poller and the port table per platform | A |
| C | frontmost app and window title, macOS (`src/os/macos.rs`) | A |
| D | frontmost app and window title, Windows (`src/os/windows.rs`) | A |
| E | live tests, README, feature flag, app registration, repo map | B, C, D |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | The cheapest way to list listening TCP ports on macOS, Windows and Linux from Rust without a privileged call | Decision 5 | |
| R02 | How to be told the frontmost app changed on macOS (`NSWorkspace` notification from `objc2-app-kit`, which the app already uses) and on Windows (`SetWinEventHook`), and which crate versions | Slices C and D | |
| R03 | How the OS reports a crash versus a normal exit for a process the app did not start | `process.crashed` | |

## Risks

- Two pollers of the process table (the app's `AgentWatcher` and this crate)
  cost more than one. Accepted for isolation: this crate only polls while a
  `process.*` rule is armed, refreshes only the fields its rules need, and uses
  the longest interval its rules allow.
