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
