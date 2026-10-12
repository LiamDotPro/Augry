//! Helpers shared by the tests in this folder. Compiled for tests only.
//!
//! The fixtures in `tests/fixtures/` are trimmed GitHub answers in the shapes
//! captured in plans/011-trigger-github/research/R01. Items are newest first,
//! as GitHub returns them. Ids, logins and text are made up around the real
//! shapes. The signed in user in these tests is `ME`.

use serde_json::Value;
use trigger_core::testkit::rule;

use super::json::{list, num};
use super::Armed;

/// The login the tests run as.
pub const ME: &str = "LiamDotPro";

pub const PULLS: &str = include_str!("../../tests/fixtures/pulls.json");
pub const RUNS: &str = include_str!("../../tests/fixtures/runs.json");
pub const RELEASES: &str = include_str!("../../tests/fixtures/releases.json");
pub const ISSUES: &str = include_str!("../../tests/fixtures/issues.json");
pub const ISSUE_COMMENTS: &str = include_str!("../../tests/fixtures/issue_comments.json");
pub const PULL_COMMENTS: &str = include_str!("../../tests/fixtures/pull_comments.json");

/// A rule as the poll loop holds it, parsed by the real `rule::parse`.
pub fn armed(id: &str, kind: &str, when: Value) -> Armed {
    let r = rule(id, kind, when);
    Armed {
        id: r.id.clone(),
        kind: r.kind.clone(),
        parsed: crate::rule::parse(&r).unwrap(),
    }
}

/// The body with only the items whose `id` is listed, written back as a body.
/// Works for a plain list and for `{"workflow_runs": [...]}`.
pub fn only(body: &str, ids: &[u64]) -> String {
    map_items(body, |items| {
        items
            .into_iter()
            .filter(|item| ids.contains(&num(item, "/id")))
            .collect()
    })
}

/// The body without the items whose `id` is listed.
pub fn without(body: &str, ids: &[u64]) -> String {
    map_items(body, |items| {
        items
            .into_iter()
            .filter(|item| !ids.contains(&num(item, "/id")))
            .collect()
    })
}

/// The body with one change made to every item whose `id` is given.
pub fn edit(body: &str, id: u64, change: impl Fn(&mut Value)) -> String {
    map_items(body, |mut items| {
        for item in items.iter_mut().filter(|i| num(i, "/id") == id) {
            change(item);
        }
        items
    })
}

fn map_items(body: &str, change: impl FnOnce(Vec<Value>) -> Vec<Value>) -> String {
    let wrapped = body.trim_start().starts_with('{');
    let items = change(list(body, wrapped.then_some("workflow_runs")).unwrap());
    if wrapped {
        serde_json::json!({ "total_count": items.len(), "workflow_runs": items }).to_string()
    } else {
        Value::Array(items).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(body: &str, wrapper: Option<&str>) -> Vec<u64> {
        list(body, wrapper)
            .unwrap()
            .iter()
            .map(|i| num(i, "/id"))
            .collect()
    }

    #[test]
    fn every_fixture_is_a_list_with_the_expected_count() {
        assert_eq!(ids(PULLS, None).len(), 7);
        assert_eq!(ids(RUNS, Some("workflow_runs")).len(), 6);
        assert_eq!(ids(RELEASES, None).len(), 4);
        assert_eq!(ids(ISSUES, None).len(), 3);
        assert_eq!(ids(ISSUE_COMMENTS, None).len(), 5);
        assert_eq!(ids(PULL_COMMENTS, None).len(), 3);
    }

    #[test]
    fn only_and_without_keep_order_and_work_on_wrapped_lists() {
        let body = only(PULLS, &[4829000005, 4830652819]);
        assert_eq!(ids(&body, None), [4830652819, 4829000005]);
        let rest = without(PULLS, &[4830652819]);
        assert_eq!(ids(&rest, None).len(), 6);
        let runs = without(RUNS, &[38303985560]);
        assert_eq!(ids(&runs, Some("workflow_runs")).len(), 5);
        let one = only(RUNS, &[38303985555]);
        assert_eq!(ids(&one, Some("workflow_runs")), [38303985555]);
    }

    #[test]
    fn edit_works_on_wrapped_lists_too() {
        let body = edit(RUNS, 38303985560, |i| i["conclusion"] = "success".into());
        let runs = list(&body, Some("workflow_runs")).unwrap();
        assert_eq!(runs[0]["conclusion"], "success");
        assert_eq!(runs[1]["conclusion"], "success");
        assert_eq!(runs[2]["conclusion"], "failure");
    }

    #[test]
    fn edit_changes_one_item() {
        let body = edit(PULLS, 4830652819, |i| i["state"] = "open".into());
        let items = list(&body, None).unwrap();
        assert_eq!(items[0]["state"], "open");
        assert_eq!(items[1]["state"], "open");
        assert_eq!(items[2]["state"], "open");
        assert_eq!(items[5]["state"], "closed");
    }

    #[test]
    fn armed_uses_the_real_parser() {
        let a = armed(
            "p/t",
            "github.pr-merged",
            serde_json::json!({ "repo": "a/b" }),
        );
        assert_eq!(a.parsed.when.repo, "a/b");
    }
}
