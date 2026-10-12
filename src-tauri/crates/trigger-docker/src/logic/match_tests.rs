use super::fixtures::*;
use super::*;
use serde_json::json;
use trigger_core::testkit::rule;

fn armed(kind: &str, when: serde_json::Value) -> Armed {
    let r = rule("p/t", kind, when);
    let parsed = crate::rule::parse(&r).unwrap();
    Armed {
        id: r.id,
        kind: r.kind,
        when: parsed,
    }
}

fn started() -> EngineEvent {
    parse_line(DOCKER_START).unwrap()
}

#[test]
fn an_empty_when_matches_every_event_of_its_kind_only() {
    let rule = armed("docker.container-started", json!({}));
    assert!(matches(&rule, &started()));
    assert!(!matches(&rule, &parse_line(DOCKER_DIE_3).unwrap()));
}

#[test]
fn the_name_glob_tests_the_container_name() {
    let hit = armed("docker.container-started", json!({ "name": "r01*" }));
    let miss = armed("docker.container-started", json!({ "name": "web-*" }));
    assert!(matches(&hit, &started()));
    assert!(!matches(&miss, &started()));
}

#[test]
fn the_label_must_match_exactly() {
    let hit = armed(
        "docker.container-started",
        json!({ "label": "com.example.team=x" }),
    );
    assert!(matches(&hit, &started()));
    for label in [
        "com.example.team=y",
        "com.example.other=x",
        "com.example.team=",
    ] {
        let miss = armed("docker.container-started", json!({ "label": label }));
        assert!(!matches(&miss, &started()), "{label}");
    }
}

#[test]
fn the_engine_field_is_tested_against_the_engine_that_printed_the_event() {
    let podman_only = armed("docker.container-started", json!({ "engine": "podman" }));
    let docker_only = armed("docker.container-started", json!({ "engine": "docker" }));
    let auto = armed("docker.container-started", json!({ "engine": "auto" }));
    let from_podman = parse_line(PODMAN_START).unwrap();
    assert!(matches(&podman_only, &from_podman));
    assert!(!matches(&podman_only, &started()));
    assert!(matches(&docker_only, &started()));
    assert!(!matches(&docker_only, &from_podman));
    assert!(matches(&auto, &started()) && matches(&auto, &from_podman));
}

#[test]
fn every_set_field_must_match() {
    let rule = armed(
        "docker.container-started",
        json!({ "name": "r01c", "image": "r01test:1", "label": "com.example.k=v" }),
    );
    assert!(matches(&rule, &started()));
    let one_wrong = armed(
        "docker.container-started",
        json!({ "name": "r01c", "image": "r01test:2", "label": "com.example.k=v" }),
    );
    assert!(!matches(&one_wrong, &started()));
}

#[test]
fn image_globs_ignore_docker_io_library_and_latest() {
    let cases = [
        // (glob, image the engine printed, should match)
        ("alpine", "docker.io/library/alpine:latest", true),
        ("alpine", "docker.io/library/alpine", true),
        ("alpine:3", "docker.io/library/alpine:3", true),
        ("alpine", "alpine:3", false),
        ("alpine:latest", "alpine", true),
        ("alpine:latest", "docker.io/library/alpine", true),
        ("alpine*", "docker.io/library/alpine:3", true),
        ("nginx:1.27", "nginx:1.27", true),
        ("nginx:1.27", "nginx:1.28", false),
        ("localhost:5000/app", "localhost:5000/app", true),
        ("localhost:5000/app:latest", "localhost:5000/app", true),
        ("myorg/app", "myorg/app:latest", true),
        // Only docker.io/library/ is dropped, not other docker.io paths.
        ("myorg/app", "docker.io/myorg/app:latest", false),
        ("app", "app@sha256:abc", false),
    ];
    for (glob, image, expected) in cases {
        let rule = armed("docker.container-started", json!({ "image": glob }));
        let mut event = started();
        event.image = image.to_string();
        assert_eq!(matches(&rule, &event), expected, "{glob} vs {image}");
    }
}

#[test]
fn for_image_built_both_name_and_image_test_the_reference() {
    let event = parse_line(DOCKER_IMAGE_TAG).unwrap();
    let by_name = armed("docker.image-built", json!({ "name": "r01test:*" }));
    let by_image = armed("docker.image-built", json!({ "image": "r01test:1" }));
    let wrong_kind = armed("docker.container-started", json!({}));
    assert!(matches(&by_name, &event));
    assert!(matches(&by_image, &event));
    assert!(!matches(&wrong_kind, &event));
}

#[test]
fn health_kinds_are_told_apart() {
    let healthy = armed("docker.container-healthy", json!({}));
    let unhealthy = armed("docker.container-unhealthy", json!({}));
    let event = parse_line(DOCKER_HEALTHY).unwrap();
    assert!(matches(&healthy, &event));
    assert!(!matches(&unhealthy, &event));
}
