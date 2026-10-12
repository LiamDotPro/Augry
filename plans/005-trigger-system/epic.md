---
id: 005-trigger-system
title: trigger-system
status: draft
depends_on: [001-trigger-core]
---

# 005 trigger-system

## Goal

The crate `src-tauri/crates/trigger-system` fires on the state of the machine:
wake and sleep, lock and unlock, idle and active, login, battery and charger,
low battery, light and dark mode, a display connected or disconnected, low disk
space, a time zone change and shutdown.

## Non goals

- Network state: `trigger-network`. Devices other than displays:
  `trigger-devices`.

## Decisions

1. **Crate** `trigger-system`, source id `system`, depends on `trigger-core`
   plus one platform crate per OS, named after research.
2. **Kinds**: `system.wake`, `system.sleep`, `system.lock`, `system.unlock`,
   `system.idle`, `system.active`, `system.login`, `system.on-battery`,
   `system.on-charger`, `system.low-battery`, `system.dark-mode`,
   `system.light-mode`, `system.display-connected`,
   `system.display-disconnected`, `system.low-disk`,
   `system.timezone-changed`, `system.shutdown`.
3. **`when`**: `after_min` for idle, `percent` for low battery, `path` and
   `gigabytes` for low disk, `name` (glob) for displays.
4. **Mechanism per platform**: macOS `NSWorkspace` and `IOKit` notifications
   (through the `objc2` crates the app already has), Windows window messages
   (`WM_POWERBROADCAST`, `WTS_SESSION_LOCK`, `WM_DISPLAYCHANGE`) on a hidden
   message window, Linux `logind` and `UPower` over D-Bus plus the xdg portal
   for the colour scheme. Idle uses `CGEventSource`, `GetLastInputInfo` and
   logind's `IdleHint`.

## Shape

Written during planning: `KINDS`, `When`, a pure `step` over a
`SystemSnapshot { awake, locked, idle_s, on_battery, battery_percent, dark,
displays, free_bytes, tz }`, and a `Probe` trait per platform.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A | kinds, `When`, snapshot diff `step` with tests | |
| B | macOS probe and notifications | A |
| C | Windows message window | A |
| D | Linux D-Bus probe | A |
| E | live tests, README, feature flag, app registration, repo map | B, C, D |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | Which Rust crates give sleep, wake, lock and power notifications on each OS today, their versions, and whether they need a run loop or a message pump the crate must own | Decision 4 | |
| R02 | How to read the idle time on each OS without permissions | `system.idle` | |
| R03 | Does Linux need both `zbus` and the xdg portal, and which versions | Slice D | |

## Risks

- Lock and unlock are not observable on every Linux desktop. The crate reports
  `Unsupported` with the reason instead of guessing.
