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
