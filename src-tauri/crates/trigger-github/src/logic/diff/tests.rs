//! Tests for `diff.rs`.

use super::*;
use crate::logic::json::{num, text};
use crate::logic::payload;
use crate::logic::testutil::armed;
use serde_json::json;

#[test]
fn every_kind_has_a_group_and_the_constants_match_kinds() {
    let ids = [
        kind::ISSUE_ASSIGNED,
        kind::REVIEW_REQUESTED,
        kind::CI_FAILED,
        kind::CI_PASSED,
        kind::PR_COMMENTED,
        kind::PR_MERGED,
        kind::RELEASE_PUBLISHED,
        kind::DEPENDENCY_PR_OPENED,
    ];
    let known: Vec<&str> = crate::KINDS.iter().map(|k| k.id).collect();
    assert_eq!(ids.to_vec(), known);
    for id in ids {
        assert!(!groups_for(id).is_empty(), "{id} has no group");
    }
    assert!(groups_for("fs.file-saved").is_empty());
}

#[test]
fn comments_need_pulls_first_and_groups_are_in_polling_order() {
    let groups = groups_for(kind::PR_COMMENTED);
    assert_eq!(
        groups,
        [Group::Pulls, Group::IssueComments, Group::PullComments]
    );
    assert!(groups.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(groups_for(kind::PR_MERGED), [Group::Pulls]);
    assert_eq!(groups_for(kind::CI_PASSED), [Group::Runs]);
}

#[test]
fn paths_match_the_research_note() {
    let p = |g, since, branch| path_for("cli/cli", g, "LiamDotPro", since, branch);
    assert_eq!(
        p(Group::Pulls, None, None),
        "repos/cli/cli/pulls?state=all&sort=updated&direction=desc&per_page=100"
    );
    assert_eq!(
        p(Group::Runs, None, None),
        "repos/cli/cli/actions/runs?status=completed&per_page=100"
    );
    assert_eq!(
        p(Group::Releases, None, None),
        "repos/cli/cli/releases?per_page=10"
    );
    assert_eq!(
        p(Group::AssignedIssues, None, None),
        "repos/cli/cli/issues?state=open&assignee=LiamDotPro&sort=updated&direction=desc&per_page=100"
    );
    assert_eq!(
        p(Group::IssueComments, None, None),
        "repos/cli/cli/issues/comments?sort=updated&direction=desc&per_page=100"
    );
    assert_eq!(
        p(Group::PullComments, None, None),
        "repos/cli/cli/pulls/comments?sort=updated&direction=desc&per_page=100"
    );
}

#[test]
fn branch_and_since_are_added_only_where_they_belong() {
    let p = |g, since, branch| path_for("cli/cli", g, "me", since, branch);
    assert_eq!(
        p(Group::Runs, None, Some("release/1")),
        "repos/cli/cli/actions/runs?status=completed&per_page=100&branch=release%2F1"
    );
    let since = Some("2026-10-12T20:24:14Z");
    assert!(p(Group::IssueComments, since, None).ends_with("&since=2026-10-12T20%3A24%3A14Z"));
    assert!(p(Group::PullComments, since, None).ends_with("&since=2026-10-12T20%3A24%3A14Z"));
    assert_eq!(
        p(Group::Pulls, since, Some("x")),
        p(Group::Pulls, None, None)
    );
    assert_eq!(p(Group::Runs, since, None), p(Group::Runs, None, None));
}

#[test]
fn the_assignee_is_encoded() {
    let path = path_for("a/b", Group::AssignedIssues, "bot[1]", None, None);
    assert!(path.contains("assignee=bot%5B1%5D&"), "{path}");
}

#[test]
fn the_feed_key_ignores_since_but_not_the_branch() {
    let plain = feed_key("a/b", Group::Runs, "me", None);
    let on_main = feed_key("a/b", Group::Runs, "me", Some("main"));
    assert_ne!(plain, on_main);
    let with_since = path_for("a/b", Group::IssueComments, "me", Some("t"), None);
    assert_ne!(
        with_since,
        feed_key("a/b", Group::IssueComments, "me", None)
    );
}

#[test]
fn etags_are_kept_per_path() {
    let mut state = RepoState::default();
    assert_eq!(state.etag("p"), None);
    state.set_etag("p", "W/\"1\"");
    state.set_etag("q", "W/\"2\"");
    state.set_etag("p", "W/\"3\"");
    assert_eq!(state.etag("p"), Some("W/\"3\""));
    assert_eq!(state.etag("q"), Some("W/\"2\""));
}

fn by_id(v: &Value) -> Option<String> {
    Some(num(v, "/id").to_string())
}

fn any_item(_: &Armed, _: &Value) -> bool {
    true
}

fn spec<'a>(
    key: &'a dyn Fn(&Value) -> Option<String>,
    accept: &'a dyn Fn(&Armed, &Value) -> bool,
) -> Spec<'a> {
    Spec {
        kind: kind::PR_MERGED,
        key,
        accept,
        build: payload::merged,
    }
}

fn merged_rule(id: &str) -> Armed {
    armed(id, kind::PR_MERGED, json!({ "repo": "a/b" }))
}

#[test]
fn each_new_fires_once_per_unseen_key_and_remembers_it() {
    let mut feed = Feed::default();
    let rules = [merged_rule("p/one")];
    let refs: Vec<&Armed> = rules.iter().collect();
    let items = vec![
        json!({ "id": 2, "number": 20 }),
        json!({ "id": 1, "number": 10 }),
    ];
    let (key, accept) = (
        &by_id as &dyn Fn(&Value) -> _,
        &any_item as &dyn Fn(&_, &_) -> _,
    );
    let mut out = Vec::new();
    each_new(&mut feed, &spec(key, accept), &refs, &items, &mut out);
    let numbers: Vec<_> = out.iter().map(|f| f.payload["number"].clone()).collect();
    assert_eq!(numbers, [json!(20), json!(10)]);
    assert_eq!(out[0].rule, "p/one");
    assert_eq!(out[0].kind, kind::PR_MERGED);
    let mut again = Vec::new();
    each_new(&mut feed, &spec(key, accept), &refs, &items, &mut again);
    assert!(again.is_empty());
}

#[test]
fn each_new_feeds_every_rule_of_the_kind_and_skips_other_kinds() {
    let mut feed = Feed::default();
    let rules = [
        merged_rule("p/one"),
        merged_rule("p/two"),
        armed("p/ci", kind::CI_FAILED, json!({ "repo": "a/b" })),
    ];
    let refs: Vec<&Armed> = rules.iter().collect();
    let items = vec![json!({ "id": 1 })];
    let mut out = Vec::new();
    each_new(&mut feed, &spec(&by_id, &any_item), &refs, &items, &mut out);
    let ids: Vec<&str> = out.iter().map(|f| f.rule.as_str()).collect();
    assert_eq!(ids, ["p/one", "p/two"]);
}

#[test]
fn each_new_remembers_items_a_rule_refused() {
    let mut feed = Feed::default();
    let rules = [merged_rule("p/one")];
    let refs: Vec<&Armed> = rules.iter().collect();
    let items = vec![json!({ "id": 1, "base": { "ref": "dev" } })];
    let only_trunk = |_: &Armed, v: &Value| text(v, "/base/ref") == "trunk";
    let mut out = Vec::new();
    each_new(
        &mut feed,
        &spec(&by_id, &only_trunk),
        &refs,
        &items,
        &mut out,
    );
    assert!(out.is_empty());
    assert!(feed.seen[kind::PR_MERGED].contains("1"));
}

#[test]
fn each_new_skips_items_without_a_key_and_duplicates_in_one_answer() {
    let mut feed = Feed::default();
    let rules = [merged_rule("p/one")];
    let refs: Vec<&Armed> = rules.iter().collect();
    let items = vec![json!({ "id": 5 }), json!({ "id": 5 }), json!({ "id": 6 })];
    let odd_only = |v: &Value| (num(v, "/id") == 5).then(|| "5".to_string());
    let mut out = Vec::new();
    each_new(
        &mut feed,
        &spec(&odd_only, &any_item),
        &refs,
        &items,
        &mut out,
    );
    assert_eq!(out.len(), 1);
}

#[test]
fn each_new_leaves_no_trace_when_no_rule_has_the_kind() {
    let mut feed = Feed::default();
    let mut out = Vec::new();
    let items = vec![json!({ "id": 1 })];
    each_new(&mut feed, &spec(&by_id, &any_item), &[], &items, &mut out);
    assert!(out.is_empty());
    assert!(feed.seen.is_empty());
}
