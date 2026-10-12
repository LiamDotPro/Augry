---
id: 015-trigger-linux
title: trigger-linux
status: draft
depends_on: [001-trigger-core]
---

# 015 trigger-linux

## Goal

The crate `src-tauri/crates/trigger-linux` holds what only Linux lets the app
observe: any D-Bus signal matching a rule, a systemd unit failing, a journald
entry matching, a desktop notification from another app, an MPRIS media player
starting or stopping, a workspace change, a package manager transaction and a
power profile change. It compiles on every platform and is `Unsupported` off
Linux.

udev rules and systemd timers are not here: they call `tg run`, so they are
`launch.cli` with recipes in `trigger-launch`.

## Non goals

- Anything that works on another platform too.

## Decisions

1. **Crate** `trigger-linux`, source id `linux`, depends on `trigger-core`
   and, on Linux only, `zbus` at the version research finds.
2. **Kinds** (eight): `linux.dbus-signal`, `linux.systemd-unit-failed`,
   `linux.journal-match`, `linux.notification`, `linux.mpris`,
   `linux.workspace-changed`, `linux.package-installed`,
   `linux.power-profile`.
3. **D-Bus** is the general escape hatch: `when` has `bus` (`session` or
   `system`), `interface`, `member`, `path` and optional argument matches.
   The systemd, notification, MPRIS and power profile kinds are fixed match
   rules on top of it.
4. **journald** runs `journalctl -f -o json` with the rule's filters as a
   child process, like `trigger-docker` does with the engine. Nothing from
   `trigger-launch`: the udev and systemd timer recipes are epic 012 slice D.
5. **Workspaces** support X11 (`_NET_CURRENT_DESKTOP`), sway and Hyprland
   IPC and KDE over D-Bus; others report `Unsupported` with the desktop name.

## Shape

Written during planning: `KINDS`, `When`, pure match rules over D-Bus
message shapes and journal JSON with fixtures, the bus connection behind a
trait.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A | kinds, `When`, matchers with fixtures, the non Linux stub | |
| B | D-Bus connection, signal subscription, the fixed rules on top | A |
| C | journald child process, workspaces per desktop | A |
| D | live tests, README, feature flag, registration, repo map | B, C |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | `zbus` current version, whether its blocking API is enough or the crate needs an async runtime, and how it behaves when no session bus exists (headless CI) | Decision 1 | |
| R02 | What the Ubuntu CI runner offers: a session bus, systemd user instance, journald access | The live tests | |

## Risks

- Desktop environments differ more than the other platforms do. Every kind
  states which desktops it was tested on in the README.
