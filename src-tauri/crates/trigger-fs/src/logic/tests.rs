use super::*;
use serde_json::json;
use trigger_core::testkit::rule;

#[test]
fn the_empty_state_machine_fires_nothing() {
    let root = std::env::temp_dir();
    let parsed =
        crate::rule::parse(&rule("p/t", "fs.file-saved", json!({ "path": root }))).unwrap();
    let armed = Armed {
        id: "p/t".into(),
        kind: "fs.file-saved".into(),
        root: root.clone(),
        when: parsed.when,
        glob: parsed.glob,
        ignore: parsed.ignore,
    };
    let mut state = State::default();
    let out = step(&mut state, &armed, Signal::Tick { now: 1000 }, &|_| false);
    assert!(out.is_empty());
}
