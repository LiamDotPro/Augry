---
id: 012-trigger-launch
title: trigger-launch
status: draft
depends_on: [001-trigger-core]
---

# 012 trigger-launch

## Goal

Anything outside the app can start a process: a `terminalgrid://run/<id>`
link in Slack or a README, the `tg run <id>` command from a script, a git hook,
cron, a launchd job or a Task Scheduler task, and a token protected call to a
local HTTP endpoint. The crate `src-tauri/crates/trigger-launch` owns the local
socket and the endpoint; the app feeds it deep links.

## Non goals

- An MCP server for agents to start processes. Worth its own epic later on
  the same socket.

## Decisions

1. **Crate** `trigger-launch`, source id `launch`, depends on `trigger-core`
   and the socket crate research names. Deep links come from
   `tauri-plugin-deep-link` in the app and are published into the crate.
2. **Kinds**: `launch.link`, `launch.cli`, `launch.http`, `launch.hook`.
3. **The socket** is a Unix socket in the app config dir, a named pipe on
   Windows, with a line protocol: `run <process id> [key=value ...]`. The
   `tg` CLI is a small binary in the same crate (`src/bin/tg.rs`), installed
   next to the app and added to PATH by the shell integration script.
4. **The HTTP endpoint** listens on `127.0.0.1` on a port from `when`, only
   while a rule is armed, and requires a bearer token the app generates and
   shows in the Settings status. `POST /run/<id>` with a JSON body as the
   payload.
5. **`when`**: `process` is implied (the rule belongs to it); `port` and
   `token` for HTTP, `name` for hooks (`pre-push`, `post-commit`).

## Shape

Written during planning: `KINDS`, the line protocol parser (pure, tested),
the socket listener behind a trait, the `tg` binary.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A | kinds, protocol parser, HTTP request parser, tests | |
| B | socket listener per platform, the `tg` binary | A |
| C | the HTTP listener, deep link feed, live tests, README, feature flag, registration, repo map | B |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | A maintained crate for Unix sockets and Windows named pipes with one API, or `std` on Unix plus `windows` on Windows, and versions | Decision 3 | |
| R02 | `tauri-plugin-deep-link` current version and what registering `terminalgrid://` needs on each OS, including an unsigned dev build on macOS | Decision 1 | |
| R03 | The smallest HTTP server crate suitable for a loopback endpoint, or whether a hand written parser over `std::net` is enough | Decision 4 | |

## Risks

- Anything that listens is an attack surface. Loopback only, token required,
  off unless armed, and the README says so.
