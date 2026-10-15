//! The pull request list: `github.review-requested`, `github.dependency-pr-opened`
//! and `github.pr-merged` all read the same answer. Pure.

use std::collections::BTreeSet;

use super::diff::{each_new, kind, Feed, Fire, Spec};
use super::json::{id_key, list, logins, num, text, BodyError, Value};
use super::payload;
use super::text::same_login;
use super::Armed;

/// Reads one answer of the pull request list and returns every firing, with
/// the keys it saw remembered in `feed`. It also adds every pull request
/// number to `prs`, for `github.pr-commented`, whether or not a rule needs the
/// answer. On a body that is not a list nothing changes.
pub fn fires(
    body: &str,
    feed: &mut Feed,
    prs: &mut BTreeSet<u64>,
    rules: &[&Armed],
    me: &str,
) -> Result<Vec<Fire>, BodyError> {
    let items = list(body, None)?;
    prs.extend(items.iter().map(|i| num(i, "/number")).filter(|n| *n != 0));
    let mut out = Vec::new();

    // Open and waiting on you.
    let wants_me = |v: &Value| {
        text(v, "/state") == "open"
            && logins(v, "/requested_reviewers")
                .iter()
                .any(|login| same_login(login, me))
    };
    let requested_key = |v: &Value| wants_me(v).then(|| id_key(v)).flatten();
    let always = |_: &Armed, _: &Value| true;
    let review = Spec {
        kind: kind::REVIEW_REQUESTED,
        key: &requested_key,
        accept: &always,
        build: payload::review,
    };
    each_new(feed, &review, rules, &items, &mut out);
    // A pull request that stops waiting on you is forgotten, so asking again fires again.
    if let Some(seen) = feed.seen.get_mut(kind::REVIEW_REQUESTED) {
        for item in items.iter().filter(|i| !wants_me(i)) {
            if let Some(key) = id_key(item) {
                seen.remove(&key);
            }
        }
    }

    // Opened by one of the rule's authors.
    let by_author = |rule: &Armed, v: &Value| {
        let author = text(v, "/user/login");
        rule.parsed
            .when
            .author
            .iter()
            .any(|wanted| same_login(wanted, author))
    };
    let dependency = Spec {
        kind: kind::DEPENDENCY_PR_OPENED,
        key: &id_key,
        accept: &by_author,
        build: payload::dependency,
    };
    each_new(feed, &dependency, rules, &items, &mut out);

    // Merged into a branch the rule matches.
    let merged_key = |v: &Value| {
        if text(v, "/merged_at").is_empty() {
            None
        } else {
            id_key(v)
        }
    };
    let into_branch = |rule: &Armed, v: &Value| rule.parsed.matches_branch(text(v, "/base/ref"));
    let merged = Spec {
        kind: kind::PR_MERGED,
        key: &merged_key,
        accept: &into_branch,
        build: payload::merged,
    };
    each_new(feed, &merged, rules, &items, &mut out);

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::testutil::{armed, edit, only, without, ME, PULLS};
    use serde_json::json;

    // Ids in tests/fixtures/pulls.json.
    const MERGED_TRUNK: u64 = 4830652819; // #14648
    const RENOVATE: u64 = 4829000001; // #14651
    const DEPENDABOT: u64 = 4829000002; // #14650
    const REVIEW_ME: u64 = 4829000003; // #14649
    const MERGED_RELEASE: u64 = 4829000004; // #14640
    const CLOSED_UNMERGED: u64 = 4829000005; // #14630
    const OLD_OPEN: u64 = 4829000006; // #14620

    struct World {
        feed: Feed,
        prs: BTreeSet<u64>,
        rules: Vec<Armed>,
    }

    impl World {
        fn new(rules: Vec<Armed>) -> World {
            World {
                feed: Feed::default(),
                prs: BTreeSet::new(),
                rules,
            }
        }

        fn poll(&mut self, body: &str) -> Vec<Fire> {
            let refs: Vec<&Armed> = self.rules.iter().collect();
            fires(body, &mut self.feed, &mut self.prs, &refs, ME).unwrap()
        }
    }

    fn rule(id: &str, kind: &str, extra: Value) -> Armed {
        let mut when = json!({ "repo": "cli/cli" });
        for (k, v) in extra.as_object().unwrap() {
            when[k] = v.clone();
        }
        armed(id, kind, when)
    }

    fn all_three() -> Vec<Armed> {
        vec![
            rule("p/review", kind::REVIEW_REQUESTED, json!({})),
            rule("p/deps", kind::DEPENDENCY_PR_OPENED, json!({})),
            rule("p/merged", kind::PR_MERGED, json!({})),
        ]
    }

    fn numbers(fires: &[Fire], rule: &str) -> Vec<u64> {
        fires
            .iter()
            .filter(|f| f.rule == rule)
            .map(|f| f.payload["number"].as_u64().unwrap())
            .collect()
    }

    #[test]
    fn new_pull_requests_fire_the_right_kinds_after_a_seed() {
        let mut w = World::new(all_three());
        let seed = only(PULLS, &[MERGED_RELEASE, CLOSED_UNMERGED, OLD_OPEN]);
        w.poll(&seed);
        let got = w.poll(PULLS);
        assert_eq!(numbers(&got, "p/review"), [14649]);
        assert_eq!(numbers(&got, "p/deps"), [14651, 14650]);
        assert_eq!(numbers(&got, "p/merged"), [14648]);
        assert_eq!(got.len(), 4);
        assert!(w.poll(PULLS).is_empty());
    }

    #[test]
    fn payloads_are_the_documented_shapes() {
        let mut w = World::new(all_three());
        let got = w.poll(&only(PULLS, &[MERGED_TRUNK, DEPENDABOT, REVIEW_ME]));
        let review = got.iter().find(|f| f.rule == "p/review").unwrap();
        assert_eq!(review.kind, kind::REVIEW_REQUESTED);
        assert_eq!(review.payload["head"], json!("octocat-flag"));
        assert_eq!(review.payload["base"], json!("trunk"));
        assert_eq!(review.payload["author"], json!("octocat"));
        let deps = got.iter().find(|f| f.rule == "p/deps").unwrap();
        assert_eq!(deps.payload["author"], json!("dependabot[bot]"));
        let merged = got.iter().find(|f| f.rule == "p/merged").unwrap();
        assert_eq!(merged.payload["merged_at"], json!("2026-10-12T16:14:17Z"));
    }

    #[test]
    fn review_requested_needs_an_open_pull_request_asking_me() {
        let mut w = World::new(vec![rule("p/r", kind::REVIEW_REQUESTED, json!({}))]);
        let got = w.poll(PULLS);
        assert_eq!(numbers(&got, "p/r"), [14649]);
        // #14648 asks sergiou87 but is closed; #14649 asks me and sergiou87.
        let mut w = World::new(vec![rule("p/r", kind::REVIEW_REQUESTED, json!({}))]);
        let closed = edit(PULLS, REVIEW_ME, |i| i["state"] = json!("closed"));
        assert!(w.poll(&closed).is_empty());
    }

    #[test]
    fn logins_are_compared_without_case() {
        let rules = [rule("p/r", kind::REVIEW_REQUESTED, json!({}))];
        let refs: Vec<&Armed> = rules.iter().collect();
        let (mut feed, mut prs) = (Feed::default(), BTreeSet::new());
        let got = fires(PULLS, &mut feed, &mut prs, &refs, "liamdotpro").unwrap();
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn a_new_request_after_it_was_withdrawn_fires_again() {
        let mut w = World::new(vec![rule("p/r", kind::REVIEW_REQUESTED, json!({}))]);
        assert_eq!(w.poll(PULLS).len(), 1);
        assert!(w.poll(PULLS).is_empty());
        let withdrawn = edit(PULLS, REVIEW_ME, |i| {
            i["requested_reviewers"] = json!([{ "login": "sergiou87" }]);
        });
        assert!(w.poll(&withdrawn).is_empty());
        assert_eq!(w.poll(PULLS).len(), 1);
    }

    #[test]
    fn a_pull_request_that_drops_out_of_the_page_is_not_forgotten() {
        let mut w = World::new(vec![rule("p/r", kind::REVIEW_REQUESTED, json!({}))]);
        assert_eq!(w.poll(PULLS).len(), 1);
        let page_without_it = without(PULLS, &[REVIEW_ME]);
        assert!(w.poll(&page_without_it).is_empty());
        assert!(w.poll(PULLS).is_empty());
    }

    #[test]
    fn merged_respects_the_branch_glob_on_the_base() {
        let mut w = World::new(vec![
            rule("p/any", kind::PR_MERGED, json!({})),
            rule("p/trunk", kind::PR_MERGED, json!({ "branch": "trunk" })),
            rule("p/rel", kind::PR_MERGED, json!({ "branch": "release/*" })),
        ]);
        let got = w.poll(PULLS);
        assert_eq!(numbers(&got, "p/any"), [14648, 14640]);
        assert_eq!(numbers(&got, "p/trunk"), [14648]);
        assert_eq!(numbers(&got, "p/rel"), [14640]);
    }

    #[test]
    fn a_closed_pull_request_that_was_not_merged_never_fires_merged() {
        let mut w = World::new(vec![rule("p/m", kind::PR_MERGED, json!({}))]);
        let got = w.poll(&only(PULLS, &[CLOSED_UNMERGED, OLD_OPEN]));
        assert!(got.is_empty());
    }

    #[test]
    fn dependency_prs_use_the_rules_author_list() {
        let mut w = World::new(vec![
            rule("p/default", kind::DEPENDENCY_PR_OPENED, json!({})),
            rule(
                "p/one",
                kind::DEPENDENCY_PR_OPENED,
                json!({ "author": ["DEPENDABOT[bot]"] }),
            ),
            rule(
                "p/human",
                kind::DEPENDENCY_PR_OPENED,
                json!({ "author": ["A4-Tacks"] }),
            ),
        ]);
        let got = w.poll(PULLS);
        assert_eq!(numbers(&got, "p/default"), [14651, 14650]);
        assert_eq!(numbers(&got, "p/one"), [14650]);
        assert_eq!(numbers(&got, "p/human"), [14648]);
    }

    #[test]
    fn two_rules_of_one_kind_both_fire_for_the_same_pull_request() {
        let mut w = World::new(vec![
            rule("p/a", kind::DEPENDENCY_PR_OPENED, json!({})),
            rule("p/b", kind::DEPENDENCY_PR_OPENED, json!({})),
        ]);
        let got = w.poll(&only(PULLS, &[RENOVATE]));
        let rules: Vec<&str> = got.iter().map(|f| f.rule.as_str()).collect();
        assert_eq!(rules, ["p/a", "p/b"]);
    }

    #[test]
    fn every_number_is_noted_even_with_no_rules() {
        let mut w = World::new(vec![]);
        assert!(w.poll(PULLS).is_empty());
        assert_eq!(w.prs.len(), 7);
        assert!(w.prs.contains(&14649));
        assert!(w.feed.seen.is_empty());
    }

    #[test]
    fn a_bad_body_changes_nothing() {
        let mut w = World::new(all_three());
        let refs: Vec<&Armed> = w.rules.iter().collect();
        let err = fires(
            "{\"message\":\"Not Found\"}",
            &mut w.feed,
            &mut w.prs,
            &refs,
            ME,
        );
        assert!(err.is_err());
        assert!(w.prs.is_empty());
        assert!(w.feed.seen.is_empty());
    }
}
