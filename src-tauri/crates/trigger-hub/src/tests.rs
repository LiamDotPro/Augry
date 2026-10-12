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
