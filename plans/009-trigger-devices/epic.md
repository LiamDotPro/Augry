---
id: 009-trigger-devices
title: trigger-devices
status: draft
depends_on: [001-trigger-core]
---

# 009 trigger-devices

## Goal

The crate `src-tauri/crates/trigger-devices` fires when hardware comes and
goes: a USB device, a drive or SD card, a Bluetooth device, a change of audio
output, a MIDI note, a button on a HID device such as a macro pad, a finished
print job, a smart card, and the microphone or camera turning on.

## Non goals

- Displays: `trigger-system`.

## Decisions

1. **Crate** `trigger-devices`, source id `devices`, depends on `trigger-core`
   and the device crates research names, one per family, each behind its own
   cargo feature inside the crate so a build can leave MIDI or HID out.
2. **Kinds**: `devices.usb-attached`, `devices.usb-detached`,
   `devices.drive-mounted`, `devices.drive-ejected`,
   `devices.bluetooth-connected`, `devices.bluetooth-disconnected`,
   `devices.audio-output-changed`, `devices.midi-note`, `devices.hid-button`,
   `devices.print-job-done`, `devices.smart-card-inserted`,
   `devices.microphone-on`, `devices.camera-on` (macOS and Windows).
3. **`when`**: `vendor` and `product` (hex ids or glob on the name), `label`
   for drives, `name` (glob) for Bluetooth and audio, `note` and `channel` for
   MIDI, `button` for HID.
4. **Payload**: ids, names, the mount path for drives, note and velocity for
   MIDI.

## Shape

Written during planning: `KINDS`, `When`, one pure diff `step` per family over
a snapshot, a `Probe` trait per family and platform.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A | kinds, `When`, snapshot diffs with tests | |
| B | USB, drives and Bluetooth per platform | A |
| C | audio output, microphone and camera per platform | A |
| D | MIDI and HID | A |
| E | printing and smart cards, live tests, README, feature flags, registration, repo map | B, C, D |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | Current cross platform crates for USB hot plug (`nusb` or `rusb`), HID (`hidapi`), MIDI (`midir`) and their versions and build requirements per OS | Decision 1 | |
| R02 | How to see mounted volumes change on each OS without polling `df` | `devices.drive-mounted` | |
| R03 | How to know the microphone or camera is in use on macOS and Windows from Rust | Decision 2 | |

## Risks

- Device crates often need system libraries at build time (libusb, libudev).
  Each one must be behind a feature and CI must prove the default build works
  on the three runners.
