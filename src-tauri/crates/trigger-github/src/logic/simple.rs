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
