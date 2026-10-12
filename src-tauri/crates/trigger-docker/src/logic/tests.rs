use super::*;
use serde_json::json;
use trigger_core::testkit::rule;

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
