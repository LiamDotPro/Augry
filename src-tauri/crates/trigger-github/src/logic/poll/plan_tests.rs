//! Tests for `plan`: which requests the rules need, and in which order.

use serde_json::json;

use super::*;
use crate::logic::diff::kind;
use crate::logic::testutil::{armed, ME};

const SINCE: &str = "2026-10-12T00:00:00Z";

fn rule(id: &str, kind: &str, extra: serde_json::Value) -> Armed {
    let mut when = json!({ "repo": "cli/cli" });
    for (k, v) in extra.as_object().unwrap() {
        when[k] = v.clone();
    }
    armed(id, kind, when)
}

fn every_kind(repo: &str) -> Vec<Armed> {
    crate::KINDS
        .iter()
        .map(|k| armed(&format!("{repo}/{}", k.id), k.id, json!({ "repo": repo })))
        .collect()
}

fn groups(jobs: &[Job]) -> Vec<Group> {
    jobs.iter().map(|j| j.group).collect()
}

#[test]
fn one_rule_makes_one_job_that_is_due_at_once() {
    let rules = vec![rule("p/rel", kind::RELEASE_PUBLISHED, json!({}))];
    let jobs = plan(&rules, ME, SINCE);
    assert_eq!(jobs.len(), 1);
    let job = &jobs[0];
    assert_eq!(job.repo, "cli/cli");
    assert_eq!(job.group, Group::Releases);
    assert_eq!(job.path, "repos/cli/cli/releases?per_page=10");
    assert_eq!(job.rules, [0]);
    assert_eq!(job.interval_s, 120);
    assert_eq!(job.due, 0);
    assert!(!job.not_found && !job.reported && job.hint_s.is_none());
}

#[test]
fn every_kind_makes_six_jobs_with_pull_requests_first() {
    let rules = every_kind("cli/cli");
    let jobs = plan(&rules, ME, SINCE);
    assert_eq!(
        groups(&jobs),
        [
            Group::Pulls,
            Group::Runs,
            Group::Releases,
            Group::AssignedIssues,
            Group::IssueComments,
            Group::PullComments
        ]
    );
    // review-requested, pr-commented, pr-merged and dependency-pr-opened read the pull request list.
    assert_eq!(jobs[0].rules.len(), 4);
    assert_eq!(jobs[4].rules.len(), 1);
    assert_eq!(jobs[5].rules.len(), 1);
}

#[test]
fn rules_that_share_a_request_share_one_job_and_the_shortest_interval_wins() {
    let rules = vec![
        rule("p/a", kind::RELEASE_PUBLISHED, json!({ "interval_s": 300 })),
        rule("p/b", kind::RELEASE_PUBLISHED, json!({ "interval_s": 90 })),
        rule("p/c", kind::RELEASE_PUBLISHED, json!({})),
    ];
    let jobs = plan(&rules, ME, SINCE);
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].rules, [0, 1, 2]);
    assert_eq!(jobs[0].interval_s, 90);
}

#[test]
fn each_repository_has_its_own_jobs_in_repository_order() {
    let mut rules = every_kind("b/two");
    rules.extend(every_kind("a/one"));
    let jobs = plan(&rules, ME, SINCE);
    assert_eq!(jobs.len(), 12);
    assert!(jobs[..6].iter().all(|j| j.repo == "a/one"));
    assert!(jobs[6..].iter().all(|j| j.repo == "b/two"));
    assert_eq!(jobs[0].group, Group::Pulls);
    assert_eq!(jobs[6].group, Group::Pulls);
}

#[test]
fn a_literal_branch_gets_its_own_runs_job_and_a_glob_does_not() {
    let rules = vec![
        rule("p/trunk", kind::CI_FAILED, json!({ "branch": "trunk" })),
        rule("p/trunk-ok", kind::CI_PASSED, json!({ "branch": "trunk" })),
        rule("p/glob", kind::CI_FAILED, json!({ "branch": "release/*" })),
        rule("p/any", kind::CI_FAILED, json!({})),
    ];
    let jobs = plan(&rules, ME, SINCE);
    assert_eq!(jobs.len(), 2);
    // The job without a branch comes first and serves the glob and the open rule.
    assert_eq!(jobs[0].rules, [2, 3]);
    assert_eq!(
        jobs[0].path,
        "repos/cli/cli/actions/runs?status=completed&per_page=100"
    );
    assert_eq!(jobs[1].rules, [0, 1]);
    assert_eq!(
        jobs[1].path,
        "repos/cli/cli/actions/runs?status=completed&per_page=100&branch=trunk"
    );
    assert_ne!(jobs[0].key, jobs[1].key);
}

#[test]
fn a_branch_on_a_pull_request_rule_does_not_split_the_pull_request_job() {
    let rules = vec![
        rule("p/a", kind::PR_MERGED, json!({ "branch": "trunk" })),
        rule("p/b", kind::PR_MERGED, json!({ "branch": "release/*" })),
    ];
    let jobs = plan(&rules, ME, SINCE);
    assert_eq!(jobs.len(), 1);
    assert!(!jobs[0].path.contains("branch="));
    assert_eq!(jobs[0].rules, [0, 1]);
}

#[test]
fn paths_carry_me_and_since_where_they_belong() {
    let rules = every_kind("cli/cli");
    let jobs = plan(&rules, ME, SINCE);
    let path = |group| jobs.iter().find(|j| j.group == group).unwrap().path.clone();
    assert!(path(Group::AssignedIssues).contains("assignee=LiamDotPro"));
    assert!(path(Group::IssueComments).ends_with("&since=2026-10-12T00%3A00%3A00Z"));
    assert!(path(Group::PullComments).ends_with("&since=2026-10-12T00%3A00%3A00Z"));
    assert!(!path(Group::Pulls).contains("since"));
    assert!(!path(Group::Releases).contains("since"));
    // The remembered feed never carries `since`.
    let key = |group| jobs.iter().find(|j| j.group == group).unwrap().key.clone();
    assert!(!key(Group::IssueComments).contains("since"));
}

#[test]
fn a_poller_knows_no_jobs_until_it_knows_who_you_are() {
    let poller = Poller::new(every_kind("cli/cli"), 1_791_763_200);
    assert!(poller.jobs().is_empty());
    assert!(!poller.is_stopped());
}
