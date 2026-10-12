---
id: 006-trigger-network
title: trigger-network
status: draft
depends_on: [001-trigger-core]
---

# 006 trigger-network

## Goal

The crate `src-tauri/crates/trigger-network` fires when the machine goes
online or offline, joins or leaves a named Wi-Fi network, connects or
disconnects a VPN, when a host becomes reachable or unreachable, when a
service shows up on the local network, when the public IP changes and when the
connection becomes metered.

## Non goals

- Anything that needs an account with a web service: `trigger-github` and
  later polling crates.

## Decisions

1. **Crate** `trigger-network`, source id `network`, depends on `trigger-core`
   plus the platform crates research names.
2. **Kinds**: `network.online`, `network.offline`, `network.wifi-joined`,
   `network.wifi-left`, `network.vpn-connected`, `network.vpn-disconnected`,
   `network.host-reachable`, `network.host-unreachable`,
   `network.service-found`, `network.public-ip-changed`, `network.metered`.
3. **`when`**: `ssid` (glob), `host` and `port` and `interval_s` for
   reachability, `service` (an mDNS type such as `_ssh._tcp`) and `name`.
4. **Reachability** is a TCP connect with a 2 second timeout on the rule's
   interval (default 30 seconds), never ICMP. Public IP uses a fixed, documented
   HTTPS endpoint, only when a rule asks for it, at most every 10 minutes.
5. **Wi-Fi name** needs Location permission on macOS; the kind says so and
   `availability` reports `NeedsPermission` until it is granted.

## Shape

Written during planning: `KINDS`, `When`, a pure `step` over a
`NetworkSnapshot`, a `Probe` trait per platform, and the reachability loop.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A | kinds, `When`, snapshot diff, reachability loop with a fake dialer, tests | |
| B | macOS (`NWPathMonitor`, CoreWLAN) | A |
| C | Windows (`NotifyIpInterfaceChange`, WLAN API) | A |
| D | Linux (NetworkManager over D-Bus, Avahi) | A |
| E | live tests, README, feature flag, app registration, repo map | B, C, D |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | Which Rust crates expose path and interface change notifications on each OS, their versions, and whether reading the SSID needs more than Location on macOS 14 and newer | Decision 2 and 5 | |
| R02 | An mDNS browsing crate that works on all three platforms, or whether to shell out to `dns-sd`, `avahi-browse` and the Windows API | `network.service-found` | |

## Risks

- VPN detection is heuristic (a `utun` or `tun` interface, a `wg` or
  `ppp` name). Document what counts as a VPN per platform.
