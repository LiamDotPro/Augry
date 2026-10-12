---
id: T17
epic: 011-trigger-github
title: Fire CI, release and issue-assigned kinds
status: todo
depends_on: [T16]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-github/src/logic/runs.rs
  - src-tauri/crates/trigger-github/src/logic/simple.rs
  - src-tauri/crates/trigger-github/src/logic.rs
---

## Goal

The crate has `logic/runs.rs` (`github.ci-failed` and `github.ci-passed`, keyed by `id:run_attempt`, filtered by the rule's branch glob on `head_branch`) and `logic/simple.rs` (`releases` for `github.release-published`, `issues` for `github.issue-assigned`). 13 tests use the fixtures.

## Read first

- `plans/011-trigger-github/research/R01-endpoints.md` the "Fire when" table rows for ci-failed, ci-passed, release-published and issue-assigned
- `src-tauri/crates/trigger-github/src/logic/pulls.rs` the same pattern for another group (created by T16)
- `src-tauri/crates/trigger-github/src/logic/diff.rs` `each_new`, `Spec` (created by T14)
- `src-tauri/crates/trigger-github/src/logic/testutil.rs` the helpers the tests use (created by T15)

## Steps

1. Create `src-tauri/crates/trigger-github/src/logic/runs.rs` with exactly this.

A run is a failure when its `conclusion` is `failure` or `timed_out`, and a pass when it is `success`. `cancelled`, `skipped` and the rest fire nothing. The seen key is `id:run_attempt`, because a re-run keeps the `id` and raises the attempt, and each attempt is its own result. A missing attempt counts as 1. A run whose `head_branch` is null matches only a rule with no `branch`.

```rust
//! The workflow run list: `github.ci-failed` and `github.ci-passed`. Pure.

use super::diff::{each_new, kind, Feed, Fire, Spec};
use super::json::{id_key, list, num, text, BodyError, Value};
use super::payload;
use super::Armed;

/// The key of a run: `id:run_attempt`, because a re-run keeps the id and bumps
/// the attempt, and each attempt is its own result. A missing attempt is 1.
fn run_key(v: &Value) -> Option<String> {
    let attempt = num(v, "/run_attempt").max(1);
    id_key(v).map(|id| format!("{id}:{attempt}"))
}

/// Reads one answer of `actions/runs?status=completed`. A run counts as a
/// failure when it `failure`d or `timed_out`, and as a pass when it was a
/// `success`. Cancelled, skipped and other endings fire nothing.
pub fn fires(body: &str, feed: &mut Feed, rules: &[&Armed]) -> Result<Vec<Fire>, BodyError> {
    let items = list(body, Some("workflow_runs"))?;
    let mut out = Vec::new();
    let failed_key = |v: &Value| match text(v, "/conclusion") {
        "failure" | "timed_out" => run_key(v),
        _ => None,
    };
    let passed_key = |v: &Value| match text(v, "/conclusion") {
        "success" => run_key(v),
        _ => None,
    };
    // `head_branch` is null for some runs; it then matches only a rule with no branch.
    let on_branch = |rule: &Armed, v: &Value| rule.parsed.matches_branch(text(v, "/head_branch"));
    let failed = Spec {
        kind: kind::CI_FAILED,
        key: &failed_key,
        accept: &on_branch,
        build: payload::ci,
    };
    let passed = Spec {
        kind: kind::CI_PASSED,
        key: &passed_key,
        accept: &on_branch,
        build: payload::ci,
    };
    each_new(feed, &failed, rules, &items, &mut out);
    each_new(feed, &passed, rules, &items, &mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::testutil::{armed, edit, only, RUNS};
    use serde_json::json;

    // Ids in tests/fixtures/runs.json, newest first.
    const FAIL_TRUNK: u64 = 38303985560;
    const PASS_TRUNK: u64 = 38303985559;
    const FAIL_FEATURE: u64 = 38303985558;
    const TIMED_OUT_RELEASE: u64 = 38303985557;
    const CANCELLED: u64 = 38303985556;
    const PASS_OLD: u64 = 38303985555;

    fn rule(id: &str, kind: &str, branch: Option<&str>) -> Armed {
        let mut when = json!({ "repo": "cli/cli" });
        if let Some(b) = branch {
            when["branch"] = json!(b);
        }
        armed(id, kind, when)
    }

    fn poll(feed: &mut Feed, rules: &[Armed], body: &str) -> Vec<Fire> {
        let refs: Vec<&Armed> = rules.iter().collect();
        fires(body, feed, &refs).unwrap()
    }

    fn ids(fires: &[Fire], rule: &str) -> Vec<u64> {
        fires
            .iter()
            .filter(|f| f.rule == rule)
            .map(|f| f.payload["id"].as_u64().unwrap())
            .collect()
    }

    #[test]
    fn failures_and_passes_fire_once_after_a_seed() {
        let rules = [
            rule("p/fail", kind::CI_FAILED, None),
            rule("p/pass", kind::CI_PASSED, None),
        ];
        let mut feed = Feed::default();
        poll(&mut feed, &rules, &only(RUNS, &[PASS_OLD, CANCELLED]));
        let got = poll(&mut feed, &rules, RUNS);
        assert_eq!(
            ids(&got, "p/fail"),
            [FAIL_TRUNK, FAIL_FEATURE, TIMED_OUT_RELEASE]
        );
        assert_eq!(ids(&got, "p/pass"), [PASS_TRUNK]);
        assert!(poll(&mut feed, &rules, RUNS).is_empty());
    }

    #[test]
    fn cancelled_runs_never_fire() {
        let rules = [
            rule("p/fail", kind::CI_FAILED, None),
            rule("p/pass", kind::CI_PASSED, None),
        ];
        let mut feed = Feed::default();
        assert!(poll(&mut feed, &rules, &only(RUNS, &[CANCELLED])).is_empty());
    }

    #[test]
    fn the_branch_glob_filters_on_head_branch() {
        let rules = [
            rule("p/trunk", kind::CI_FAILED, Some("trunk")),
            rule("p/rel", kind::CI_FAILED, Some("release/*")),
            rule("p/feat", kind::CI_FAILED, Some("feature/*")),
        ];
        let got = poll(&mut Feed::default(), &rules, RUNS);
        assert_eq!(ids(&got, "p/trunk"), [FAIL_TRUNK]);
        assert_eq!(ids(&got, "p/rel"), [TIMED_OUT_RELEASE]);
        assert_eq!(ids(&got, "p/feat"), [FAIL_FEATURE]);
    }

    #[test]
    fn a_run_without_a_branch_matches_only_a_rule_without_one() {
        let rules = [
            rule("p/any", kind::CI_FAILED, None),
            rule("p/trunk", kind::CI_FAILED, Some("trunk")),
        ];
        let body = edit(RUNS, FAIL_TRUNK, |i| i["head_branch"] = json!(null));
        let got = poll(&mut Feed::default(), &rules, &body);
        assert!(ids(&got, "p/any").contains(&FAIL_TRUNK));
        assert!(!ids(&got, "p/trunk").contains(&FAIL_TRUNK));
    }

    #[test]
    fn a_re_run_is_a_new_result_and_the_other_kind_fires() {
        let rules = [
            rule("p/fail", kind::CI_FAILED, Some("trunk")),
            rule("p/pass", kind::CI_PASSED, Some("trunk")),
        ];
        let mut feed = Feed::default();
        poll(&mut feed, &rules, RUNS);
        let passed_on_retry = edit(RUNS, FAIL_TRUNK, |i| {
            i["run_attempt"] = json!(2);
            i["conclusion"] = json!("success");
        });
        let got = poll(&mut feed, &rules, &passed_on_retry);
        assert_eq!(ids(&got, "p/pass"), [FAIL_TRUNK]);
        assert!(ids(&got, "p/fail").is_empty());
        let failed_again = edit(RUNS, FAIL_TRUNK, |i| i["run_attempt"] = json!(3));
        let got = poll(&mut feed, &rules, &failed_again);
        assert_eq!(ids(&got, "p/fail"), [FAIL_TRUNK]);
    }

    #[test]
    fn the_same_attempt_is_never_fired_twice_even_if_it_moves_in_the_list() {
        let rules = [rule("p/fail", kind::CI_FAILED, None)];
        let mut feed = Feed::default();
        assert_eq!(poll(&mut feed, &rules, RUNS).len(), 3);
        let moved = only(RUNS, &[FAIL_FEATURE, FAIL_TRUNK]);
        assert!(poll(&mut feed, &rules, &moved).is_empty());
    }

    #[test]
    fn the_payload_has_the_run_fields() {
        let rules = [rule("p/fail", kind::CI_FAILED, Some("trunk"))];
        let got = poll(&mut Feed::default(), &rules, RUNS);
        let p = &got[0].payload;
        assert_eq!(got[0].kind, kind::CI_FAILED);
        assert_eq!(p["name"], json!("CI"));
        assert_eq!(p["branch"], json!("trunk"));
        assert_eq!(p["conclusion"], json!("failure"));
        assert_eq!(p["event"], json!("push"));
        assert_eq!(p["actor"], json!("BagToad"));
        assert_eq!(p["sha"], json!("a1b2c3d4e5f60718293a4b5c6d7e8f9012345678"));
    }

    #[test]
    fn a_body_without_workflow_runs_is_an_error() {
        let rules = [rule("p/fail", kind::CI_FAILED, None)];
        let refs: Vec<&Armed> = rules.iter().collect();
        let mut feed = Feed::default();
        assert!(fires("[]", &mut feed, &refs).is_err());
        assert!(feed.seen.is_empty());
    }
}
```

2. Create `src-tauri/crates/trigger-github/src/logic/simple.rs` with exactly this.

A release fires when it is not a draft and its `id` is new, so a draft fires when it is published. An issue fires when it has no `pull_request` key (the issues list also holds pull requests) and its `id` is new. The request is already limited to issues assigned to you, so there is no assignee check here.

```rust
//! The two lists that need no filtering beyond "have we seen it": releases
//! (`github.release-published`) and issues assigned to you
//! (`github.issue-assigned`). Pure.

use super::diff::{each_new, kind, Feed, Fire, Spec};
use super::json::{flag, has, id_key, list, BodyError, Value};
use super::payload;
use super::Armed;

fn anyone(_: &Armed, _: &Value) -> bool {
    true
}

/// Reads one answer of `releases`. A draft is not published yet; it fires when
/// it stops being a draft.
pub fn releases(body: &str, feed: &mut Feed, rules: &[&Armed]) -> Result<Vec<Fire>, BodyError> {
    let items = list(body, None)?;
    let published_key = |v: &Value| if flag(v, "/draft") { None } else { id_key(v) };
    let spec = Spec {
        kind: kind::RELEASE_PUBLISHED,
        key: &published_key,
        accept: &anyone,
        build: payload::release,
    };
    let mut out = Vec::new();
    each_new(feed, &spec, rules, &items, &mut out);
    Ok(out)
}

/// Reads one answer of `issues?state=open&assignee=<me>`. That list also holds
/// pull requests, which carry a `pull_request` key and are skipped.
pub fn issues(body: &str, feed: &mut Feed, rules: &[&Armed]) -> Result<Vec<Fire>, BodyError> {
    let items = list(body, None)?;
    let issue_key = |v: &Value| {
        if has(v, "/pull_request") {
            None
        } else {
            id_key(v)
        }
    };
    let spec = Spec {
        kind: kind::ISSUE_ASSIGNED,
        key: &issue_key,
        accept: &anyone,
        build: payload::issue,
    };
    let mut out = Vec::new();
    each_new(feed, &spec, rules, &items, &mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::testutil::{armed, edit, only, ISSUES, RELEASES};
    use serde_json::json;

    fn rule(kind: &str) -> Armed {
        armed("p/t", kind, json!({ "repo": "cli/cli" }))
    }

    fn tags(fires: &[Fire]) -> Vec<String> {
        fires
            .iter()
            .map(|f| f.payload["tag"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn a_new_release_fires_once_and_a_draft_not_at_all() {
        let rules = [rule(kind::RELEASE_PUBLISHED)];
        let refs: Vec<&Armed> = rules.iter().collect();
        let mut feed = Feed::default();
        let seed = only(RELEASES, &[399674740, 399674700]);
        releases(&seed, &mut feed, &refs).unwrap();
        let got = releases(RELEASES, &mut feed, &refs).unwrap();
        assert_eq!(tags(&got), ["v2.103.0-rc.1"]);
        assert_eq!(got[0].payload["prerelease"], json!("true"));
        assert_eq!(got[0].payload["author"], json!("github-actions[bot]"));
        assert!(releases(RELEASES, &mut feed, &refs).unwrap().is_empty());
    }

    #[test]
    fn a_draft_fires_when_it_is_published() {
        let rules = [rule(kind::RELEASE_PUBLISHED)];
        let refs: Vec<&Armed> = rules.iter().collect();
        let mut feed = Feed::default();
        releases(RELEASES, &mut feed, &refs).unwrap();
        let published = edit(RELEASES, 399674745, |i| {
            i["draft"] = json!(false);
            i["published_at"] = json!("2026-10-12T21:00:00Z");
        });
        let got = releases(&published, &mut feed, &refs).unwrap();
        assert_eq!(tags(&got), ["v2.102.1"]);
        assert_eq!(got[0].payload["name"], json!("v2.102.1"));
    }

    #[test]
    fn a_new_issue_fires_and_a_pull_request_does_not() {
        let rules = [rule(kind::ISSUE_ASSIGNED)];
        let refs: Vec<&Armed> = rules.iter().collect();
        let mut feed = Feed::default();
        issues(&only(ISSUES, &[5741818100]), &mut feed, &refs).unwrap();
        let got = issues(ISSUES, &mut feed, &refs).unwrap();
        assert_eq!(got.len(), 1);
        let p = &got[0].payload;
        assert_eq!(p["number"], json!(14660));
        assert_eq!(p["title"], json!("Crash when the config file is empty"));
        assert_eq!(p["author"], json!("octocat"));
        assert_eq!(p["assignees"], json!("LiamDotPro"));
        assert!(issues(ISSUES, &mut feed, &refs).unwrap().is_empty());
    }

    #[test]
    fn a_pull_request_in_the_issue_list_is_never_remembered() {
        let rules = [rule(kind::ISSUE_ASSIGNED)];
        let refs: Vec<&Armed> = rules.iter().collect();
        let mut feed = Feed::default();
        issues(&only(ISSUES, &[5741818299]), &mut feed, &refs).unwrap();
        assert!(feed.seen[kind::ISSUE_ASSIGNED].is_empty());
    }

    #[test]
    fn other_kinds_and_bad_bodies_are_left_alone() {
        let rules = [rule(kind::CI_FAILED)];
        let refs: Vec<&Armed> = rules.iter().collect();
        let mut feed = Feed::default();
        assert!(releases(RELEASES, &mut feed, &refs).unwrap().is_empty());
        assert!(issues(ISSUES, &mut feed, &refs).unwrap().is_empty());
        assert!(feed.seen.is_empty());
        assert!(releases("{}", &mut feed, &refs).is_err());
        assert!(issues("nope", &mut feed, &refs).is_err());
    }
}
```

3. Edit `logic.rs` so it declares the two modules. Replace the line `pub mod response;` with the three lines below. Use exactly this edit in `src-tauri/crates/trigger-github/src/logic.rs`.

Find this text, which appears once:

```rust
pub mod response;
```

Replace it with:

```rust
pub mod response;
pub mod runs;
pub mod simple;
```

4. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib` passes with 94 tests (13 new in this card)
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (the `source`, `readme` and `live` tests included)
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (warnings that `lib.rs` and `os.rs` still have template code are expected until slice C)
- [ ] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- Un-assignment: an issue that is unassigned and assigned again does not fire again. That is a known limit, not a bug to fix here.
- Changing `payload.rs`, `diff.rs` or the fixtures.
- Removing `#![allow(dead_code)]` from `logic.rs`.

## If stuck

- If a test fails, compare the file with step 1 and 2 character by character. Do not change a test or a fixture.
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
