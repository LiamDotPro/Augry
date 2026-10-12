---
id: 014-trigger-windows
title: trigger-windows
status: draft
depends_on: [001-trigger-core]
---

# 014 trigger-windows

## Goal

The crate `src-tauri/crates/trigger-windows` holds the triggers only Windows
has: an Event Log entry matching a query, a Task Scheduler trigger, a WMI
event, a registry key changing, a File Explorer context menu entry, a Jump List
task, a toast notification button, a virtual desktop change, a WSL distro
starting or stopping, a Defender alert and an installed Windows Update. It
compiles on every platform and is `Unsupported` off Windows.

## Non goals

- Anything that works on another platform too.

## Decisions

1. **Crate** `trigger-windows`, source id `windows`, depends on
   `trigger-core` and, on Windows only, the `windows` crate at the version
   research finds.
2. **Kinds**: `windows.event-log`, `windows.scheduled-task`,
   `windows.wmi-event`, `windows.registry-changed`, `windows.explorer-menu`,
   `windows.jump-list`, `windows.toast-action`,
   `windows.virtual-desktop-changed`, `windows.wsl-started`,
   `windows.wsl-stopped`, `windows.defender-alert`,
   `windows.update-installed`.
3. **Event Log** uses `EvtSubscribe` with an XPath from `when` (`channel`,
   `query`). **WMI** uses `IWbemServices::ExecNotificationQueryAsync` with a
   WQL string from `when`. Both are the general escape hatches; the Defender,
   update and crash kinds are fixed queries on top of them.
4. **Task Scheduler, Explorer menu and Jump List** reach the app through the
   `tg` CLI from `trigger-launch`; this crate ships the registrations
   (registry entries for the menu, the Jump List tasks).
5. **WSL** is polled with `wsl.exe --list --running` every 5 seconds while a
   rule is armed.

## Shape

Written during planning: `KINDS`, `When`, pure parsers for Event Log XML and
WMI objects with fixtures, the subscription layers behind traits.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A | kinds, `When`, parsers with fixtures, the non Windows stub | |
| B | Event Log and WMI subscriptions | A |
| C | registry watch, virtual desktop, WSL polling | A |
| D | Explorer menu and Jump List registration, toast actions, live tests, README, feature flag, registration, repo map | B, C |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | `windows` crate current version and the feature flags for Event Log, WMI, registry notifications and the virtual desktop interfaces | Decision 1 | |
| R02 | Whether a packaged (MSIX, Microsoft Store) build may register Explorer context menu entries and Jump List tasks, and how that differs from the NSIS build | Decision 4 | |

## Risks

- COM and WMI from a thread need the right apartment initialisation; the
  subscription thread owns that and nothing else touches COM.
