---
id: 008-trigger-input
title: trigger-input
status: draft
depends_on: [001-trigger-core]
---

# 008 trigger-input

## Goal

The crate `src-tauri/crates/trigger-input` fires when the clipboard gets text,
a URL or an image that matches a rule, when a global hotkey is pressed, when a
file is dropped on the app or opened with it, and when a tray menu entry is
chosen. These are the "I want this now" triggers.

## Non goals

- Selected text in other apps, Services menus and share targets: the
  platform crates.

## Decisions

1. **Crate** `trigger-input`, source id `input`, depends on `trigger-core`
   and the clipboard and hotkey crates research names. The drop, open with and
   tray kinds are fed by the app through `publish`, since Tauri owns those.
2. **Kinds**: `input.clipboard-text`, `input.clipboard-url`,
   `input.clipboard-image`, `input.hotkey`, `input.file-dropped`,
   `input.file-opened`, `input.tray-menu`.
3. **`when`**: `pattern` (regex) or `host` (glob on the URL host), `keys`
   (such as `cmd+shift+r`), `ext` (list) for files, `label` for the tray.
4. **Clipboard watching is opt in and visible**: it only runs while a rule is
   armed, polls the change count every 500 ms, never stores what it reads, and
   puts at most 200 characters of a match in the payload. Image payloads are a
   temp file path, deleted after one minute.
5. **Wayland** has no global clipboard or hotkeys without a portal; the kinds
   report `Unsupported` there with the reason.

## Shape

Written during planning: `KINDS`, `When`, the pure matchers, a `Clipboard`
trait with a fake.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A | kinds, `When`, matchers, fake clipboard tests | |
| B | clipboard and hotkeys on macOS and Windows | A |
| C | clipboard and hotkeys on Linux X11, Wayland reported as unsupported | A |
| D | app feeds for drop, open with and tray; live tests, README, feature flag, registration, repo map | B, C |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | Current versions of `arboard` (clipboard) and `tauri-plugin-global-shortcut`, and whether `arboard` exposes a change count or the crate must hash | Decision 1 and 4 | |
| R02 | What Tauri 2 emits for a file drop and for "open with" on each OS, and the capability it needs | Slice D | |

## Risks

- Clipboard watching is the most sensitive trigger in the catalogue. The
  README and the Settings status must say plainly when it is running.
