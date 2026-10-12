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
| `example` | Replace with the real fields |

## Platforms

| Platform | How it is detected | Limits |
|---|---|---|
| macOS | | |
| Windows | | |
| Linux | | |

## Permissions

What the OS asks the user for, and when. "None" is a fine answer.

## Example

```json
{ "id": "web-up", "kind": "docker.container-started", "when": { "name": "web-*", "engine": "auto" } }
```

## Develop

| File | What it holds |
|---|---|
| `src/lib.rs` | `KINDS`, `DockerSource` and the thread that joins the OS layer to the logic. Glue only |
| `src/rule.rs` | The typed `When` and `parse`. Pure, unit tested |
| `src/logic.rs` | Pure: which rules a `Signal` fires, with what payload, and any debounce state. Unit tested |
| `src/os.rs` | The only file that talks to the OS: `availability()` and `listen()`, per platform behind `cfg` |
| `tests/live.rs` | `#[ignore]` tests against the real OS |
| `tests/readme.rs` | Fails when the Kinds table above is stale |

```
cd src-tauri
cargo test -p trigger-docker                       # pure and readme tests, any OS, offline
cargo test -p trigger-docker -- --ignored          # live tests on this machine
cargo clippy -p trigger-docker --all-targets -- -D warnings
```

## Known limits

What does not work, on which platform, and why.

- `docker.image-built` fires on any `image tag` event, so a manual `docker tag` fires it too. Docker has no event that says a build ran; a tagged build always ends with a tag event, so this is the best signal. An untagged build fires nothing.

- The Podman parsing is checked against lines made from Podman v5.6.0's own event struct, not lines captured from a running Podman. Podman 6 and remote Podman (`podman machine`) are not checked.
