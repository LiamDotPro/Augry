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
output, and the microphone or camera turning on.

## Non goals

- Displays: `trigger-system`.
- MIDI, HID macro pads, print jobs and smart cards. Each is a small crate of its
  own later, made from the template, not a feature inside this one.

## Decisions

1. **Crate** `trigger-devices`, source id `devices`, depends on `trigger-core`
   and the device crates research names. No cargo features inside the crate.
2. **Kinds** (nine): `devices.usb-attached`, `devices.usb-detached`,
   `devices.drive-mounted`, `devices.drive-ejected`,
   `devices.bluetooth-connected`, `devices.bluetooth-disconnected`,
   `devices.audio-output-changed`, `devices.microphone-on`,
   `devices.camera-on` (the last two macOS and Windows).
3. **`when`**: `vendor` and `product` (hex ids or glob on the name), `label`
   for drives, `name` (glob) for Bluetooth and audio.
4. **Payload**: ids, names, the mount path for drives.

## Shape

Written during planning: `KINDS`, `When`, one pure diff `step` per family over
a snapshot, a `Probe` trait per family and platform.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A | kinds, `When`, snapshot diffs with tests | |
| B | USB, drives and Bluetooth per platform | A |
| C | audio output, microphone and camera per platform | A |
| D | live tests, README, feature flag, registration, repo map | B, C |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | Current cross platform crate for USB hot plug (`nusb` or `rusb`), its version and build requirements per OS (a system library such as libusb or libudev?) | Decision 1 | |
| R02 | How to see mounted volumes change on each OS without polling `df` | `devices.drive-mounted` | |
| R03 | How to know the microphone or camera is in use on macOS and Windows from Rust | Decision 2 | |

## Risks

- Device crates often need system libraries at build time (libusb, libudev).
  Prefer pure Rust crates; CI must prove the default build works on the three
  runners.
