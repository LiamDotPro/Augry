use std::cell::Cell;

use serde_json::json;
use trigger_core::testkit::rule;

use super::*;

fn armed(id: &str, engine: &str) -> Armed {
    let r = rule(id, "docker.container-started", json!({ "engine": engine }));
    let when = crate::rule::parse(&r).unwrap();
    Armed {
        id: r.id,
        kind: r.kind,
        when,
    }
}

fn ready_at(path: &str) -> Probe {
    Probe::Ready(PathBuf::from(path))
}

fn silent_at(path: &str) -> Probe {
    Probe::Silent(PathBuf::from(path))
}

fn ids(group: &Group) -> Vec<&str> {
    group.rules.iter().map(|r| r.id.as_str()).collect()
}

#[test]
fn an_explicit_engine_gets_its_own_group() {
    let probe = |e: Engine| match e {
        Engine::Docker => ready_at("/bin/docker"),
        Engine::Podman => Probe::Missing,
    };
    let groups = plan(vec![armed("p/a", "docker")], &probe).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].engine, Engine::Docker);
    assert_eq!(groups[0].path, PathBuf::from("/bin/docker"));
    assert_eq!(ids(&groups[0]), vec!["p/a"]);
}

#[test]
fn auto_takes_docker_and_never_asks_podman() {
    let asked = Cell::new(0);
    let probe = |e: Engine| {
        asked.set(asked.get() + 1);
        match e {
            Engine::Docker => ready_at("/bin/docker"),
            Engine::Podman => ready_at("/bin/podman"),
        }
    };
    let groups = plan(vec![armed("p/a", "auto")], &probe).unwrap();
    assert_eq!(groups[0].engine, Engine::Docker);
    assert_eq!(asked.get(), 1);
}

#[test]
fn auto_falls_back_to_podman() {
    let probe = |e: Engine| match e {
        Engine::Docker => silent_at("/bin/docker"),
        Engine::Podman => ready_at("/bin/podman"),
    };
    let groups = plan(vec![armed("p/a", "auto")], &probe).unwrap();
    assert_eq!(groups[0].engine, Engine::Podman);
    assert_eq!(groups[0].path, PathBuf::from("/bin/podman"));
}

#[test]
fn auto_with_no_engine_says_neither_was_found() {
    let probe = |_: Engine| Probe::Missing;
    let err = plan(vec![armed("p/a", "auto")], &probe).unwrap_err();
    assert_eq!(err, "docker and podman were not found");
}

#[test]
fn a_silent_engine_is_reported_with_its_path() {
    let probe = |e: Engine| match e {
        Engine::Docker => silent_at("/usr/local/bin/docker"),
        Engine::Podman => Probe::Missing,
    };
    let err = plan(vec![armed("p/a", "auto")], &probe).unwrap_err();
    assert_eq!(
        err,
        "docker found at /usr/local/bin/docker but the engine did not answer"
    );
    let err = plan(vec![armed("p/a", "docker")], &probe).unwrap_err();
    assert_eq!(
        err,
        "docker found at /usr/local/bin/docker but the engine did not answer"
    );
}

#[test]
fn two_silent_engines_are_both_reported() {
    let probe = |e: Engine| match e {
        Engine::Docker => silent_at("/d/docker"),
        Engine::Podman => silent_at("/p/podman"),
    };
    let err = plan(vec![armed("p/a", "auto")], &probe).unwrap_err();
    assert_eq!(
        err,
        "docker found at /d/docker but the engine did not answer; podman found at /p/podman but the engine did not answer"
    );
}

#[test]
fn an_explicit_engine_that_is_missing_fails_even_if_the_other_works() {
    let probe = |e: Engine| match e {
        Engine::Docker => ready_at("/bin/docker"),
        Engine::Podman => Probe::Missing,
    };
    let err = plan(vec![armed("p/a", "podman")], &probe).unwrap_err();
    assert_eq!(err, "podman was not found");
}

#[test]
fn rules_naming_different_engines_make_one_group_each() {
    let probe = |e: Engine| match e {
        Engine::Docker => ready_at("/bin/docker"),
        Engine::Podman => ready_at("/bin/podman"),
    };
    let rules = vec![
        armed("p/a", "podman"),
        armed("p/b", "docker"),
        armed("p/c", "auto"),
    ];
    let groups = plan(rules, &probe).unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].engine, Engine::Podman);
    assert_eq!(ids(&groups[0]), vec!["p/a"]);
    assert_eq!(groups[1].engine, Engine::Docker);
    assert_eq!(ids(&groups[1]), vec!["p/b", "p/c"]);
}

#[test]
fn each_engine_is_asked_once() {
    let asked = Cell::new(0);
    let probe = |_: Engine| {
        asked.set(asked.get() + 1);
        ready_at("/bin/engine")
    };
    let rules = vec![
        armed("p/a", "docker"),
        armed("p/b", "auto"),
        armed("p/c", "docker"),
    ];
    let groups = plan(rules, &probe).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(ids(&groups[0]), vec!["p/a", "p/b", "p/c"]);
    assert_eq!(asked.get(), 1);
}

#[test]
fn no_rules_means_no_groups_and_no_questions() {
    let probe = |_: Engine| -> Probe { panic!("must not be asked") };
    assert!(plan(Vec::new(), &probe).unwrap().is_empty());
}
