---
id: T26
epic: 010-trigger-docker
title: Add live tests for health, image build and a silent engine
status: todo
depends_on: [T25]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-docker/tests/live.rs
---

## Goal

`tests/live.rs` has the rest of the live tests: a passing and a failing health check fire healthy and unhealthy, a real `docker build -t` fires image built, and an engine that does not answer makes `start` fail with the message "docker found at <path> but the engine did not answer". The whole live suite passes here with Docker running.

## Read first

- `src-tauri/crates/trigger-docker/tests/live.rs` the file as T25 left it: `docker`, `engine_ready`, `unique`, `Cleanup`, `arm`, `wait_rule`
- `plans/010-trigger-docker/epic.md` Decision 8 (the message for a silent engine)
- `plans/010-trigger-docker/research/R01-events-json.md` the health and image build lines

## Steps

1. Check an engine is up: `docker version --format '{{.Server.Version}}'` prints a version. If not, see If stuck.

2. Append this to the end of `src-tauri/crates/trigger-docker/tests/live.rs`, after a blank line. The health tests use `docker run --health-cmd`, which is the same as a Dockerfile `HEALTHCHECK`, so no image build is needed. The build test pipes a two line Dockerfile on stdin, so there is no build folder. The last test points the docker tool at a socket nobody listens on by setting `DOCKER_HOST` for the whole process, which is why the live tests must run with `--test-threads=1`; it puts the old value back.

```rust
#[test]
#[ignore]
fn a_passing_health_check_fires_healthy() {
    if !engine_ready() {
        return;
    }
    let mut cleanup = Cleanup::default();
    let name = unique("healthy");
    cleanup.containers.push(name.clone());
    let (sink, _handle) = arm(
        vec![rule(
            "live/healthy",
            "docker.container-healthy",
            json!({ "name": name }),
        )],
        &mut cleanup,
    );

    docker(&[
        "run",
        "-d",
        "--name",
        &name,
        "--health-cmd",
        "true",
        "--health-interval",
        "1s",
        "--health-retries",
        "1",
        "alpine:3",
        "sleep",
        "300",
    ])
    .expect("docker run");

    let healthy = wait_rule(&sink, "docker.container-healthy", "live/healthy");
    assert_eq!(healthy.payload["health"], json!("healthy"));
    assert_eq!(healthy.payload["name"], json!(name));
}

#[test]
#[ignore]
fn a_failing_health_check_fires_unhealthy() {
    if !engine_ready() {
        return;
    }
    let mut cleanup = Cleanup::default();
    let name = unique("unhealthy");
    cleanup.containers.push(name.clone());
    let (sink, _handle) = arm(
        vec![rule(
            "live/unhealthy",
            "docker.container-unhealthy",
            json!({ "name": name }),
        )],
        &mut cleanup,
    );

    docker(&[
        "run",
        "-d",
        "--name",
        &name,
        "--health-cmd",
        "false",
        "--health-interval",
        "1s",
        "--health-retries",
        "1",
        "alpine:3",
        "sleep",
        "300",
    ])
    .expect("docker run");

    let unhealthy = wait_rule(&sink, "docker.container-unhealthy", "live/unhealthy");
    assert_eq!(unhealthy.payload["health"], json!("unhealthy"));
}

#[test]
#[ignore]
fn a_docker_build_fires_image_built() {
    if !engine_ready() {
        return;
    }
    let mut cleanup = Cleanup::default();
    let tag = format!("{}:1", unique("built"));
    cleanup.images.push(tag.clone());
    let (sink, _handle) = arm(
        vec![rule(
            "live/built",
            "docker.image-built",
            json!({ "name": tag }),
        )],
        &mut cleanup,
    );

    // The Dockerfile comes from stdin, so there is no build folder to clean up.
    let mut build = Command::new("docker")
        .args(["build", "-q", "-t", &tag, "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("docker build");
    {
        use std::io::Write;
        let mut stdin = build.stdin.take().unwrap();
        stdin
            .write_all(b"FROM alpine:3\nLABEL tg.live=1\n")
            .unwrap();
    }
    assert!(build.wait().unwrap().success(), "docker build failed");

    let built = wait_rule(&sink, "docker.image-built", "live/built");
    assert_eq!(built.payload["name"], json!(tag));
    assert_eq!(built.payload["engine"], json!("docker"));
    assert!(!built.payload.contains_key("container_id"));
}

#[test]
#[ignore]
fn an_engine_that_does_not_answer_is_reported_by_start() {
    if docker(&["--version"]).is_none() {
        eprintln!("skipping: the docker tool was not found");
        return;
    }
    // Point the tool at a socket nobody listens on. This changes the whole
    // process, which is why the live tests run with `--test-threads=1`.
    let before = std::env::var_os("DOCKER_HOST");
    std::env::set_var("DOCKER_HOST", "unix:///nonexistent/trigger-docker.sock");
    let sink = RecordingSink::new();
    let result = DockerSource.start(
        vec![rule(
            "live/any",
            "docker.container-started",
            json!({ "engine": "docker" }),
        )],
        sink.tx.clone(),
    );
    match before {
        Some(value) => std::env::set_var("DOCKER_HOST", value),
        None => std::env::remove_var("DOCKER_HOST"),
    }
    let message = result.err().expect("start should fail").to_string();
    assert!(
        message.starts_with("docker found at ")
            && message.ends_with("but the engine did not answer"),
        "unexpected message: {message}"
    );
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-docker`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker --test live -- --ignored --test-threads=1` passes: `6 passed`
- [ ] `cd src-tauri && cargo test -p trigger-docker --test live -- --ignored --test-threads=1` passes a second time straight after (no leftovers break it)
- [ ] `cd src-tauri && DOCKER_HOST=unix:///nonexistent/x.sock cargo test -p trigger-docker --test live -- --ignored --test-threads=1 --nocapture` passes and prints `skipping:` for the five engine tests
- [ ] `cd src-tauri && cargo test -p trigger-docker` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/010-trigger-docker/tasks/`
- [ ] `docker ps -a --filter name=tg-live --format '{{.Names}}'` prints nothing, and `docker images --format '{{.Repository}}' | grep -c tg-live` prints `0`

## Out of scope

- Changing the helpers from T25, or the two tests it added.
- A Podman live test, or a test that installs or stops Docker.
- Raising `WAIT` or the warm-up count to make a slow run pass.
- Adding dependencies.
- The README. Internal code only; the card that writes the README is T27, which describes this work.

## If stuck

- If `docker version` does not print a version, start Docker Desktop (on macOS: `open -a Docker`), wait for `docker version` to work and try again. If it still fails, set status blocked and say so: this card requires a running engine.
- If `an_engine_that_does_not_answer_is_reported_by_start` fails with a message that starts with `docker was not found`, the docker tool is not on `PATH` for the test. Set status blocked.
- If only a health test times out, run `docker run -d --name tg-live-check --health-cmd true --health-interval 1s --health-retries 1 alpine:3 sleep 300`, then `docker inspect --format '{{.State.Health.Status}}' tg-live-check` after three seconds. If it does not print `healthy`, set status blocked. Remove the container with `docker rm -f tg-live-check` either way.
- If a leftover `tg-live-...` container or image remains after a failed run, remove it with `docker rm -f <name>` or `docker rmi -f <tag>`.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
