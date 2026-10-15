---
id: T27
epic: 010-trigger-docker
title: Write the README and close slice C
status: done
depends_on: [T26]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-docker/README.md
---

## Goal

`README.md` is complete: all eight sections are filled in (Kinds and Rule fields and Payload were written by earlier cards, this card writes Platforms, Permissions, Example, Develop and Known limits), with Permissions "none" and the known limits stated. The crate then passes every check with no template warning, which closes slice C.

## Read first

- `src-tauri/crates/trigger-docker/README.md` the file as the earlier cards left it
- `plans/_templates/trigger-crate/README.md` the template and its eight sections
- `plans/010-trigger-docker/epic.md` Decisions 4, 7, 8 and 9
- `src-tauri/crates/trigger-docker/src/os.rs:19-70` the folder lists the Platforms table repeats

## Steps

1. In `src-tauri/crates/trigger-docker/README.md` find the line `## Platforms`. Delete it and every line after it (through the end of the file) and put the text below in its place. Everything above `## Platforms` (the title, Kinds with its markers, Rule fields, Payload) stays exactly as it is. The Known limits section keeps the two bullets earlier cards wrote (the `docker tag` one and the Podman one, which now also mentions the live tests) and adds the others.

````markdown
## Platforms

| Platform | How it is detected | Limits |
|---|---|---|
| macOS | The `docker` or `podman` tool is looked for on the app's `PATH`, then in `/usr/local/bin`, `/opt/homebrew/bin`, `/opt/podman/bin`, `/usr/bin`, `/Applications/Docker.app/Contents/Resources/bin`, `~/.orbstack/bin`, `~/.rd/bin` and `~/.docker/bin`. Docker Desktop, Colima, OrbStack and Rancher Desktop (with dockerd) all use the Docker tool. Checked live with Docker Desktop | An app started from the Dock has a short `PATH`, so a tool outside those folders is not found |
| Windows | The same, with `docker.exe` and `podman.exe`, on `PATH` then in the Docker Desktop, Podman and Rancher Desktop folders under `%ProgramFiles%`, `%LOCALAPPDATA%` and `%USERPROFILE%` | The folders come from vendor documentation and have not been tried on a Windows machine. No console window opens for the child |
| Linux | The same as macOS, plus `/snap/bin` | Not tried on a Linux machine. Rootless Podman and Docker sockets are the tool's business; the crate only runs the tool |

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
````

2. Check the Kinds table is still current: `cd src-tauri && cargo run -p trigger-docker --example readme` prints a table. It must be identical to the text between `<!-- kinds -->` and `<!-- /kinds -->`. If it is not, replace the text between the markers with the printed table.

3. Run the acceptance commands. The live tests are not required here (T26 did them).

## Acceptance

- [ ] `grep -c "^## " src-tauri/crates/trigger-docker/README.md` prints `8`
- [ ] `grep -c "Replace with\|not implemented yet\|Template: remove" src-tauri/crates/trigger-docker/README.md src-tauri/crates/trigger-docker/src/lib.rs src-tauri/crates/trigger-docker/src/os.rs` prints `0` after each of the three file names (grep exits 1 for a count of 0; that is expected)
- [ ] `cd src-tauri && cargo test -p trigger-docker --test readme` passes
- [ ] `cd src-tauri && cargo test -p trigger-docker` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/010-trigger-docker/tasks/`
- [ ] `node plans/_tools/check-crates.mjs 2>&1 | grep -c "trigger-docker"` prints `0` (grep exits 1 for a count of 0; that is expected): no warning or error mentions trigger-docker (no template text, no file over 400 lines)
- [ ] `cd src-tauri && cargo test --workspace` passes

## Out of scope

- Changing any `.rs` file. If a check fails because of code, see If stuck.
- Adding rows to the Kinds table or fields to the Payload table.
- The repo map (`plans/_context/repo-map.md`), CI and the app's `Cargo.toml`: slice D.
- Rewording the sections that earlier cards wrote.

## If stuck

- If `check-crates.mjs` prints a warning for a file over 400 lines (for example `src/logic.rs`), set status blocked and name the file. Do not split it here.
- If `check-crates.mjs` still warns `README.md still has template text`, search the README for `Replace with` and `not implemented yet` and delete only that text.
- If the `readme` example output differs from the table in the README, paste the printed table between the markers as step 2 says. Do not edit `KINDS`.
- If `cargo test --workspace` fails in a crate other than trigger-docker, set status blocked and name the crate.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
