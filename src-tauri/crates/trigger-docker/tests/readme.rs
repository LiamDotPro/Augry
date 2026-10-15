//! Fails when the Kinds table in README.md does not match `KINDS`.

#[test]
fn readme_kinds_table_is_current() {
    let readme = include_str!("../README.md");
    let table = trigger_core::kinds_markdown(trigger_docker::KINDS);
    let between = readme
        .split("<!-- kinds -->")
        .nth(1)
        .and_then(|rest| rest.split("<!-- /kinds -->").next())
        .expect("README.md has <!-- kinds --> and <!-- /kinds --> markers");
    assert_eq!(
        between.trim(),
        table.trim(),
        "README.md Kinds table is stale: run `cargo run -p trigger-docker --example readme` and paste the output"
    );
}
