---
id: T25
epic: 010-trigger-docker
title: Add live tests for started and stopped containers
status: done
depends_on: [T24]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-docker/tests/live.rs
---

## Goal

`tests/live.rs` has the live test helpers and the first two live tests against a real Docker: a container that exits with code 3 fires started then stopped with `exit_code` 3, and `docker stop` fires stopped exactly once with 137. Every test prints a message and returns when no engine answers. This machine has Docker Desktop running, so the tests really run here.

## Read first

- `plans/_context/conventions.md` the line about live tests (`#[ignore]`, a comment on how to run, temp files cleaned up)
- `src-tauri/crates/trigger-fs/tests/live.rs` the reference live test (read only)
- `src-tauri/crates/trigger-docker/tests/live.rs` the template test this card replaces
- `src-tauri/crates/trigger-core/src/testkit.rs:9-60` `RecordingSink` and `wait_for`
- `plans/010-trigger-docker/research/R01-events-json.md` the `docker stop` sequence (`kill`, `stop`, `die` with `exitCode` 137)

## Steps

1. Check an engine is up: `docker version --format '{{.Server.Version}}'` prints a version. If not, see If stuck.

2. Replace the whole of `src-tauri/crates/trigger-docker/tests/live.rs` with this. How it works: `arm` starts the source with the test's rules plus a warm-up rule, then tags `alpine:3` with a new tag every half second until the source reports one. The events stream has no history, so without the warm-up an event made right after `start` could be lost. `Cleanup` removes containers and tags when the test ends, even when it panics. `wait_rule` skips events of other rules. There is no `uuid` dependency: `unique` uses the process id and the clock.

```rust
//! Tests against a real container engine. Not run by `cargo test`; run them on
//! a machine with Docker running:
//!
//!     cargo test -p trigger-docker -- --ignored --test-threads=1
//!
//! Each test arms rules, makes the engine do the real thing with the `docker`
//! tool, and waits for the event with `RecordingSink`. A test that finds no
//! engine answering prints why and returns, so it passes on a machine without
//! Docker. They use `alpine:3` and remove every container and image they make.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::json;
use trigger_core::testkit::{rule, RecordingSink};
use trigger_core::{Event, Handle, Rule, Source};
use trigger_docker::DockerSource;

const WAIT: Duration = Duration::from_secs(20);

/// Runs `docker` with these arguments. Gives its stdout when it succeeded.
fn docker(args: &[&str]) -> Option<String> {
    let out = Command::new("docker")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// True when an engine answers and `alpine:3` is there or can be pulled.
/// Prints the reason when not.
fn engine_ready() -> bool {
    if docker(&["version", "--format", "{{.Server.Version}}"]).is_none() {
        eprintln!(
            "skipping: no Docker engine answered `docker version`. Start Docker to run this test."
        );
        return false;
    }
    if docker(&["image", "inspect", "alpine:3"]).is_none()
        && docker(&["pull", "-q", "alpine:3"]).is_none()
    {
        eprintln!("skipping: alpine:3 is not here and could not be pulled");
        return false;
    }
    true
}

/// A name no other run uses, safe for containers and image tags.
fn unique(label: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        "tg-live-{label}-{}-{}",
        std::process::id(),
        nanos % 1_000_000_000
    )
}

/// Removes what a test made, even when the test panics.
#[derive(Default)]
struct Cleanup {
    containers: Vec<String>,
    images: Vec<String>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        for name in &self.containers {
            let _ = docker(&["rm", "-f", name]);
        }
        for tag in &self.images {
            let _ = docker(&["rmi", "-f", tag]);
        }
    }
}

/// The first event of this kind for this rule id, or `None` after `timeout`.
/// Events of other rules are skipped.
fn wait_rule_for(
    sink: &RecordingSink,
    kind: &str,
    rule_id: &str,
    timeout: Duration,
) -> Option<Event> {
    let deadline = Instant::now() + timeout;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match sink.wait_for(kind, left) {
            Some(event) if event.rule == rule_id => return Some(event),
            Some(_) => {}
            None => return None,
        }
    }
}

fn wait_rule(sink: &RecordingSink, kind: &str, rule_id: &str) -> Event {
    wait_rule_for(sink, kind, rule_id, WAIT)
        .unwrap_or_else(|| panic!("no {kind} event for rule {rule_id} within {WAIT:?}"))
}

/// Starts the source with these rules and a warm-up rule, then waits until the
/// event stream is attached. The stream has no history, so anything that
/// happens before it attaches is lost. The warm-up tags an image until the
/// source reports it.
fn arm(mut rules: Vec<Rule>, cleanup: &mut Cleanup) -> (RecordingSink, Handle) {
    let warm = unique("warm");
    rules.push(rule(
        "live/warm",
        "docker.image-built",
        json!({ "name": format!("{warm}:*") }),
    ));
    let sink = RecordingSink::new();
    let handle = DockerSource.start(rules, sink.tx.clone()).expect("start");
    for attempt in 0..30 {
        let tag = format!("{warm}:{attempt}");
        cleanup.images.push(tag.clone());
        docker(&["tag", "alpine:3", &tag]).expect("docker tag");
        let seen = wait_rule_for(
            &sink,
            "docker.image-built",
            "live/warm",
            Duration::from_millis(500),
        );
        if seen.is_some() {
            return (sink, handle);
        }
    }
    panic!("the event stream did not attach within 15 seconds");
}

#[test]
#[ignore]
fn a_container_that_exits_fires_started_then_stopped_with_its_exit_code() {
    if !engine_ready() {
        return;
    }
    let mut cleanup = Cleanup::default();
    let name = unique("exit");
    cleanup.containers.push(name.clone());
    let (sink, _handle) = arm(
        vec![
            rule(
                "live/started",
                "docker.container-started",
                json!({ "name": name }),
            ),
            rule(
                "live/stopped",
                "docker.container-stopped",
                json!({ "name": name }),
            ),
        ],
        &mut cleanup,
    );

    docker(&[
        "run", "-d", "--name", &name, "alpine:3", "sh", "-c", "exit 3",
    ])
    .expect("docker run");

    let started = wait_rule(&sink, "docker.container-started", "live/started");
    assert_eq!(started.payload["engine"], json!("docker"));
    assert_eq!(started.payload["name"], json!(name));
    assert_eq!(started.payload["image"], json!("alpine:3"));
    let id = started.payload["container_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(id.len(), 12);

    let stopped = wait_rule(&sink, "docker.container-stopped", "live/stopped");
    assert_eq!(stopped.payload["exit_code"], json!(3));
    assert_eq!(stopped.payload["container_id"], json!(id));
}

#[test]
#[ignore]
fn docker_stop_fires_stopped_once() {
    if !engine_ready() {
        return;
    }
    let mut cleanup = Cleanup::default();
    let name = unique("stop");
    cleanup.containers.push(name.clone());
    let (sink, _handle) = arm(
        vec![
            rule(
                "live/started",
                "docker.container-started",
                json!({ "name": name }),
            ),
            rule(
                "live/stopped",
                "docker.container-stopped",
                json!({ "name": name }),
            ),
        ],
        &mut cleanup,
    );

    docker(&["run", "-d", "--name", &name, "alpine:3", "sleep", "300"]).expect("docker run");
    wait_rule(&sink, "docker.container-started", "live/started");
    docker(&["stop", "-t", "1", &name]).expect("docker stop");

    let stopped = wait_rule(&sink, "docker.container-stopped", "live/stopped");
    // PID 1 ignores SIGTERM, so the engine ends it with SIGKILL after 1 second.
    assert_eq!(stopped.payload["exit_code"], json!(137));
    // `docker stop` also prints `stop` and `kill` events. They must not fire.
    let again = wait_rule_for(
        &sink,
        "docker.container-stopped",
        "live/stopped",
        Duration::from_secs(2),
    );
    assert!(again.is_none(), "stopped fired twice");
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-docker`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker --test live -- --ignored --test-threads=1` passes: `2 passed`
- [ ] `cd src-tauri && DOCKER_HOST=unix:///nonexistent/x.sock cargo test -p trigger-docker --test live -- --ignored --test-threads=1 --nocapture` passes and prints `skipping:` twice
- [ ] `docker ps -a --filter name=tg-live --format '{{{{.Names}}}}'` prints nothing, and `docker images --format '{{{{.Repository}}}}' | grep -c tg-live` prints `0`
- [ ] `cd src-tauri && cargo test -p trigger-docker` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/010-trigger-docker/tasks/`
- [ ] `ps aux | grep "docker events" | grep -v grep` prints nothing (the handle's drop killed the child)

## Out of scope

- The healthy, unhealthy and image-built tests, and the test of a silent engine (T26).
- Making the tests run in `cargo test` without `--ignored`. They stay `#[ignore]`.
- Adding dependencies (`uuid`, `tempfile`) or a `[dev-dependencies]` block.
- Podman. The tests use `docker` only; Podman lines stay synthetic (README Known limits).
- Changing timeouts to make a flaky run pass. If a test times out, see If stuck.
- The README. Internal code only; the card that writes the README is T27, which describes this work.

## If stuck

- If `docker version` does not print a version, Docker is not running. Start Docker Desktop (on macOS: `open -a Docker`), wait for `docker version` to work, and try again. If it still does not work, set status blocked and say so: this card requires a running engine.
- If `alpine:3` is not on the machine and `docker pull alpine:3` fails, set status blocked.
- If a test fails with `the event stream did not attach within 15 seconds`, run `docker events --format '{{json .}}' --filter type=image` in another terminal while running `docker tag alpine:3 tg-live-check:1`. If the terminal shows no line, set status blocked: the engine does not emit tag events. If it does show a line, set status blocked and paste the first line of `cargo test` output.
- If a leftover container or image named `tg-live-...` remains after a failed run, remove it with `docker rm -f <name>` or `docker rmi -f <tag>`.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
