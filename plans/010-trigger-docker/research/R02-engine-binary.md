---
id: R02
epic: 010-trigger-docker
question: How the app finds the engine binary when launched from the Dock or Start menu, where PATH is minimal (the app already solves this for agents in `agent/installed.rs`)
status: answered
---

## Answer

The app builds a search list from its own PATH, then the login shell PATH (`$SHELL -ilc /usr/bin/env`, 5 s timeout, unix only), then a hard coded list of usual dirs, and checks each for a file. The crate should copy that pattern (it cannot import it) and add engine specific dirs. Check the engine with `<bin> version --format '{{.Server.Version}}'` under a timeout (exit 0 and non empty stdout means up). `docker events` without `--since` does not replay history (verified), so omit it. Kill with `child.kill()` then `child.wait()`.

## Evidence

Repo, in `/Users/liamread/WebstormProjects/Terminal-Grid-010`:
- `src-tauri/src/agent/installed.rs:16-27` `find(names, dirs, exts)` checks `dir.join(name+ext).is_file()`. `:29-40` PATHEXT on Windows, `""` elsewhere.
- `:51-85` `login_shell_path()`: runs `$SHELL -ilc /usr/bin/env` (default `/bin/bash`), stdin null, stderr null, reads stdout on a thread, `recv_timeout(5s)`, then `child.kill()` and `child.wait()`. `:44-49` `path_from_env_output` takes the last `PATH=` line, ignoring startup noise.
- `:87-107` `usual_dirs()` (unix only): `/opt/homebrew/bin`, `/usr/local/bin`, and home subdirs.
- `:109-123` `search_dirs()`: own PATH, then login PATH, then usual dirs, deduplicated. Windows uses own PATH only (GUI apps on Windows inherit the full user PATH).
- Windows no window flag: `src-tauri/src/git/cli.rs:13-14` `CREATE_NO_WINDOW = 0x0800_0000` (cfg windows), applied at `:63` via `creation_flags`. Same in `pty/shell.rs:160` and `triggers/variables.rs:161-170`. A trigger crate must define its own constant and use `std::os::windows::process::CommandExt`.
- Timeout pattern for a child: `git/cli.rs:11-12` (3 s timeout, 10 ms poll with `try_wait`).

Ran on this machine (macOS, Docker Desktop, 2026-10-12):
- `which docker` gives `/usr/local/bin/docker`; `docker version --format '{{.Server.Version}}'` prints `29.4.3`, exit 0.
- With `DOCKER_HOST=unix:///nope` the same command prints an error to stderr, empty stdout, exit 1. So "binary exists, engine down" is distinguishable.
- `docker events --format ...` for 3 s with no `--since`: 0 lines. With `--since 1h`: 259 lines replayed. Replay only happens with `--since`. `docker events --help` lists `--since` and `--until`.
- `~/.orbstack/bin`, `~/.rd/bin`, `/opt/podman/bin` do not exist here, so those paths come from vendor docs and memory, not verified.
- No Podman installed here; Podman behaviour was not run.

## Implications for the cards

Helper `find_engine(engine) -> Option<PathBuf>` in the crate, names `docker` or `podman` (`.exe` on Windows):
1. Search dirs in order: process `PATH`; on unix the login shell PATH (copy `login_shell_path` and `path_from_env_output` as is, with the 5 s timeout and kill plus wait); then the candidates below; dedupe; first `is_file()` wins.
2. Candidates unix: `/usr/local/bin`, `/opt/homebrew/bin`, `/opt/podman/bin`, `/usr/bin`, `~/.orbstack/bin`, `~/.rd/bin`, `~/.docker/bin`, `/Applications/Docker.app/Contents/Resources/bin`, `/snap/bin`. Colima has no CLI of its own for docker; it uses the Homebrew docker CLI, already covered.
3. Candidates Windows: `%ProgramFiles%\Docker\Docker\resources\bin`, `%ProgramFiles%\RedHat\Podman`, `%ProgramFiles%\Podman`, `%LOCALAPPDATA%\Programs\Podman`, `%USERPROFILE%\.rd\bin`. Read the env vars, do not hard code `C:`.
4. `engine = auto`: try `docker` first, then `podman`, pick the first whose version check passes.
5. Availability: run `<bin> version --format '{{.Server.Version}}'`, stdin null, stdout piped, stderr null. Poll `try_wait` every 10 ms up to 5 s; on timeout `kill` then `wait`. Up means exit 0 and trimmed stdout non empty. Otherwise `Unsupported` with a reason such as "docker not found" or "docker found at X but the engine did not answer".
6. Windows: `.creation_flags(0x0800_0000)` under `#[cfg(windows)]` on every Command (version check and events).
7. Spawn the events child by resolved absolute path. Args: `events --format '{{json .}}'`. Do not pass `--since`. After a restart with backoff, events during the gap are lost unless you pass `--since <last event unix time>` and dedupe; decide in the card.
8. Kill on drop: `impl Drop` and the stop method call `child.kill()` then `child.wait()` (SIGKILL on unix, TerminateProcess on Windows, both std). Ignore kill errors (already exited). Read stdout on a dedicated thread; killing closes the pipe so `read_line` returns 0 and the thread ends. Keep the `Child` owned by the supervisor, not the reader thread. Closing stdin does not stop `docker events`.
9. Test the kill with a fake long running process (`sleep 1000`, or `ping -t localhost` on Windows) and assert `try_wait` shows it exited after drop.

## Doubts

- Orbstack, Rancher, Podman and Windows paths are unverified on a machine. They are a fallback after PATH and the login shell, so a wrong entry is harmless.
- Podman `version --format '{{.Server.Version}}'` and "no replay without --since" were not run. With a stopped Podman machine `podman version` should fail, which gives the wanted Unsupported.
- `DOCKER_HOST` or docker contexts set only in the user's shell rc are not in the app's env (only PATH is taken from the login shell). Colima users on a non default socket may be reported as engine down. Passing the whole login env to the child would fix it; not decided.
