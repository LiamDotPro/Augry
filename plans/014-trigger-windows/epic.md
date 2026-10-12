---
id: 014-trigger-windows
title: trigger-windows
status: draft
depends_on: [001-trigger-core]
---

# 014 trigger-windows

## Goal

The crate `src-tauri/crates/trigger-windows` holds what only Windows lets the
app observe: an Event Log entry matching a query, a WMI event, a registry key
changing, a virtual desktop change, a WSL distro starting or stopping, a
Defender alert and an installed Windows Update. It compiles on every platform
and is `Unsupported` off Windows.

Task Scheduler, the Explorer menu and Jump Lists are not here: they all call
`tg run`, so they are `launch.cli` with recipes in `trigger-launch`.

## Non goals

- Anything that works on another platform too.

## Decisions

1. **Crate** `trigger-windows`, source id `windows`, depends on
   `trigger-core` and, on Windows only, the `windows` crate at the version
   research finds.
2. **Kinds** (eight): `windows.event-log`, `windows.wmi-event`,
   `windows.registry-changed`, `windows.virtual-desktop-changed`,
   `windows.wsl-started`, `windows.wsl-stopped`, `windows.defender-alert`,
   `windows.update-installed`.
3. **Event Log** uses `EvtSubscribe` with an XPath from `when` (`channel`,
   `query`). **WMI** uses `IWbemServices::ExecNotificationQueryAsync` with a
   WQL string from `when`. Both are the general escape hatches; the Defender,
   update and crash kinds are fixed queries on top of them.
4. **Nothing from `trigger-launch`.** This crate neither calls nor knows about
   `tg`. The Task Scheduler, Explorer menu and Jump List recipes are epic 012
   slice D. Toast buttons belong to the app, not to this crate.
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
| D | live tests, README, feature flag, registration, repo map | B, C |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | `windows` crate current version and the feature flags for Event Log, WMI, registry notifications and the virtual desktop interfaces | Decision 1 | |

## Risks

- COM and WMI from a thread need the right apartment initialisation; the
  subscription thread owns that and nothing else touches COM.
