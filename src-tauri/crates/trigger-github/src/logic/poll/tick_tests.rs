//! Tests for `Poller::tick` when everything goes well: what is asked, when,
//! with which `ETag`, and what fires. Failures are in `verdict_tests.rs`.

use serde_json::json;

use super::*;
use crate::logic::diff::kind;
use crate::logic::fake_gh::{not_modified, ok, FakeGh};
use crate::logic::testutil::{
    armed, only, ISSUES, ISSUE_COMMENTS, ME, PULLS, PULL_COMMENTS, RELEASES, RUNS,
};

const REPO: &str = "cli/cli";
/// 2026-10-12T00:00:00Z, the start time the fixtures were written around.
const START: u64 = 1_791_763_200;
const GROUPS: [Group; 6] = [
    Group::Pulls,
    Group::Runs,
    Group::Releases,
    Group::AssignedIssues,
    Group::IssueComments,
    Group::PullComments,
];

fn every_kind() -> Vec<Armed> {
    crate::KINDS
        .iter()
        .map(|k| armed(&format!("p/{}", k.id), k.id, json!({ "repo": REPO })))
        .collect()
}

/// The request path of a group, as `plan` builds it for this test.
fn path(group: Group) -> String {
    path_for(REPO, group, ME, Some(&iso(START)), None)
}

fn tick(poller: &mut Poller, now: u64, gh: &FakeGh) -> Outcome {
    poller.tick(now, gh, &|| false)
}

/// The fixtures, reduced to a few items for the first poll.
fn seed_bodies() -> [String; 6] {
    [
        only(PULLS, &[4829000004, 4829000005, 4829000006]),
        only(RUNS, &[38303985555, 38303985556]),
        only(RELEASES, &[399674740, 399674700]),
        only(ISSUES, &[5741818100]),
        only(ISSUE_COMMENTS, &[6131952697, 6131952628]),
        only(PULL_COMMENTS, &[4233990557]),
    ]
}

fn full_bodies() -> [&'static str; 6] {
    [PULLS, RUNS, RELEASES, ISSUES, ISSUE_COMMENTS, PULL_COMMENTS]
}

/// Queues, for every group: the seed answer, the full answer, then a 304.
fn serve_three_rounds(gh: &FakeGh) {
    for ((group, seed), full) in GROUPS.into_iter().zip(seed_bodies()).zip(full_bodies()) {
        gh.serve(&path(group), ok(&seed, "W/\"seed\""));
        gh.serve(&path(group), ok(full, "W/\"news\""));
        gh.serve(&path(group), not_modified());
    }
}

fn count(out: &Outcome, kind: &str) -> usize {
    out.fires.iter().filter(|f| f.kind == kind).count()
}

#[test]
fn the_first_round_seeds_the_second_fires_and_the_third_is_a_304() {
    let gh = FakeGh::new(Some(ME));
    serve_three_rounds(&gh);
    let mut poller = Poller::new(every_kind(), START);

    let first = tick(&mut poller, START, &gh);
    assert!(first.fires.is_empty(), "{:?}", first.fires);
    assert!(first.notes.is_empty());
    let expected: Vec<String> = GROUPS.into_iter().map(path).collect();
    assert_eq!(gh.paths(), expected, "pull requests first, then the rest");
    assert!(gh.calls().iter().all(|(_, etag)| etag.is_none()));

    let second = tick(&mut poller, START + 120, &gh);
    assert_eq!(count(&second, kind::REVIEW_REQUESTED), 1);
    assert_eq!(count(&second, kind::DEPENDENCY_PR_OPENED), 2);
    assert_eq!(count(&second, kind::PR_MERGED), 1);
    assert_eq!(count(&second, kind::CI_FAILED), 3);
    assert_eq!(count(&second, kind::CI_PASSED), 1);
    assert_eq!(count(&second, kind::RELEASE_PUBLISHED), 1);
    assert_eq!(count(&second, kind::ISSUE_ASSIGNED), 1);
    assert_eq!(count(&second, kind::PR_COMMENTED), 2);
    assert_eq!(second.fires.len(), 12);
    let calls = gh.calls();
    assert_eq!(calls.len(), 12);
    assert!(calls[6..]
        .iter()
        .all(|(_, etag)| etag.as_deref() == Some("W/\"seed\"")));

    let third = tick(&mut poller, START + 240, &gh);
    assert!(third.fires.is_empty());
    let calls = gh.calls();
    assert_eq!(calls.len(), 18);
    assert!(calls[12..]
        .iter()
        .all(|(_, etag)| etag.as_deref() == Some("W/\"news\"")));
}

#[test]
fn the_login_is_asked_once() {
    let gh = FakeGh::new(Some(ME));
    serve_three_rounds(&gh);
    let mut poller = Poller::new(every_kind(), START);
    for round in 0..3 {
        tick(&mut poller, START + round * 120, &gh);
    }
    assert_eq!(gh.logins(), 1);
}

#[test]
fn nothing_is_asked_before_the_interval_is_up() {
    let gh = FakeGh::new(Some(ME));
    serve_three_rounds(&gh);
    let mut poller = Poller::new(every_kind(), START);
    tick(&mut poller, START, &gh);
    assert_eq!(gh.calls().len(), 6);
    for second in [1, 60, 119] {
        let out = tick(&mut poller, START + second, &gh);
        assert!(out.fires.is_empty());
    }
    assert_eq!(gh.calls().len(), 6);
    tick(&mut poller, START + 120, &gh);
    assert_eq!(gh.calls().len(), 12);
}

#[test]
fn a_rule_with_a_longer_interval_is_asked_less_often() {
    let gh = FakeGh::new(Some(ME));
    let release = path_for(REPO, Group::Releases, ME, None, None);
    let runs = path_for(REPO, Group::Runs, ME, None, None);
    gh.serve(&release, ok(&only(RELEASES, &[399674740]), "W/\"r\""));
    gh.serve(&runs, ok(&only(RUNS, &[38303985555]), "W/\"c\""));
    let rules = vec![
        armed(
            "p/rel",
            kind::RELEASE_PUBLISHED,
            json!({ "repo": REPO, "interval_s": 60 }),
        ),
        armed(
            "p/ci",
            kind::CI_FAILED,
            json!({ "repo": REPO, "interval_s": 300 }),
        ),
    ];
    let mut poller = Poller::new(rules, START);
    for second in [0, 60, 120, 180, 240, 300] {
        tick(&mut poller, START + second, &gh);
    }
    let asked = |p: &str| gh.paths().iter().filter(|x| *x == p).count();
    assert_eq!(asked(&release), 6);
    assert_eq!(asked(&runs), 2);
}

#[test]
fn only_the_requests_a_rule_needs_are_made() {
    let gh = FakeGh::new(Some(ME));
    let rules = vec![armed(
        "p/rel",
        kind::RELEASE_PUBLISHED,
        json!({ "repo": REPO }),
    )];
    let mut poller = Poller::new(rules, START);
    tick(&mut poller, START, &gh);
    assert_eq!(gh.paths(), ["repos/cli/cli/releases?per_page=10"]);
}

#[test]
fn a_cancelled_tick_asks_nothing() {
    let gh = FakeGh::new(Some(ME));
    let mut poller = Poller::new(every_kind(), START);
    let out = poller.tick(START, &gh, &|| true);
    assert!(out.fires.is_empty());
    assert!(gh.calls().is_empty());
}

#[test]
fn a_comment_from_before_the_start_never_fires() {
    let gh = FakeGh::new(Some(ME));
    let rules = vec![armed("p/c", kind::PR_COMMENTED, json!({ "repo": REPO }))];
    // The seed has nothing, so everything in the later answers looks new. The
    // review comment on #14568 was written on 2026-10-09, before the start.
    gh.serve(&path(Group::Pulls), ok("[]", "W/\"p\""));
    gh.serve(&path(Group::IssueComments), ok("[]", "W/\"i\""));
    gh.serve(&path(Group::PullComments), ok("[]", "W/\"r\""));
    gh.serve(&path(Group::Pulls), ok(PULLS, "W/\"p2\""));
    gh.serve(&path(Group::IssueComments), ok(ISSUE_COMMENTS, "W/\"i2\""));
    gh.serve(&path(Group::PullComments), ok(PULL_COMMENTS, "W/\"r2\""));
    let mut poller = Poller::new(rules, START);
    tick(&mut poller, START, &gh);
    let out = tick(&mut poller, START + 120, &gh);
    let ids: Vec<&serde_json::Value> = out.fires.iter().map(|f| &f.payload["id"]).collect();
    assert!(!ids.contains(&&json!(4233990557u64)), "{ids:?}");
    assert!(!out.fires.is_empty());
}
