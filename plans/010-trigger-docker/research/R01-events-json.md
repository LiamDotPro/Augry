---
id: R01
epic: 010-trigger-docker
question: The exact JSON of `docker events` and `podman events` for start, die, health_status and build events on current versions
status: partial
---

## Answer

Docker lines below are captured from a real daemon (Docker Desktop, Engine 29.4.3, API 1.54; Engine 27/28 print the same shape). Podman is not installed here, so the Podman lines are generated with the real v5.6.0 `Event` struct (compiled Go, marshalled) and are not captured from a running Podman.
Docker: `{"Type","Action","Actor":{"ID","Attributes":{name,image,labels...}},"scope","time","timeNano"}`. Action is `start`, `die` (exit code in `Actor.Attributes.exitCode`, a string), `stop`, `health_status: healthy`, `health_status: unhealthy`.
Podman: flat `{"ID","Image","Name","Status","time","timeNano","Type","Attributes"{labels}}`. Status is `start`, `died` (exit code in top level `ContainerExitCode`, a number), `stop`, `health_status` with the verdict in top level `health_status`.
Build: BuildKit emits no container event. It emits `image create` then `image tag` (only if tagged). Podman emits `{"Status":"build","Type":"image","Name":<ref>}`. There is no Docker `build` event.
Flag: both accept `--format '{{json .}}'`. Docker also accepts `--format json`; Podman's own docs say `--format json` too (Podman 5 `events --format json` and `'{{json .}}'` both print the same JSON line).

## Evidence

Ran on 2026-10-12 against Docker Desktop (`docker version` client 29.4.3, server API 1.54, containerd v2.2.3, containerd image store). Test image: alpine:3 with `HEALTHCHECK --interval=1s --timeout=1s --retries=1 CMD test -f /ok`, label `com.example.team=x`, run with `--label com.example.k=v`, stopped with `docker stop -t 1`, plus `docker run alpine:3 sh -c 'exit 3'`. Built once with BuildKit and once with `DOCKER_BUILDKIT=0`. Event stream captured with `docker events --format '{{json .}}'`.

Podman: read `libpod/events/config.go`, `libpod/events.go`, `libpod/runtime_img.go`, `cmd/podman/system/events.go` at tag v5.6.0 on raw.githubusercontent.com (2026-10-12), and https://docs.podman.io/en/latest/markdown/podman-events.1.html. The CLI prints `newEventFromLibpodEvent(e).ToJSONString()` which is `json.Marshal` of the struct below (events.go in cmd/podman/system):

```go
type Event struct {
  ContainerExitCode *int `json:",omitempty"`
  ID, Image, Name string `json:",omitempty"`
  Network string `json:"network,omitempty"`
  Status events.Status          // no tag => key "Status"
  Time int64 `json:"time,omitempty"`
  TimeNano int64 `json:"timeNano,omitempty"`
  Type events.Type              // key "Type"
  HealthStatus string `json:"health_status,omitempty"`
  Error string `json:",omitempty"`
  events.Details  // embedded: ContainerInspectData, PodID (omitempty), Attributes (no tag, no omitempty)
}
```

Facts from source: `Exited` status string is `"died"`; `HealthStatus` is `"health_status"`; `Build` is `"build"`; build event is written by `newImageBuildCompleteEvent` with only Type=image and Name set (ID empty); the died event sets `ContainerExitCode`; health events set `health_status` (and an internal health log/streak that the CLI struct drops); `Attributes` carries the container labels (nil map prints `"Attributes":null`). Exit code on `remove` events also appears when known.

### Docker lines (verbatim captures)

Container start (labels from the image and from `--label` appear in Attributes; Docker Desktop adds `desktop.docker.io/ports.scheme`):

```
{"Type":"container","Action":"start","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","com.example.team":"x","desktop.docker.io/ports.scheme":"v2","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837794,"timeNano":1791837794607003258}
```

Health, first unhealthy then healthy (the second check after `/ok` appeared):

```
{"Type":"container","Action":"health_status: unhealthy","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","com.example.team":"x","desktop.docker.io/ports.scheme":"v2","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837795,"timeNano":1791837795647713133}
{"Type":"container","Action":"health_status: healthy","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","com.example.team":"x","desktop.docker.io/ports.scheme":"v2","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837797,"timeNano":1791837797743493218}
```

`docker stop -t 1` sequence: kill signal 15, kill signal 9, then `stop`, then `die` (exitCode 137 and execDuration), then `destroy` after `rm`:

```
{"Type":"container","Action":"kill","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","com.example.team":"x","desktop.docker.io/ports.scheme":"v2","image":"r01test:1","name":"r01c","signal":"9"}},"scope":"local","time":1791837807,"timeNano":1791837807655361458}
{"Type":"container","Action":"stop","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","com.example.team":"x","desktop.docker.io/ports.scheme":"v2","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837807,"timeNano":1791837807760686416}
{"Type":"container","Action":"die","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","com.example.team":"x","desktop.docker.io/ports.scheme":"v2","execDuration":"13","exitCode":"137","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837807,"timeNano":1791837807764166125}
```

Natural exit with code 3 (no `stop` event, only `die`):

```
{"Type":"container","Action":"die","Actor":{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Attributes":{"desktop.docker.io/ports.scheme":"v2","execDuration":"0","exitCode":"3","image":"alpine:3","name":"r01x"}},"scope":"local","time":1791837807,"timeNano":1791837807993939750}
```

Noise the parser must ignore (and not treat as `die`): `exec_create: <cmd>`, `exec_start: <cmd>`, `exec_die` (has its own `exitCode` and `execID`; one set per health probe, every interval), `attach`, `create`, `destroy`, `kill`, plus `network` and `image` types.

Image build, BuildKit with the containerd image store (the whole build was these two lines, no container events):

```
{"Type":"image","Action":"create","Actor":{"ID":"sha256:d760162063cb2959aa76446b17be2d95caa8289dd13aca56a65e12ec54eafa9f","Attributes":{"com.example.team":"x","name":"sha256:d760162063cb2959aa76446b17be2d95caa8289dd13aca56a65e12ec54eafa9f"}},"scope":"local","time":1791837793,"timeNano":1791837793998815716}
{"Type":"image","Action":"tag","Actor":{"ID":"sha256:d760162063cb2959aa76446b17be2d95caa8289dd13aca56a65e12ec54eafa9f","Attributes":{"com.example.team":"x","name":"r01test:1"}},"scope":"local","time":1791837794,"timeNano":1791837794011737591}
```

Legacy builder (`DOCKER_BUILDKIT=0`): per step a `container create`/`container destroy` pair of temporary containers (random names like `eloquent_hoover`, no start), one `image create` per step, then a final `image create` and `image tag` (`name":"r01test:2"`). So `image` + `tag` is the common last event for a tagged build on both builders. An untagged build only gives `image create`. A plain `docker tag` also emits `image tag`, and a pull emits `image pull` (not `tag`), so `tag` is slightly over broad (see Doubts).

### Podman lines (generated from the v5.6.0 struct, not captured)

```
{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"start","time":1791799200,"timeNano":1791799200123456789,"Type":"container","Attributes":{"com.example.k":"v"}}
{"ContainerExitCode":3,"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"died","time":1791799200,"timeNano":1791799200123456789,"Type":"container","Attributes":{"com.example.k":"v"}}
{"ContainerExitCode":137,"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"died","time":1791799200,"timeNano":1791799200123456789,"Type":"container","Attributes":{"com.example.k":"v"}}
{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"stop","time":1791799200,"timeNano":1791799200123456789,"Type":"container","Attributes":{"com.example.k":"v"}}
{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"health_status","time":1791799200,"timeNano":1791799200123456789,"Type":"container","health_status":"healthy","Attributes":{"com.example.k":"v"}}
{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"health_status","time":1791799200,"timeNano":1791799200123456789,"Type":"container","health_status":"unhealthy","Attributes":{"com.example.k":"v"}}
{"Name":"localhost/r01test:1","Status":"build","time":1791799200,"timeNano":1791799200123456789,"Type":"image","Attributes":null}
```

### Field differences

| Item | Docker | Podman |
|---|---|---|
| shape | nested `Actor.Attributes` | flat top level, labels in `Attributes` |
| event kind | `Action` (string, can carry `: detail`) | `Status` |
| type | `Type` | `Type` |
| container name | `Actor.Attributes.name` | `Name` |
| image | `Actor.Attributes.image` (as given, or a sha when built just now) | `Image` (usually fully qualified, e.g. `docker.io/library/alpine:3`) |
| labels | mixed into `Actor.Attributes` with name, image, exitCode, signal, execID | `Attributes` only (no name/image inside) |
| exit code | `Actor.Attributes.exitCode` as a string, only on `die` and `exec_die` | `ContainerExitCode` as a number, on `died` (and on `remove` when known) |
| health | `Action` is `health_status: healthy` or `health_status: unhealthy` (also `health_status: starting` is not emitted on a plain run; do not rely on it) | `Status:"health_status"` plus `health_status` |
| time | `time` seconds, `timeNano` ns | `time` seconds, `timeNano` ns (same names, same units) |
| scope | `"scope":"local"` | absent |
| die means | process ended | `died` |
| build finished | `image` `tag` (or `create`) | `image` `build` with `Name` = image ref |

Docker 29 no longer prints the legacy `status`, `id`, `from` keys (none appeared in the capture). Older engines may still print them, so the parser must not require or rely on them. Verify the Engine 27/28 claim: not run here, only inferred from the stable `events.Message` type in moby; see Doubts.

## Implications for the cards

Command lines: Docker `docker events --format '{{json .}}'`; Podman `podman events --format json` (also valid: `'{{json .}}'`). Add `--filter type=container --filter type=image` to cut noise (exec_* events are the bulk). Do not add `--since` for the live stream; on restart of the child the supervisor would otherwise replay events, so on restart pass `--since <last timeNano seen / 1e9>` only if duplicates are acceptable, else omit it (events during the gap are lost, which is acceptable for triggers).

Parser design (pure function `parse_line(&str) -> Option<DockerEvent>`):

1. `serde_json::from_str::<serde_json::Value>`; invalid or non object lines return `None` (the stream can print errors on stderr only, but be tolerant).
2. Normalise to one struct: `{ engine_shape, kind_type: String, action: String, id: String, name: Option<String>, image: Option<String>, labels: BTreeMap<String,String>, exit_code: Option<i32>, time_nano: Option<i64> }`.
3. Detect shape by key: if `Actor` is an object it is the Docker shape, else the Podman shape (`Status` present). Do not trust the `engine` config for parsing, only for choosing the binary.
4. Docker: type = `Type`; action = `Action`; split action at the first `:` to get `base` and `detail` (trim). id = `Actor.ID`; name = `Attributes.name`; image = `Attributes.image`; exit code = `Attributes.exitCode` parsed from a string; labels = all `Attributes` entries except `name`, `image`, `exitCode`, `execDuration`, `execID`, `signal`, `container`, `type`. (Label keys can collide with these in theory; accept that.)
5. Podman: type = `Type`; action = `Status`; id = `ID`; name = `Name`; image = `Image`; exit code = `ContainerExitCode` (number); labels = `Attributes` (may be `null`, treat as empty); health verdict = `health_status`. Map `died` to `die` so the next step has one vocabulary.
6. Time: use `timeNano` if present, else `time * 1e9`. Both engines supply both.
7. Mapping to kinds (container events, base action after normalising):
   - `start` -> `docker.container-started`
   - `die` -> `docker.container-stopped` with `exit_code`. Use `die`, not `stop`: `stop` is only emitted for `docker stop`, never for a container that exits by itself, and `die` is emitted in both cases. Ignore `stop` and `kill` so one stop does not fire twice.
   - `health_status` with verdict `healthy` -> `docker.container-healthy`; `unhealthy` -> `docker.container-unhealthy`. Docker verdict comes from the text after `:`, Podman from `health_status`. Ignore other verdicts (`starting`).
   - Do not match on `exec_die`: `Action` equals `exec_die` or starts with `exec_`, so compare the base only after splitting at `:` and compare whole strings, never `starts_with("die")`.
   - Image: Docker `Type=image`, base `tag` -> `docker.image-built` with `name` = `Attributes.name` (the new tag). Podman `Type=image`, `Status=build` -> `docker.image-built` with `name` = `Name`. Ignore Docker `image create` (it fires once per legacy build step and for untagged layers), and ignore `pull`, `untag`, `delete`.
8. Matcher: `name` glob on container name (for image events match against the image ref), `image` glob against the image (also try with the `docker.io/library/` prefix stripped on Podman, and with a trailing `:latest` optional), `label` `key=value` exact match against `labels`, `engine` against the binary actually running (docker or podman), not the JSON shape.
9. Event payload fields for the trigger: `engine`, `container_id`, `name`, `image`, `exit_code` (only for stopped), `health` (for health kinds), `time_nano`.

Fixtures: use the verbatim Docker lines above for start, both health lines, `die` 137, `die` 3, `kill` signal 9 (must be ignored), `stop` (must be ignored), both image lines (`create` ignored, `tag` yields image-built), and add three noise fixtures: an `exec_die` line (must not produce stopped; take one of the captures that carries `"exitCode":"1"` and `execID`), a `container create` line, a `network connect` line. Use the Podman lines above for the Podman side and label them synthetic in a comment; the live test (slice C) should replace them with a capture when a Podman is available.

Gotchas:
- Docker `exitCode` is a JSON string; Podman `ContainerExitCode` is a JSON number. Accept both in one `Value` helper.
- `health_status: healthy` is one Action string with a space after the colon. Match on trimmed text.
- When the Docker health check is configured with `--retries=1` the first probe may fail and give `unhealthy` before `healthy`; that is real engine behaviour, not a parser bug.
- Dockerfile LABELs end up on `image` events and on container events of that image, so label matching works on both.
- Colima, OrbStack, Rancher Desktop with dockerd all print the Docker shape. Rancher Desktop with containerd and nerdctl is out of scope.

## Doubts

- No Podman available on this machine. The Podman lines come from the source struct at v5.6.0, marshalled with real Go `encoding/json`, so key order and names are right for v5.x, but a live capture is missing. Latest Podman release is v6.1.3 (GitHub API, 2026-10-12); the `main` branch moved the entities types to `go.podman.io/podman/v6/pkg/domain/entities/types`, and I did not confirm the CLI JSON shape is unchanged in v6. Podman 5 behaviour is as stated.
- `podman events --format json` via a remote `podman machine`/`CONTAINER_HOST` goes through the API and the entities converter, which is not the same code path; the printed JSON is built from the same CLI struct, so should be identical, but is not verified.
- Docker Engine 27/28 were not run. Only Engine 29.4.3 was captured. The `events.Message` shape (Type, Action, Actor, scope, time, timeNano) is long stable in moby, but I did not check whether 27/28 still also print legacy `status`, `id`, `from` (older versions did). The parser design ignores those keys, so it is safe either way.
- The containerd image store (Docker Desktop default on new installs) may emit a different set for builds than the classic graph driver store with BuildKit. Not tested on the classic store. If a build with classic store emits only `image tag`... that still matches. If it emitted only `image create` for an untagged build, no event fires (acceptable).
- `image tag` also fires for a manual `docker tag`, and `docker pull` of an already built image does not. Treating `tag` as "build finished" can give false positives for `docker tag`. There is no engine event that says "a build ran" on Docker, so this is the best available signal. The epic may want to document it in the README.
- Health `starting` was not observed; not verified whether any engine emits it.
