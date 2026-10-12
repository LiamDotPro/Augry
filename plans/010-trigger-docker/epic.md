---
id: 010-trigger-docker
title: trigger-docker
status: draft
depends_on: [001-trigger-core]
---

# 010 trigger-docker

## Goal

The crate `src-tauri/crates/trigger-docker` fires on container events from
Docker and Podman: a container starting, stopping, becoming healthy or
unhealthy, and an image build finishing. It reads the engine's own event stream
and needs no polling.

## Non goals

- Starting containers. Kubernetes.

## Decisions

1. **Crate** `trigger-docker`, source id `docker`, depends on `trigger-core`
   only. It runs `docker events --format '{{json .}}'` (or `podman events`)
   as a child process and parses lines, so there is no engine API dependency
   and it works with Docker Desktop, Colima, OrbStack and Podman alike.
2. **Kinds**: `docker.container-started`, `docker.container-stopped`,
   `docker.container-healthy`, `docker.container-unhealthy`,
   `docker.image-built`.
3. **`when`**: `name` (glob on the container name), `image` (glob), `label`
   (`key=value`), `engine` (`docker`, `podman`, or `auto`).
4. **Availability** checks that the engine binary is on PATH and answers
   `version`; otherwise `Unsupported` with the reason. The child process is
   restarted with backoff when it exits.

## Shape

Written during planning: `KINDS`, `When`, the pure line parser and matcher
with fixture lines from both engines, the child process supervisor behind a
trait.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A | kinds, `When`, event line parser and matcher with fixtures | |
| B | the child process supervisor with backoff, fake process tests | A |
| C | live tests (skipped when no engine), README, feature flag, registration, repo map | B |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | The exact JSON of `docker events` and `podman events` for start, die, health_status and build events on current versions | The parser fixtures | |
| R02 | How the app finds the engine binary when launched from the Dock or Start menu, where PATH is minimal (the app already solves this for agents in `agent/installed.rs`) | Decision 4 | |

## Risks

- `docker events` blocks for ever by design; stopping the handle must kill the
  child on every OS.
