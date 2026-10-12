---
id: 010-trigger-docker
title: trigger-docker
status: carding
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

5. **Parsing** follows R01 exactly: one pure `parse_line(&str) ->
   Option<EngineEvent>` that detects the shape by whether `Actor` is an object
   (Docker) or not (Podman), normalises both (Podman `died` to `die`, exit
   code from a string or a number, `timeNano` else `time * 1e9`), and maps:
   `start` to started, `die` to stopped (never `stop`, `kill` or `exec_*`;
   compare whole base actions split at the first `:`), `health_status`
   verdicts to healthy and unhealthy (ignore `starting`), Docker `image tag`
   and Podman `image build` to image-built. A manual `docker tag` also fires
   image-built; that is a Known limit in the README.
6. **The command** is `<bin> events --format '{{json .}}' --filter
   type=container --filter type=image` for Docker and `<bin> events --format
   json --filter type=container --filter type=image` for Podman, with no
   `--since`. Events while the child is restarting are lost; accepted.
7. **Finding the binary** is the crate's own `find_engine` in `os.rs` (R02):
   process PATH, then R02's candidate folders per OS, first `is_file()` wins,
   cached in a `OnceLock`. No login shell probe. `availability()` only checks
   that a binary was found, because the catalogue calls it on the UI thread.
8. **`start`** runs `<bin> version --format '{{.Server.Version}}'` (5 s,
   `try_wait` poll, kill on timeout, `CREATE_NO_WINDOW`). No answer is
   `SourceError::Unavailable("<engine> found at <path> but the engine did not
   answer")`, so Settings shows it. `engine: auto` tries docker then podman.
   Rules naming different engines run one child per engine.
9. **The supervisor** owns the `Child`, reads stdout lines on a named thread,
   restarts the child with backoff 1, 2, 4 ... 60 s when it exits, and kills
   then waits it on stop. It is behind `trait Engine: Send { fn spawn(&self)
   -> std::io::Result<Box<dyn Lines>> }` so tests use a fake that replays
   fixture lines and a fake that exits.
10. **Payload**: `engine`, `container_id` (12 characters), `name`, `image`,
    `exit_code` (stopped only), `health` (health kinds only), `time` (ms).
    The image glob also matches with `docker.io/library/` stripped and with
    `:latest` optional.
11. **Registration**: cargo feature `trigger-docker` (default on) and one line
    in `sources()`, exactly like `trigger-fs`.

## Shape

```
src-tauri/crates/trigger-docker/
  src/lib.rs     KINDS (5), DockerSource, start
  src/rule.rs    When { name, image, label, engine }, parse
  src/logic.rs   EngineEvent, parse_line, matches(rule, event), payload
  src/logic/tests.rs   fixture lines from R01 (Docker real, Podman marked synthetic)
  src/os.rs      find_engine, version check, Engine impl over std::process
  src/supervisor.rs    restart with backoff, kill on stop, behind the Engine trait
  tests/live.rs  #[ignore]; skipped with a message when no engine answers
```

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A (T01 to T09) | scaffold, KINDS, `rule.rs`, `parse_line` and matcher with R01's fixtures | |
| B (T10 to T19) | the supervisor with backoff and kill, tested with fake engines | A |
| C (T20 to T29) | `os.rs` (find, version check, real engine), glue, live tests against Docker, README | B |
| D (T30 to T39) | app: feature and `sources()` line, repo map row | C |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | The exact JSON of `docker events` and `podman events` for start, die, health_status and build events on current versions | The parser fixtures | Docker 29 captured live; Podman from source (partial). Folded into Decisions 5, 6, 10. |
| R02 | How the app finds the engine binary when launched from the Dock or Start menu, where PATH is minimal (the app already solves this for agents in `agent/installed.rs`) | Decision 4 | PATH then candidate folders; `version --format` check with 5 s timeout; kill then wait on stop; no `--since`. Folded into 7, 8, 9. |

## Risks

- `docker events` blocks for ever by design; stopping the handle must kill the
  child on every OS.
