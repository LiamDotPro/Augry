use super::fixtures::*;
use super::*;
use serde_json::json;
use trigger_core::testkit::rule;

const ID_R01C: &str = "ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe";

fn labels(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn r01c_labels() -> BTreeMap<String, String> {
    labels(&[
        ("com.example.k", "v"),
        ("com.example.team", "x"),
        ("desktop.docker.io/ports.scheme", "v2"),
    ])
}

#[test]
fn an_armed_rule_holds_its_parsed_when() {
    let r = rule(
        "p/t",
        "docker.container-started",
        json!({ "name": "web-*" }),
    );
    let parsed = crate::rule::parse(&r).unwrap();
    let armed = Armed {
        id: r.id.clone(),
        kind: r.kind.clone(),
        when: parsed,
    };
    assert_eq!(armed.kind, "docker.container-started");
    assert!(armed.when.name.is_some());
}

#[test]
fn every_event_kind_id_is_a_declared_kind() {
    for kind in [
        EventKind::Started,
        EventKind::Stopped,
        EventKind::Healthy,
        EventKind::Unhealthy,
        EventKind::ImageBuilt,
    ] {
        assert!(crate::KINDS.iter().any(|k| k.id == kind.id()), "{kind:?}");
    }
}

#[test]
fn docker_start() {
    let e = parse_line(DOCKER_START).unwrap();
    assert_eq!(
        e,
        EngineEvent {
            engine: Engine::Docker,
            kind: EventKind::Started,
            id: ID_R01C.into(),
            name: "r01c".into(),
            image: "r01test:1".into(),
            labels: r01c_labels(),
            exit_code: None,
            time_ms: 1_791_837_794_607,
        }
    );
}

#[test]
fn docker_die_carries_the_exit_code_and_keeps_it_out_of_the_labels() {
    let e = parse_line(DOCKER_DIE_137).unwrap();
    assert_eq!(e.kind, EventKind::Stopped);
    assert_eq!(e.exit_code, Some(137));
    assert_eq!(e.labels, r01c_labels());
    assert_eq!(e.time_ms, 1_791_837_807_764);

    let e = parse_line(DOCKER_DIE_3).unwrap();
    assert_eq!(e.kind, EventKind::Stopped);
    assert_eq!(e.exit_code, Some(3));
    assert_eq!(e.name, "r01x");
    assert_eq!(e.image, "alpine:3");
    assert_eq!(
        e.labels,
        labels(&[("desktop.docker.io/ports.scheme", "v2")])
    );
}

#[test]
fn docker_health_verdicts() {
    let healthy = parse_line(DOCKER_HEALTHY).unwrap();
    assert_eq!(healthy.kind, EventKind::Healthy);
    assert_eq!(healthy.name, "r01c");
    let unhealthy = parse_line(DOCKER_UNHEALTHY).unwrap();
    assert_eq!(unhealthy.kind, EventKind::Unhealthy);
    assert_eq!(unhealthy.exit_code, None);
}

#[test]
fn a_health_verdict_without_the_space_still_counts() {
    let line = DOCKER_HEALTHY.replace("health_status: healthy", "health_status:healthy");
    assert_eq!(parse_line(&line).unwrap().kind, EventKind::Healthy);
}

#[test]
fn the_health_verdict_starting_is_ignored() {
    assert_eq!(parse_line(DOCKER_HEALTH_STARTING), None);
}

#[test]
fn an_exit_code_printed_as_a_number_is_read_too() {
    let line = DOCKER_DIE_3.replace(r#""exitCode":"3""#, r#""exitCode":3"#);
    assert_eq!(parse_line(&line).unwrap().exit_code, Some(3));
}

#[test]
fn the_time_falls_back_to_seconds_without_timenano() {
    let line = DOCKER_START.replace(r#","timeNano":1791837794607003258"#, "");
    assert_eq!(parse_line(&line).unwrap().time_ms, 1_791_837_794_000);
    let neither = line.replace(r#","time":1791837794"#, "");
    assert_eq!(parse_line(&neither).unwrap().time_ms, 0);
}

#[test]
fn lines_that_are_not_events_give_none() {
    for line in [
        "",
        "   ",
        "not json",
        "[]",
        "42",
        "null",
        "{}",
        r#"{"Type":"container"}"#,
        r#"{"Type":"container","Action":"start"}"#,
        r#"{"Type":"container","Action":"start","Actor":"x"}"#,
        r#"{"Type":"container","Action":7,"Actor":{"ID":"a"}}"#,
    ] {
        assert_eq!(parse_line(line), None, "{line}");
    }
}

#[test]
fn a_line_with_a_trailing_newline_parses() {
    let line = format!("{DOCKER_START}\r\n");
    assert!(parse_line(&line).is_some());
}
