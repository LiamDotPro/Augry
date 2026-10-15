# trigger-docker

Fires on container and image events from Docker and Podman.

Source id `docker`. One crate, one source: everything it fires starts with
`docker.`.

## Kinds

Generated from `KINDS` in `src/lib.rs`. Refresh with
`cargo run -p trigger-docker --example readme` and paste the output between
the markers; `tests/readme.rs` fails while this table is stale.

<!-- kinds -->
| Kind | Title | Platforms | Permission | When | Payload |
| --- | --- | --- | --- | --- | --- |
| docker.container-started | Container started | macOS, Windows, Linux | none | name, image, label, engine | engine, container_id, name, image, time |
| docker.container-stopped | Container stopped | macOS, Windows, Linux | none | name, image, label, engine | engine, container_id, name, image, time, exit_code |
| docker.container-healthy | Container healthy | macOS, Windows, Linux | none | name, image, label, engine | engine, container_id, name, image, time, health |
| docker.container-unhealthy | Container unhealthy | macOS, Windows, Linux | none | name, image, label, engine | engine, container_id, name, image, time, health |
| docker.image-built | Image built | macOS, Windows, Linux | none | name, image, label, engine | engine, name, image, time |
<!-- /kinds -->

## Rule fields

The `when` object of a trigger, parsed by `src/rule.rs`. Unknown fields are an
error, so a typo shows up in the editor.

| Field | Required | Default | Meaning |
|---|---|---|---|
| `name` | no | any | Glob on the container name. For `docker.image-built` it is tested against the image reference |
| `image` | no | any | Glob on the image. `docker.io/library/` and `:latest` are optional, so `alpine` matches `docker.io/library/alpine:latest` |
| `label` | no | any | `key=value`. The container (or image) must carry that label with exactly that value |
| `engine` | no | `auto` | `docker`, `podman` or `auto`. Which engine the rule listens to |

## Payload

What an event carries, available to steps as `${trigger.<field>}`. Paths, names,
ids and numbers only. Never file contents or secrets, never more than 4 KiB.

| Field | Meaning |
|---|---|
| `engine` | `docker` or `podman`, whichever printed the event |
| `container_id` | First 12 characters of the container id. Not on `docker.image-built` |
| `name` | Container name, or the image reference for `docker.image-built` |
| `image` | Image the container runs, as the engine reports it. For `docker.image-built` the same as `name` |
| `time` | When the engine reported it, in milliseconds since 1970 |
| `exit_code` | `docker.container-stopped` only. Exit code of the main process, `-1` when the engine did not report one |
| `health` | `docker.container-healthy` and `docker.container-unhealthy` only: `healthy` or `unhealthy` |

## Platforms

| Platform | How it is detected | Limits |
|---|---|---|
| macOS | The `docker` or `podman` tool is looked for on the app's `PATH`, then in `/usr/local/bin`, `/opt/homebrew/bin`, `/opt/podman/bin`, `/usr/bin`, `/snap/bin`, `/Applications/Docker.app/Contents/Resources/bin`, `~/.orbstack/bin`, `~/.rd/bin` and `~/.docker/bin`. Docker Desktop, Colima, OrbStack and Rancher Desktop (with dockerd) all use the Docker tool. Checked live with Docker Desktop | An app started from the Dock has a short `PATH`, so a tool outside those folders is not found |
| Windows | The same, with `docker.exe` and `podman.exe`, on `PATH` then in the Docker Desktop, Podman and Rancher Desktop folders under `%ProgramFiles%`, `%LOCALAPPDATA%` and `%USERPROFILE%` | The folders come from vendor documentation and have not been tried on a Windows machine. No console window opens for the child |
| Linux | The same folders as macOS | Not tried on a Linux machine. Rootless Podman and Docker sockets are the tool's business; the crate only runs the tool |

In every case the tool must also answer `version` within five seconds. A tool that is found but does not answer makes the source unavailable, with the message `docker found at <path> but the engine did not answer`, which Settings shows. `engine: auto` uses Docker when it answers and Podman otherwise. Rules that name different engines run one `events` child per engine. The child is restarted with a 1, 2, 4 ... 60 second wait if it exits, and killed when the source stops.

## Permissions

None. The crate runs the container tool as the user, which needs the same access to the engine's socket that `docker ps` needs. It asks the OS for nothing and never reads container output or files.

## Example

```json
{ "id": "web-up", "kind": "docker.container-started", "when": { "name": "web-*", "engine": "auto" } }
```

## Develop

| File | What it holds |
|---|---|
| `src/lib.rs` | `KINDS`, `DockerSource` and `start`, which plans the engines, starts one supervisor each and sends fired rules to the sink. Glue only |
| `src/rule.rs` | The typed `When` and `parse`. Pure, unit tested |
| `src/logic.rs` | Pure: `parse_line` turns an engine line into an `EngineEvent`, `matches` tests a rule, `payload` and `fire` build what is sent. Unit tested with the lines in `src/logic/fixtures.rs` |
| `src/plan.rs` | Pure: which engine serves which rules, and the message when none can. Unit tested |
| `src/supervisor.rs` | Runs one engine child on a named thread, restarts it with backoff, kills it on stop. Behind the `EngineProcess` trait, so the tests use fake children (`src/supervisor/fakes.rs`) |
| `src/os.rs` | The only file that talks to the OS: `find_engine`, `availability()`, `probe` (the `version` check) and `RealEngine`, the `events` child |
| `tests/live.rs` | `#[ignore]` tests against a real Docker. Each prints `skipping:` and passes when no engine answers |
| `tests/readme.rs` | Fails when the Kinds table above is stale |

```
cd src-tauri
cargo test -p trigger-docker                                    # pure and readme tests, any OS, offline
cargo test -p trigger-docker -- --ignored --test-threads=1      # live tests: needs a running Docker, uses alpine:3
cargo clippy -p trigger-docker --all-targets -- -D warnings
```

The live tests start and remove containers and images named `tg-live-...`. They must run one at a time because one of them sets `DOCKER_HOST` for its process.

## Known limits

What does not work, on which platform, and why.

- `docker.image-built` fires on any `image tag` event, so a manual `docker tag` fires it too. Docker has no event that says a build ran; a tagged build always ends with a tag event, so this is the best signal. An untagged build fires nothing.
- Events are lost while the engine's `events` child is restarting, and for the moments before the first one attaches. The stream is read without `--since` on purpose, so a restart never replays old events.
- The Podman parsing is checked against lines made from Podman v5.6.0's own event struct, not lines captured from a running Podman. Podman 6 and remote Podman (`podman machine`) are not checked, and the live tests use Docker only. The Podman lines in the unit tests are synthetic.
- The tool is looked for once per run of the app. Installing Docker while the app is open is seen after a restart.
- Only the `PATH` of the app is used. A `DOCKER_HOST` or docker context set only in a shell startup file is not seen, so Colima or OrbStack on a non default socket can be reported as not answering.
- If any rule names an engine that is not running, the whole source is unavailable, not only that rule.
- Containers are not started and Kubernetes is not watched.
