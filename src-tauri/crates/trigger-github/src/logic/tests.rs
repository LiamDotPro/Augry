//! The whole round: every group, polled in order, through `diff` and `State`,
//! with the fixtures. The per group rules are tested in their own files.

use serde_json::json;

use super::diff::{diff, feed_key, kind, path_for, Fire, Group};
use super::testutil::{
    armed, only, ISSUES, ISSUE_COMMENTS, ME, PULLS, PULL_COMMENTS, RELEASES, RUNS,
};
use super::{Armed, State};

const REPO: &str = "cli/cli";
const SINCE: &str = "2026-10-12T00:00:00Z";

fn every_kind() -> Vec<Armed> {
    crate::KINDS
        .iter()
        .map(|k| armed(&format!("p/{}", k.id), k.id, json!({ "repo": REPO })))
        .collect()
}

/// One round over all six groups, in polling order.
fn round(state: &mut State, rules: &[Armed], bodies: [&str; 6]) -> Vec<Fire> {
    let refs: Vec<&Armed> = rules.iter().collect();
    let groups = [
        Group::Pulls,
        Group::Runs,
        Group::Releases,
        Group::AssignedIssues,
        Group::IssueComments,
        Group::PullComments,
    ];
    let mut out = Vec::new();
    for (group, body) in groups.into_iter().zip(bodies) {
        let key = feed_key(REPO, group, ME, None);
        let repo = state.repo(REPO);
        out.extend(diff(group, body, &key, repo, &refs, ME, SINCE).unwrap());
    }
    out
}

fn count(fires: &[Fire], kind: &str) -> usize {
    fires.iter().filter(|f| f.kind == kind).count()
}

#[test]
fn a_seed_round_is_silent_then_news_fires_once_per_kind() {
    let rules = every_kind();
    let mut state = State::default();
    let seed = round(
        &mut state,
        &rules,
        [
            &only(PULLS, &[4829000004, 4829000005, 4829000006]),
            &only(RUNS, &[38303985555, 38303985556]),
            &only(RELEASES, &[399674740, 399674700]),
            &only(ISSUES, &[5741818100]),
            &only(ISSUE_COMMENTS, &[6131952697, 6131952628]),
            &only(PULL_COMMENTS, &[4233990557]),
        ],
    );
    assert!(seed.is_empty(), "{seed:?}");

    let news = round(
        &mut state,
        &rules,
        [PULLS, RUNS, RELEASES, ISSUES, ISSUE_COMMENTS, PULL_COMMENTS],
    );
    assert_eq!(count(&news, kind::REVIEW_REQUESTED), 1);
    assert_eq!(count(&news, kind::DEPENDENCY_PR_OPENED), 2);
    assert_eq!(count(&news, kind::PR_MERGED), 1);
    assert_eq!(count(&news, kind::CI_FAILED), 3);
    assert_eq!(count(&news, kind::CI_PASSED), 1);
    assert_eq!(count(&news, kind::RELEASE_PUBLISHED), 1);
    assert_eq!(count(&news, kind::ISSUE_ASSIGNED), 1);
    // One on an issue comment list (a pull request found just above) and one review comment.
    assert_eq!(count(&news, kind::PR_COMMENTED), 2);
    assert_eq!(news.len(), 12);

    let quiet = round(
        &mut state,
        &rules,
        [PULLS, RUNS, RELEASES, ISSUES, ISSUE_COMMENTS, PULL_COMMENTS],
    );
    assert!(quiet.is_empty(), "{quiet:?}");
}

#[test]
fn every_fire_carries_the_rule_and_exactly_the_documented_payload() {
    let rules = every_kind();
    let mut state = State::default();
    let seed = ["[]", "{\"workflow_runs\":[]}", "[]", "[]", "[]", "[]"];
    assert!(round(&mut state, &rules, seed).is_empty());
    let news = round(
        &mut state,
        &rules,
        [PULLS, RUNS, RELEASES, ISSUES, ISSUE_COMMENTS, PULL_COMMENTS],
    );
    assert!(!news.is_empty());
    for fire in &news {
        assert_eq!(fire.rule, format!("p/{}", fire.kind));
        let info = crate::KINDS.iter().find(|k| k.id == fire.kind).unwrap();
        let mut documented: Vec<&str> = info.payload.iter().map(|f| f.name).collect();
        documented.sort_unstable();
        let keys: Vec<&str> = fire.payload.keys().map(String::as_str).collect();
        assert_eq!(keys, documented, "{}", fire.kind);
    }
}

#[test]
fn a_bad_body_is_an_error_and_the_next_good_one_is_still_a_seed() {
    let rules = every_kind();
    let refs: Vec<&Armed> = rules.iter().collect();
    let mut state = State::default();
    let key = feed_key(REPO, Group::Releases, ME, None);
    let repo = state.repo(REPO);
    let bad = diff(
        Group::Releases,
        "{\"message\":\"Bad\"}",
        &key,
        repo,
        &refs,
        ME,
        SINCE,
    );
    assert!(bad.is_err());
    let seed = diff(Group::Releases, RELEASES, &key, repo, &refs, ME, SINCE).unwrap();
    assert!(seed.is_empty());
}

#[test]
fn repositories_and_branch_filtered_feeds_are_seeded_separately() {
    let rules = every_kind();
    let refs: Vec<&Armed> = rules.iter().collect();
    let mut state = State::default();
    let any = feed_key(REPO, Group::Runs, ME, None);
    let trunk = feed_key(REPO, Group::Runs, ME, Some("trunk"));
    assert_ne!(any, trunk);
    let first = diff(Group::Runs, RUNS, &any, state.repo(REPO), &refs, ME, SINCE).unwrap();
    assert!(first.is_empty());
    let other_feed = diff(
        Group::Runs,
        RUNS,
        &trunk,
        state.repo(REPO),
        &refs,
        ME,
        SINCE,
    )
    .unwrap();
    assert!(other_feed.is_empty());
    let other_repo = diff(Group::Runs, RUNS, &any, state.repo("a/b"), &refs, ME, SINCE).unwrap();
    assert!(other_repo.is_empty());
    assert_eq!(state.repos.len(), 2);
}

#[test]
fn etags_live_in_the_repo_state_by_full_path() {
    let mut state = State::default();
    let path = path_for(REPO, Group::Pulls, ME, None, None);
    state.repo(REPO).set_etag(&path, "W/\"093a24\"");
    assert_eq!(state.repo(REPO).etag(&path), Some("W/\"093a24\""));
    assert_eq!(state.repo("a/b").etag(&path), None);
}
