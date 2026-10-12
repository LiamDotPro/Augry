//! Tests for `Poller::tick` when GitHub pushes back: rate limits, errors, a
//! lost login, a missing repository, and answers that make no sense.

use serde_json::json;

use super::*;
use crate::logic::diff::kind;
use crate::logic::fake_gh::{http, not_modified, ok, ok_with, transport, FakeGh};
use crate::logic::response::NOT_LOGGED_IN;
use crate::logic::testutil::{armed, only, ME, PULLS, RELEASES};

const REPO: &str = "cli/cli";
/// 2026-10-12T00:00:00Z.
const START: u64 = 1_791_763_200;

fn releases() -> String {
    path_for(REPO, Group::Releases, ME, None, None)
}

fn pulls() -> String {
    path_for(REPO, Group::Pulls, ME, None, None)
}

/// One release rule: one request, `releases()`.
fn release_poller() -> Poller {
    let rule = armed(
        "p/rel",
        kind::RELEASE_PUBLISHED,
        json!({ "repo": REPO, "interval_s": 60 }),
    );
    Poller::new(vec![rule], START)
}

fn seed_body() -> String {
    only(RELEASES, &[399674740, 399674700])
}

fn tick(poller: &mut Poller, now: u64, gh: &FakeGh) -> Outcome {
    poller.tick(now, gh, &|| false)
}

fn calls_at(gh: &FakeGh) -> usize {
    gh.calls().len()
}

#[test]
fn a_rate_limit_stops_every_request_until_the_wait_is_over() {
    let gh = FakeGh::new(Some(ME));
    // Every group needs an answer; only the pull request list is limited.
    let rules: Vec<Armed> = crate::KINDS
        .iter()
        .map(|k| armed(&format!("p/{}", k.id), k.id, json!({ "repo": REPO })))
        .collect();
    let paths: Vec<String> = plan(&rules, ME, &iso(START))
        .iter()
        .map(|j| j.path.clone())
        .collect();
    for path in &paths {
        gh.serve(path, ok("[]", "W/\"seed\""));
    }
    gh.serve(&paths[1], ok("{\"workflow_runs\":[]}", "W/\"runs\""));
    gh.serve(&paths[0], http(403, &[("retry-after", "90")], "{}"));
    gh.serve(&paths[0], ok("[]", "W/\"after\""));
    let mut poller = Poller::new(rules, START);

    tick(&mut poller, START, &gh);
    assert_eq!(calls_at(&gh), 6);

    let limited = tick(&mut poller, START + 120, &gh);
    assert_eq!(calls_at(&gh), 7, "the rest of the round waits");
    assert_eq!(limited.notes.len(), 1);
    assert!(limited.notes[0].text.contains("90 seconds"));
    assert_eq!(limited.notes[0].rules.len(), 4, "the rules on that request");

    for second in [121, 150, 209] {
        tick(&mut poller, START + second, &gh);
    }
    assert_eq!(calls_at(&gh), 7, "nothing is asked during the wait");

    tick(&mut poller, START + 210, &gh);
    assert_eq!(calls_at(&gh), 13, "all six requests were due and are made");
    let calls = gh.calls();
    assert_eq!(calls[7].0, paths[0], "pull requests are first again");
    assert_eq!(calls[7].1.as_deref(), Some("W/\"seed\""));
}

#[test]
fn transport_errors_back_off_60_120_240_seconds_and_say_nothing() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), ok(&seed_body(), "W/\"seed\""));
    gh.serve(
        &releases(),
        transport("gh exited with code 1 and gave no HTTP status"),
    );
    let mut poller = release_poller();
    tick(&mut poller, START, &gh);
    assert_eq!(calls_at(&gh), 1);

    let mut asked = Vec::new();
    for second in 60..=480 {
        let before = calls_at(&gh);
        let out = tick(&mut poller, START + second, &gh);
        assert!(out.notes.is_empty() && out.fires.is_empty());
        if calls_at(&gh) > before {
            asked.push(second);
        }
    }
    // Asked at 60 (wait 60), 120 (wait 120), 240 (wait 240), then 480.
    assert_eq!(asked, [60, 120, 240, 480]);
}

#[test]
fn a_good_answer_ends_the_backoff() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), ok(&seed_body(), "W/\"seed\""));
    gh.serve(&releases(), transport("offline"));
    gh.serve(&releases(), not_modified());
    let mut poller = release_poller();
    tick(&mut poller, START, &gh);
    tick(&mut poller, START + 60, &gh);
    assert_eq!(calls_at(&gh), 2);
    tick(&mut poller, START + 119, &gh);
    assert_eq!(calls_at(&gh), 2, "waiting 60 seconds");
    tick(&mut poller, START + 120, &gh);
    assert_eq!(calls_at(&gh), 3);
    // The 304 reset the ladder, so the next request is one interval later.
    tick(&mut poller, START + 179, &gh);
    assert_eq!(calls_at(&gh), 3);
    tick(&mut poller, START + 180, &gh);
    assert_eq!(calls_at(&gh), 4);
}

#[test]
fn a_401_stops_the_poller_for_good_and_names_every_rule() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), ok(&seed_body(), "W/\"seed\""));
    gh.serve(
        &releases(),
        http(401, &[], "{\"message\":\"Bad credentials\"}"),
    );
    let mut poller = release_poller();
    tick(&mut poller, START, &gh);
    assert!(!poller.is_stopped());
    let out = tick(&mut poller, START + 60, &gh);
    assert!(poller.is_stopped());
    assert_eq!(out.notes.len(), 1);
    assert_eq!(out.notes[0].rules, ["p/rel"]);
    assert!(out.notes[0].text.contains("gh auth login"));
    assert!(!out.notes[0].text.contains("Bad credentials"));
    let before = calls_at(&gh);
    for second in [61, 200, 100_000] {
        let again = tick(&mut poller, START + second, &gh);
        assert!(again.fires.is_empty() && again.notes.is_empty());
    }
    assert_eq!(calls_at(&gh), before);
}

#[test]
fn gh_reporting_no_login_stops_the_poller_too() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), transport(NOT_LOGGED_IN));
    let mut poller = release_poller();
    tick(&mut poller, START, &gh);
    assert!(poller.is_stopped());
}

#[test]
fn a_missing_repository_is_reported_once_and_asked_five_times_less_often() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), ok(&seed_body(), "W/\"seed\""));
    gh.serve(&releases(), http(404, &[], "{\"message\":\"Not Found\"}"));
    gh.serve(&releases(), ok(&seed_body(), "W/\"back\""));
    let mut poller = release_poller();
    tick(&mut poller, START, &gh);

    let first = tick(&mut poller, START + 60, &gh);
    assert_eq!(first.notes.len(), 1);
    assert_eq!(first.notes[0].rules, ["p/rel"]);
    assert!(first.notes[0].text.contains("404"));
    assert!(!first.notes[0].text.contains("Not Found"));
    assert_eq!(calls_at(&gh), 2);

    // 60 seconds times 5 is 300.
    tick(&mut poller, START + 359, &gh);
    assert_eq!(calls_at(&gh), 2);
    tick(&mut poller, START + 360, &gh);
    assert_eq!(calls_at(&gh), 3);

    // The repository is back: normal interval again.
    tick(&mut poller, START + 419, &gh);
    assert_eq!(calls_at(&gh), 3);
    tick(&mut poller, START + 420, &gh);
    assert_eq!(calls_at(&gh), 4);
}

#[test]
fn a_404_that_repeats_is_not_reported_again() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), http(404, &[], "{}"));
    let mut poller = release_poller();
    let mut notes = 0;
    for second in [0, 300, 600, 900] {
        notes += tick(&mut poller, START + second, &gh).notes.len();
    }
    assert_eq!(calls_at(&gh), 4);
    assert_eq!(notes, 1);
}

#[test]
fn a_plain_403_is_reported_once_and_does_not_slow_the_polling() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), ok(&seed_body(), "W/\"seed\""));
    gh.serve(
        &releases(),
        http(403, &[], "{\"message\":\"Resource protected by SSO\"}"),
    );
    let mut poller = release_poller();
    tick(&mut poller, START, &gh);
    let first = tick(&mut poller, START + 60, &gh);
    assert_eq!(first.notes.len(), 1);
    assert!(first.notes[0].text.contains("403"));
    assert!(!first.notes[0].text.contains("SSO"));
    let second = tick(&mut poller, START + 120, &gh);
    assert!(second.notes.is_empty());
    assert_eq!(calls_at(&gh), 3);
}

#[test]
fn x_poll_interval_raises_the_interval() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(
        &releases(),
        ok_with(&seed_body(), "W/\"seed\"", &[("x-poll-interval", "300")]),
    );
    let mut poller = release_poller();
    tick(&mut poller, START, &gh);
    tick(&mut poller, START + 299, &gh);
    assert_eq!(calls_at(&gh), 1);
    tick(&mut poller, START + 300, &gh);
    assert_eq!(calls_at(&gh), 2);
}

#[test]
fn few_requests_left_stretch_the_interval_four_times() {
    let gh = FakeGh::new(Some(ME));
    let reset = (START + 3600).to_string();
    gh.serve(
        &releases(),
        ok_with(
            &seed_body(),
            "W/\"seed\"",
            &[
                ("x-ratelimit-remaining", "50"),
                ("x-ratelimit-reset", &reset),
            ],
        ),
    );
    let mut poller = release_poller();
    tick(&mut poller, START, &gh);
    tick(&mut poller, START + 239, &gh);
    assert_eq!(calls_at(&gh), 1);
    tick(&mut poller, START + 240, &gh);
    assert_eq!(calls_at(&gh), 2);
}

#[test]
fn an_unreadable_answer_is_reported_once_without_its_text_and_no_etag_is_kept() {
    let gh = FakeGh::new(Some(ME));
    gh.serve(&releases(), ok("{\"message\":\"oops\"}", "W/\"bad\""));
    gh.serve(&releases(), ok("{\"message\":\"oops\"}", "W/\"bad\""));
    gh.serve(&releases(), ok(&seed_body(), "W/\"seed\""));
    gh.serve(&releases(), ok(RELEASES, "W/\"news\""));
    let mut poller = release_poller();
    let first = tick(&mut poller, START, &gh);
    assert_eq!(first.notes.len(), 1);
    assert!(!first.notes[0].text.contains("oops"));
    let second = tick(&mut poller, START + 60, &gh);
    assert!(second.notes.is_empty(), "reported once");
    let third = tick(&mut poller, START + 120, &gh);
    assert!(third.fires.is_empty(), "this answer only seeds");
    let fourth = tick(&mut poller, START + 180, &gh);
    assert_eq!(fourth.fires.len(), 1);
    let sent: Vec<Option<String>> = gh.calls().into_iter().map(|(_, etag)| etag).collect();
    assert_eq!(
        sent[..3],
        [None, None, None],
        "a bad answer's etag is not kept"
    );
    assert_eq!(sent[3].as_deref(), Some("W/\"seed\""));
}

#[test]
fn a_failed_login_is_retried_every_minute_and_nothing_else_is_asked() {
    let gh = FakeGh::new(None);
    let mut poller = release_poller();
    tick(&mut poller, START, &gh);
    assert_eq!(gh.logins(), 1);
    tick(&mut poller, START + 59, &gh);
    assert_eq!(gh.logins(), 1);
    tick(&mut poller, START + 60, &gh);
    assert_eq!(gh.logins(), 2);
    assert!(gh.calls().is_empty());
    assert!(poller.jobs().is_empty());

    gh.set_me(Some(ME));
    gh.serve(&releases(), ok(&seed_body(), "W/\"seed\""));
    tick(&mut poller, START + 120, &gh);
    assert_eq!(gh.logins(), 3);
    assert_eq!(calls_at(&gh), 1);
    assert_eq!(poller.jobs().len(), 1);
}

#[test]
fn a_pull_request_rule_fires_through_the_whole_path() {
    // The pull request fixtures through `tick`: a seed, then news.
    let gh = FakeGh::new(Some(ME));
    gh.serve(&pulls(), ok(&only(PULLS, &[4829000004]), "W/\"seed\""));
    gh.serve(&pulls(), ok(PULLS, "W/\"news\""));
    let rule = armed(
        "p/review",
        kind::REVIEW_REQUESTED,
        json!({ "repo": REPO, "interval_s": 60 }),
    );
    let mut poller = Poller::new(vec![rule], START);
    tick(&mut poller, START, &gh);
    let out = tick(&mut poller, START + 60, &gh);
    assert_eq!(out.fires.len(), 1);
    assert_eq!(out.fires[0].rule, "p/review");
}
