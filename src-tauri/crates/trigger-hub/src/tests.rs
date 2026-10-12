//! Tests for the hub. Each card adds its own module below.

mod catalogue_tests {
    use crate::{Fired, Hub, RuleState};
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::time::UNIX_EPOCH;
    use trigger_core::testkit::FakeSource;
    use trigger_core::{Availability, Event, KindInfo, Platforms};

    static KINDS: [KindInfo; 1] = [KindInfo {
        id: "fake.x",
        title: "X",
        summary: "s",
        platforms: Platforms::ALL,
        permission: None,
        when: &[],
        payload: &[],
    }];

    #[test]
    fn catalogue_lists_every_source() {
        let first = FakeSource::new("fake", &KINDS);
        let mut second = FakeSource::new("other", &[]);
        second.availability = Availability::Unsupported {
            reason: "not here".to_string(),
        };
        let hub = Hub::new(vec![Box::new(first), Box::new(second)]);
        let list = hub.catalogue();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "fake");
        assert_eq!(list[0].kinds.len(), 1);
        assert_eq!(list[0].kinds[0].id, "fake.x");
        assert_eq!(list[0].availability, Availability::Available);
        assert_eq!(list[1].id, "other");
        assert!(list[1].kinds.is_empty());
        assert_eq!(
            list[1].availability,
            Availability::Unsupported {
                reason: "not here".to_string()
            }
        );
    }

    #[test]
    fn rule_state_json_is_pinned() {
        assert_eq!(
            serde_json::to_value(RuleState::Active).unwrap(),
            json!({"state": "active"})
        );
        assert_eq!(
            serde_json::to_value(RuleState::Rejected { reason: "r".into() }).unwrap(),
            json!({"state": "rejected", "reason": "r"})
        );
        assert_eq!(
            serde_json::to_value(RuleState::SourceFailed { reason: "r".into() }).unwrap(),
            json!({"state": "sourceFailed", "reason": "r"})
        );
    }

    #[test]
    fn fired_json_is_pinned() {
        let fired = Fired {
            event: Event {
                rule: "r1".into(),
                kind: "fake.x".into(),
                at: UNIX_EPOCH,
                payload: BTreeMap::new(),
            },
            coalesced: 3,
        };
        assert_eq!(
            serde_json::to_value(fired).unwrap(),
            json!({"event": {"rule": "r1", "kind": "fake.x", "at": 0, "payload": {}}, "coalesced": 3})
        );
    }
}

mod publish_tests {
    use crate::Hub;
    use std::collections::BTreeMap;
    use std::sync::mpsc::Receiver;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use trigger_core::Event;

    use crate::Fired;

    const WAIT: Duration = Duration::from_secs(5);

    fn event(rule: &str, secs: u64) -> Event {
        Event {
            rule: rule.to_string(),
            kind: "fake.x".to_string(),
            at: UNIX_EPOCH + Duration::from_secs(secs),
            payload: BTreeMap::new(),
        }
    }

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn nothing_more(rx: &Receiver<Fired>) -> bool {
        rx.recv_timeout(Duration::from_millis(200)).is_err()
    }

    #[test]
    fn publish_reaches_a_subscriber() {
        let hub = Hub::new(vec![]);
        let rx = hub.subscribe();
        hub.publish(event("r1", 100));
        let fired = rx.recv_timeout(WAIT).unwrap();
        assert_eq!(fired.event.rule, "r1");
        assert_eq!(fired.coalesced, 0);
    }

    #[test]
    fn publish_reaches_every_subscriber() {
        let hub = Hub::new(vec![]);
        let a = hub.subscribe();
        let b = hub.subscribe();
        hub.publish(event("r1", 100));
        assert_eq!(a.recv_timeout(WAIT).unwrap().event.rule, "r1");
        assert_eq!(b.recv_timeout(WAIT).unwrap().event.rule, "r1");
    }

    #[test]
    fn a_dropped_subscriber_does_not_stop_the_others() {
        let hub = Hub::new(vec![]);
        let gone = hub.subscribe();
        let kept = hub.subscribe();
        drop(gone);
        hub.publish(event("r1", 100));
        assert!(kept.recv_timeout(WAIT).is_ok());
        hub.publish(event("r2", 100));
        assert_eq!(kept.recv_timeout(WAIT).unwrap().event.rule, "r2");
    }

    #[test]
    fn cool_down_drops_and_counts_and_the_next_fired_carries_the_count() {
        let hub = Hub::new(vec![]);
        let rx = hub.subscribe();
        hub.publish(event("r1", 100));
        hub.publish(event("r1", 101));
        hub.publish(event("r1", 101));
        hub.publish(event("r1", 103));
        let first = rx.recv_timeout(WAIT).unwrap();
        assert_eq!(first.event.at, at(100));
        assert_eq!(first.coalesced, 0);
        let second = rx.recv_timeout(WAIT).unwrap();
        assert_eq!(second.event.at, at(103));
        assert_eq!(second.coalesced, 2);
        assert!(nothing_more(&rx));
        // The count was reset.
        hub.publish(event("r1", 106));
        assert_eq!(rx.recv_timeout(WAIT).unwrap().coalesced, 0);
    }

    #[test]
    fn rules_cool_down_separately() {
        let hub = Hub::new(vec![]);
        let rx = hub.subscribe();
        hub.publish(event("r1", 100));
        hub.publish(event("r2", 100));
        assert_eq!(rx.recv_timeout(WAIT).unwrap().event.rule, "r1");
        assert_eq!(rx.recv_timeout(WAIT).unwrap().event.rule, "r2");
    }

    #[test]
    fn with_cool_down_replaces_the_default() {
        let hub = Hub::new(vec![]).with_cool_down(Duration::ZERO);
        let rx = hub.subscribe();
        hub.publish(event("r1", 100));
        hub.publish(event("r1", 100));
        assert_eq!(rx.recv_timeout(WAIT).unwrap().coalesced, 0);
        assert_eq!(rx.recv_timeout(WAIT).unwrap().coalesced, 0);
    }

    #[test]
    fn dropping_the_hub_stops_the_thread_quickly() {
        let hub = Hub::new(vec![]);
        let rx = hub.subscribe();
        let started = std::time::Instant::now();
        drop(hub);
        assert!(started.elapsed() < Duration::from_secs(2));
        // The thread is gone, so its sender to this subscriber is gone too.
        assert!(rx.recv_timeout(Duration::from_secs(2)).is_err());
    }
}

mod check_tests {
    use crate::test_support::shared;
    use crate::Hub;
    use serde_json::json;
    use trigger_core::testkit::{rule, FakeSource};
    use trigger_core::RuleError;

    fn hub() -> Hub {
        let (fs, _, _) = shared(FakeSource::new("fs", &[]), true);
        let (git, _, _) = shared(FakeSource::new("git", &[]), false);
        Hub::new(vec![fs, git])
    }

    #[test]
    fn a_good_rule_passes_the_owning_source() {
        assert!(hub().check(&rule("r1", "fs.file-saved", json!({}))).is_ok());
        assert!(hub().check(&rule("r2", "git.commit", json!({}))).is_ok());
    }

    #[test]
    fn the_owning_source_can_reject_a_rule() {
        let err = hub()
            .check(&rule("r1", "fs.file-saved", json!({"bad": 1})))
            .unwrap_err();
        assert_eq!(err.to_string(), "field bad: not allowed");
        // The git source in this test does not reject "bad".
        assert!(hub()
            .check(&rule("r2", "git.commit", json!({"bad": 1})))
            .is_ok());
    }

    #[test]
    fn a_kind_no_source_owns_is_unknown() {
        let err = hub().check(&rule("r1", "other.x", json!({}))).unwrap_err();
        assert!(matches!(err, RuleError::UnknownKind(ref k) if k == "other.x"));
    }

    #[test]
    fn only_the_part_before_the_first_dot_picks_the_source() {
        // "fsx.y" must not match the source "fs", and "fs" alone is not "fs.".
        assert!(hub().check(&rule("r1", "fsx.y", json!({}))).is_err());
        assert!(hub().check(&rule("r2", "fs", json!({}))).is_ok());
        assert!(hub().check(&rule("r3", "fs.a.b", json!({}))).is_ok());
    }
}

mod apply_tests {
    use crate::test_support::shared;
    use crate::{Hub, RuleState};
    use serde_json::json;
    use trigger_core::testkit::{rule, FakeSource};
    use trigger_core::{Availability, Permission};

    fn ids(rules: &[trigger_core::Rule]) -> Vec<&str> {
        rules.iter().map(|r| r.id.as_str()).collect()
    }

    #[test]
    fn a_rule_no_source_owns_is_rejected() {
        let (fake, started, _) = shared(FakeSource::new("fake", &[]), false);
        let hub = Hub::new(vec![fake]);
        let states = hub.apply(vec![
            rule("r1", "other.x", json!({})),
            rule("r2", "plain", json!({})),
        ]);
        assert_eq!(
            states["r1"],
            RuleState::Rejected {
                reason: "no source for kind other.x".to_string()
            }
        );
        assert_eq!(
            states["r2"],
            RuleState::Rejected {
                reason: "no source for kind plain".to_string()
            }
        );
        assert!(started.started.lock().unwrap().is_empty());
    }

    #[test]
    fn a_rule_the_source_rejects_is_rejected_and_the_rest_start() {
        let (fake, started, _) = shared(FakeSource::new("fake", &[]), true);
        let hub = Hub::new(vec![fake]);
        let states = hub.apply(vec![
            rule("bad1", "fake.x", json!({"bad": 1})),
            rule("good", "fake.x", json!({})),
        ]);
        assert_eq!(
            states["bad1"],
            RuleState::Rejected {
                reason: "field bad: not allowed".to_string()
            }
        );
        assert_eq!(states["good"], RuleState::Active);
        let calls = started.started.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(ids(&calls[0]), vec!["good"]);
    }

    #[test]
    fn start_is_called_once_with_the_rules_sorted_by_id() {
        let (fake, started, _) = shared(FakeSource::new("fake", &[]), false);
        let hub = Hub::new(vec![fake]);
        let states = hub.apply(vec![
            rule("r2", "fake.x", json!({})),
            rule("r1", "fake.y", json!({"a": 1})),
        ]);
        assert_eq!(states.len(), 2);
        assert_eq!(states["r1"], RuleState::Active);
        assert_eq!(states["r2"], RuleState::Active);
        let calls = started.started.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(ids(&calls[0]), vec!["r1", "r2"]);
        assert_eq!(calls[0][0].when["a"], json!(1));
    }

    #[test]
    fn each_source_gets_only_its_own_rules() {
        let (a, a_started, _) = shared(FakeSource::new("alpha", &[]), false);
        let (b, b_started, _) = shared(FakeSource::new("beta", &[]), false);
        let hub = Hub::new(vec![a, b]);
        hub.apply(vec![
            rule("r1", "alpha.x", json!({})),
            rule("r2", "beta.x", json!({})),
            rule("r3", "alpha.y", json!({})),
        ]);
        assert_eq!(ids(&a_started.started.lock().unwrap()[0]), vec!["r1", "r3"]);
        assert_eq!(ids(&b_started.started.lock().unwrap()[0]), vec!["r2"]);
    }

    #[test]
    fn an_unavailable_source_is_not_started() {
        let mut unsupported = FakeSource::new("alpha", &[]);
        unsupported.availability = Availability::Unsupported {
            reason: "nope".to_string(),
        };
        let mut needs = FakeSource::new("beta", &[]);
        needs.availability = Availability::NeedsPermission {
            permission: Permission::Accessibility,
            how: "Open System Settings".to_string(),
        };
        let (a, a_started, _) = shared(unsupported, false);
        let (b, b_started, _) = shared(needs, false);
        let hub = Hub::new(vec![a, b]);
        let states = hub.apply(vec![
            rule("r1", "alpha.x", json!({})),
            rule("r2", "beta.x", json!({})),
        ]);
        assert_eq!(
            states["r1"],
            RuleState::SourceFailed {
                reason: "nope".to_string()
            }
        );
        assert_eq!(
            states["r2"],
            RuleState::SourceFailed {
                reason: "needs permission Open System Settings".to_string()
            }
        );
        assert!(a_started.started.lock().unwrap().is_empty());
        assert!(b_started.started.lock().unwrap().is_empty());
    }
}
