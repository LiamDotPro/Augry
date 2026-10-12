---
id: T15
epic: 011-trigger-github
title: Test diff.rs and add the fixtures and test helpers
status: todo
depends_on: [T14]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-github/src/logic/diff/tests.rs
  - src-tauri/crates/trigger-github/src/logic/diff.rs
  - src-tauri/crates/trigger-github/tests/fixtures/pulls.json
  - src-tauri/crates/trigger-github/tests/fixtures/runs.json
  - src-tauri/crates/trigger-github/tests/fixtures/releases.json
  - src-tauri/crates/trigger-github/tests/fixtures/issues.json
  - src-tauri/crates/trigger-github/tests/fixtures/issue_comments.json
  - src-tauri/crates/trigger-github/tests/fixtures/pull_comments.json
  - src-tauri/crates/trigger-github/src/logic/testutil.rs
---

## Goal

The tests for `diff.rs` exist (`logic/diff/tests.rs`, 12 tests), the six fixture files of trimmed GitHub answers are saved under `tests/fixtures/`, and `logic/testutil.rs` is replaced by the full helper file that loads them (`PULLS`, `RUNS`, `RELEASES`, `ISSUES`, `ISSUE_COMMENTS`, `PULL_COMMENTS`, `ME`) and has `armed`, `only`, `without` and `edit`.

## Read first

- `plans/011-trigger-github/research/R01-endpoints.md` "Evidence", the trimmed real responses (the fixtures are these shapes with more items)
- `src-tauri/crates/trigger-github/src/logic/diff.rs` what the tests test (created by T14)
- `src-tauri/crates/trigger-github/src/logic/testutil.rs` the file you replace (created by T14)

## Steps

1. Create `src-tauri/crates/trigger-github/src/logic/diff/tests.rs` with exactly this.

```rust
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
```

2. Append two lines to `diff.rs` so it loads its tests. Run this from the repo root:

```
printf '\n#[cfg(test)]\nmod tests;\n' >> src-tauri/crates/trigger-github/src/logic/diff.rs
```

3. Create `src-tauri/crates/trigger-github/tests/fixtures/pulls.json` with exactly this.

The fixtures are made-up items in the shapes R01 captured from `cli/cli`: the ids, logins and text are invented around the real field names, and nothing in them is secret. Items are newest first, as GitHub returns them. Later cards pick items out by `id`. `LiamDotPro` is the signed in user in the tests (`ME`). Copy each file exactly: the ids and dates are what the tests count on.

```json
[
  {"id":4830652819,"number":14648,"title":"Fix the unclean background colors of gh-search-code","state":"closed","merged_at":"2026-10-12T16:14:17Z","created_at":"2026-10-10T08:00:00Z","updated_at":"2026-10-12T16:14:17Z","html_url":"https://github.com/cli/cli/pull/14648","user":{"login":"A4-Tacks","type":"User"},"head":{"ref":"end-search-bg"},"base":{"ref":"trunk"},"requested_reviewers":[{"login":"sergiou87"}],"requested_teams":[],"draft":false},
  {"id":4829000001,"number":14651,"title":"chore(deps): update dependency golang.org/x/term to v0.26.0","state":"open","merged_at":null,"created_at":"2026-10-12T15:00:00Z","updated_at":"2026-10-12T15:00:00Z","html_url":"https://github.com/cli/cli/pull/14651","user":{"login":"renovate[bot]","type":"Bot"},"head":{"ref":"renovate/golang.org-x-term-0.x"},"base":{"ref":"trunk"},"requested_reviewers":[],"requested_teams":[],"draft":false},
  {"id":4829000002,"number":14650,"title":"chore(deps): bump github.com/mattn/go-colorable from 0.1.15 to 0.1.16","state":"open","merged_at":null,"created_at":"2026-10-09T14:03:29Z","updated_at":"2026-10-09T14:03:29Z","html_url":"https://github.com/cli/cli/pull/14650","user":{"login":"dependabot[bot]","type":"Bot"},"head":{"ref":"dependabot/go_modules/github.com/mattn/go-colorable-0.1.16"},"base":{"ref":"trunk"},"requested_reviewers":[],"requested_teams":[],"draft":false},
  {"id":4829000003,"number":14649,"title":"Add a flag to gh pr list","state":"open","merged_at":null,"created_at":"2026-10-12T14:00:00Z","updated_at":"2026-10-12T14:00:00Z","html_url":"https://github.com/cli/cli/pull/14649","user":{"login":"octocat","type":"User"},"head":{"ref":"octocat-flag"},"base":{"ref":"trunk"},"requested_reviewers":[{"login":"LiamDotPro"},{"login":"sergiou87"}],"requested_teams":[],"draft":false},
  {"id":4829000004,"number":14640,"title":"Backport the colour fix","state":"closed","merged_at":"2026-10-11T09:00:00Z","created_at":"2026-10-11T08:00:00Z","updated_at":"2026-10-11T09:00:00Z","html_url":"https://github.com/cli/cli/pull/14640","user":{"login":"octocat","type":"User"},"head":{"ref":"backport-1"},"base":{"ref":"release/2.100"},"requested_reviewers":[],"requested_teams":[],"draft":false},
  {"id":4829000005,"number":14630,"title":"Try a different approach to prompts","state":"closed","merged_at":null,"created_at":"2026-10-08T08:00:00Z","updated_at":"2026-10-08T08:00:00Z","html_url":"https://github.com/cli/cli/pull/14630","user":{"login":"octocat","type":"User"},"head":{"ref":"prompt-try"},"base":{"ref":"trunk"},"requested_reviewers":[],"requested_teams":[],"draft":false},
  {"id":4829000006,"number":14620,"title":"Document the new flag","state":"open","merged_at":null,"created_at":"2026-10-07T08:00:00Z","updated_at":"2026-10-07T08:00:00Z","html_url":"https://github.com/cli/cli/pull/14620","user":{"login":"LiamDotPro","type":"User"},"head":{"ref":"docs-flag"},"base":{"ref":"trunk"},"requested_reviewers":[],"requested_teams":[],"draft":false}
]
```

4. Create `src-tauri/crates/trigger-github/tests/fixtures/runs.json` with exactly this.

```json
{
 "total_count": 6,
 "workflow_runs": [
    {"id":38303985560,"run_number":5560,"run_attempt":1,"name":"CI","head_branch":"trunk","head_sha":"a1b2c3d4e5f60718293a4b5c6d7e8f9012345678","event":"push","status":"completed","conclusion":"failure","html_url":"https://github.com/cli/cli/actions/runs/38303985560","created_at":"2026-10-12T20:40:00Z","updated_at":"2026-10-12T20:40:00Z","run_started_at":"2026-10-12T20:40:00Z","actor":{"login":"BagToad"}},
    {"id":38303985559,"run_number":5559,"run_attempt":1,"name":"Lint","head_branch":"trunk","head_sha":"a1b2c3d4e5f60718293a4b5c6d7e8f9012345678","event":"push","status":"completed","conclusion":"success","html_url":"https://github.com/cli/cli/actions/runs/38303985559","created_at":"2026-10-12T20:39:00Z","updated_at":"2026-10-12T20:39:00Z","run_started_at":"2026-10-12T20:39:00Z","actor":{"login":"BagToad"}},
    {"id":38303985558,"run_number":5558,"run_attempt":1,"name":"CI","head_branch":"feature/login","head_sha":"b2c3d4e5f60718293a4b5c6d7e8f90123456789","event":"pull_request","status":"completed","conclusion":"failure","html_url":"https://github.com/cli/cli/actions/runs/38303985558","created_at":"2026-10-12T20:38:00Z","updated_at":"2026-10-12T20:38:00Z","run_started_at":"2026-10-12T20:38:00Z","actor":{"login":"octocat"}},
    {"id":38303985557,"run_number":5557,"run_attempt":1,"name":"Release","head_branch":"release/2.100","head_sha":"c3d4e5f60718293a4b5c6d7e8f9012345678901","event":"push","status":"completed","conclusion":"timed_out","html_url":"https://github.com/cli/cli/actions/runs/38303985557","created_at":"2026-10-12T20:37:00Z","updated_at":"2026-10-12T20:37:00Z","run_started_at":"2026-10-12T20:37:00Z","actor":{"login":"BagToad"}},
    {"id":38303985556,"run_number":5556,"run_attempt":1,"name":"CI","head_branch":"trunk","head_sha":"d4e5f60718293a4b5c6d7e8f901234567890123","event":"push","status":"completed","conclusion":"cancelled","html_url":"https://github.com/cli/cli/actions/runs/38303985556","created_at":"2026-10-12T20:30:00Z","updated_at":"2026-10-12T20:30:00Z","run_started_at":"2026-10-12T20:30:00Z","actor":{"login":"BagToad"}},
    {"id":38303985555,"run_number":5555,"run_attempt":1,"name":"Triage Scheduled Tasks","head_branch":"trunk","head_sha":"54a671d9a8f8c7a785c132d6f6a25375dc96acc1","event":"schedule","status":"completed","conclusion":"success","html_url":"https://github.com/cli/cli/actions/runs/38303985555","created_at":"2026-10-12T20:24:27Z","updated_at":"2026-10-12T20:24:27Z","run_started_at":"2026-10-12T20:24:27Z","actor":{"login":"BagToad"}}
 ]
}
```

5. Create `src-tauri/crates/trigger-github/tests/fixtures/releases.json` with exactly this.

```json
[
  {"id":399674750,"tag_name":"v2.103.0-rc.1","name":"GitHub CLI 2.103.0 RC1","draft":false,"prerelease":true,"published_at":"2026-10-10T02:40:02Z","created_at":"2026-10-10T02:40:02Z","html_url":"https://github.com/cli/cli/releases/tag/v2.103.0-rc.1","author":{"login":"github-actions[bot]"}},
  {"id":399674745,"tag_name":"v2.102.1","name":null,"draft":true,"prerelease":false,"published_at":null,"created_at":"2026-10-10T00:00:00Z","html_url":"https://github.com/cli/cli/releases/tag/v2.102.1","author":{"login":"github-actions[bot]"}},
  {"id":399674740,"tag_name":"v2.102.0","name":"GitHub CLI 2.102.0","draft":false,"prerelease":false,"published_at":"2026-09-30T02:40:02Z","created_at":"2026-09-30T02:40:02Z","html_url":"https://github.com/cli/cli/releases/tag/v2.102.0","author":{"login":"github-actions[bot]"}},
  {"id":399674700,"tag_name":"v2.101.0","name":"GitHub CLI 2.101.0","draft":false,"prerelease":false,"published_at":"2026-09-15T02:40:02Z","created_at":"2026-09-15T02:40:02Z","html_url":"https://github.com/cli/cli/releases/tag/v2.101.0","author":{"login":"github-actions[bot]"}}
]
```

6. Create `src-tauri/crates/trigger-github/tests/fixtures/issues.json` with exactly this.

```json
[
  {"id":5741818300,"number":14660,"title":"Crash when the config file is empty","state":"open","updated_at":"2026-10-12T10:00:00Z","html_url":"https://github.com/cli/cli/issues/14660","user":{"login":"octocat"},"assignees":[{"login":"LiamDotPro"}],"repository_url":"https://api.github.com/repos/cli/cli"},
  {"id":5741818299,"number":14655,"title":"Update the contributing guide","state":"open","updated_at":"2026-10-12T10:00:00Z","html_url":"https://github.com/cli/cli/pull/14655","user":{"login":"Copilot"},"assignees":[{"login":"LiamDotPro"},{"login":"Copilot"}],"repository_url":"https://api.github.com/repos/cli/cli","pull_request":{"url":"https://api.github.com/repos/cli/cli/pulls/14655"}},
  {"id":5741818100,"number":14600,"title":"Support gh pr checks --watch for forks","state":"open","updated_at":"2026-10-12T10:00:00Z","html_url":"https://github.com/cli/cli/issues/14600","user":{"login":"babakks"},"assignees":[{"login":"LiamDotPro"}],"repository_url":"https://api.github.com/repos/cli/cli"}
]
```

7. Create `src-tauri/crates/trigger-github/tests/fixtures/issue_comments.json` with exactly this.

```json
[
  {"id":6131952700,"issue_url":"https://api.github.com/repos/cli/cli/issues/14649","html_url":"https://github.com/cli/cli/issues/14649#issuecomment-6131952700","created_at":"2026-10-12T20:30:00Z","updated_at":"2026-10-12T20:30:00Z","user":{"login":"sergiou87"},"body":"Looks good to me, one nit on the flag name."},
  {"id":6131952699,"issue_url":"https://api.github.com/repos/cli/cli/issues/14660","html_url":"https://github.com/cli/cli/issues/14660#issuecomment-6131952699","created_at":"2026-10-12T20:29:00Z","updated_at":"2026-10-12T20:29:00Z","user":{"login":"octocat"},"body":"I can reproduce this on trunk."},
  {"id":6131952698,"issue_url":"https://api.github.com/repos/cli/cli/issues/14649","html_url":"https://github.com/cli/cli/issues/14649#issuecomment-6131952698","created_at":"2026-10-12T20:20:54Z","updated_at":"2026-10-12T20:20:54Z","user":{"login":"LiamDotPro"},"body":"Thanks, I will rename it."},
  {"id":6131952697,"issue_url":"https://api.github.com/repos/cli/cli/issues/14620","html_url":"https://github.com/cli/cli/issues/14620#issuecomment-6131952697","created_at":"2026-10-01T10:00:00Z","updated_at":"2026-10-12T20:10:00Z","user":{"login":"octocat"},"body":"An old comment that was edited today."},
  {"id":6131952628,"issue_url":"https://api.github.com/repos/cli/cli/issues/13923","html_url":"https://github.com/cli/cli/issues/13923#issuecomment-6131952628","created_at":"2026-10-12T20:20:54Z","updated_at":"2026-10-12T20:29:40Z","user":{"login":"4wrxb"},"body":"A comment on a pull request that is not in the list."}
]
```

8. Create `src-tauri/crates/trigger-github/tests/fixtures/pull_comments.json` with exactly this.

```json
[
  {"id":4233990601,"pull_request_url":"https://api.github.com/repos/cli/cli/pulls/14649","html_url":"https://github.com/cli/cli/pull/14649#discussion_r4233990601","created_at":"2026-10-12T20:32:00Z","updated_at":"2026-10-12T20:32:00Z","user":{"login":"LiamDotPro"},"body":"Fixed in the next commit."},
  {"id":4233990600,"pull_request_url":"https://api.github.com/repos/cli/cli/pulls/14649","html_url":"https://github.com/cli/cli/pull/14649#discussion_r4233990600","created_at":"2026-10-12T20:31:00Z","updated_at":"2026-10-12T20:31:00Z","user":{"login":"BagToad"},"body":"Could this use the shared helper instead?"},
  {"id":4233990557,"pull_request_url":"https://api.github.com/repos/cli/cli/pulls/14568","html_url":"https://github.com/cli/cli/pull/14568#discussion_r4233990557","created_at":"2026-10-09T19:50:38Z","updated_at":"2026-10-09T19:50:38Z","user":{"login":"BagToad"},"body":"This was written before the app started."}
]
```

9. Replace the whole of `src-tauri/crates/trigger-github/src/logic/testutil.rs` with exactly this.

`only` and `without` keep or drop items by `id` and write the list back (for `runs.json` they work inside `workflow_runs`). `edit` changes the items with one `id`. Tests use them to build "the poll before" and "the poll after" from one fixture.

```rust
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
```

10. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib` passes with 69 tests (17 new in this card)
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (the `source`, `readme` and `live` tests included)
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (warnings that `lib.rs` and `os.rs` still have template code are expected until slice C)
- [ ] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- Changing a fixture to make a test pass, or adding items. Later cards count on these exact files.
- Real network captures. The fixtures stay hand written.
- Removing `#![allow(dead_code)]` from `logic.rs`.

## If stuck

- If a test in `diff/tests.rs` fails, compare the file with its step character by character. Do not change a test.
- If `testutil::tests::every_fixture_is_a_list_with_the_expected_count` fails, a fixture file differs from the card (a missing item or a typo). Compare it character by character.
- If `include_str!` cannot find a fixture, the file is not at `src-tauri/crates/trigger-github/tests/fixtures/<name>.json`. Move it there.
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
