---
id: 013-trigger-macos
title: trigger-macos
status: draft
depends_on: [001-trigger-core]
---

# 013 trigger-macos

## Goal

The crate `src-tauri/crates/trigger-macos` holds the triggers only macOS has:
a Shortcuts action or automation, a Folder Action, a Services menu entry on
selected text, a Finder Quick Action, a system Calendar event or Reminder, an
iOS Simulator booting, an Xcode build finishing, a track starting in Music,
Low Power Mode, a Space change and a finished Time Machine backup. It compiles
on every platform and is `Unsupported` off macOS.

## Non goals

- Anything that works on another platform too: those live in the shared
  crates.

## Decisions

1. **Crate** `trigger-macos`, source id `macos`, depends on `trigger-core`
   and, on macOS only, the `objc2` crates the app already uses.
2. **Kinds**: `macos.shortcut`, `macos.folder-action`, `macos.service`,
   `macos.quick-action`, `macos.calendar-event`, `macos.reminder-due`,
   `macos.simulator-booted`, `macos.xcode-build-finished`,
   `macos.music-track`, `macos.low-power-mode`, `macos.space-changed`,
   `macos.time-machine-finished`.
3. **Shortcuts, Folder Actions, Services and Quick Actions** all reach the
   app through the `tg` CLI or the deep link from `trigger-launch`; this crate
   ships the Shortcuts action definition, the Services entry and the
   Automator workflow, and the kinds that name them.
4. **Calendar and Reminders** need their permissions; `availability` reports
   `NeedsPermission` with the Settings path until granted.
5. Distributed notifications (`com.apple.Music.playerInfo`,
   `com.apple.screenIsLocked` style) are read through `NSDistributedNotificationCenter`.

## Shape

Written during planning: `KINDS`, `When`, the pure matchers, the
notification observer behind a trait.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A | kinds, `When`, matchers, the non macOS stub | |
| B | distributed notifications, Low Power Mode, Spaces, Music | A |
| C | Calendar and Reminders through EventKit | A |
| D | Shortcuts action, Services and Quick Action definitions, Simulator and Xcode, live tests, README, feature flag, registration, repo map | B, C |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | How a Tauri app exposes an App Intent for Shortcuts, or whether a Shortcuts "Run Shell Script" calling `tg` is the only workable route today | Decision 3 | |
| R02 | EventKit access from `objc2-event-kit`, its version and the entitlement and Info.plist keys needed | Slice C | |

## Risks

- App Store review and notarisation care about Calendar and Reminders usage
  strings; `tauri.macos.conf.json` must carry them.
