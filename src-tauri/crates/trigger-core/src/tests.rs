//! Unit tests for the contract. Kept out of lib.rs so that file stays small.

mod platforms_tests {
    use crate::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Stamp {
        #[serde(with = "crate::ms_since_epoch")]
        at: SystemTime,
    }

    #[test]
    fn or_and_has() {
        let p = Platforms::MACOS.or(Platforms::LINUX);
        assert!(p.has(Platforms::MACOS));
        assert!(p.has(Platforms::LINUX));
        assert!(!p.has(Platforms::WINDOWS));
        assert!(Platforms::ALL.has(p));
    }

    #[test]
    fn here_is_one_platform_in_all() {
        assert!(Platforms::ALL.has(Platforms::here()));
    }

    #[test]
    fn serialises_as_names() {
        assert_eq!(
            serde_json::to_value(Platforms::ALL).unwrap(),
            serde_json::json!(["macos", "windows", "linux"])
        );
        assert_eq!(
            serde_json::to_value(Platforms::MACOS.or(Platforms::LINUX)).unwrap(),
            serde_json::json!(["macos", "linux"])
        );
    }

    #[test]
    fn time_round_trips_as_milliseconds() {
        let after = Stamp {
            at: UNIX_EPOCH + Duration::from_millis(1_760_000_000_123),
        };
        let json = serde_json::to_string(&after).unwrap();
        assert_eq!(json, r#"{"at":1760000000123}"#);
        assert_eq!(serde_json::from_str::<Stamp>(&json).unwrap(), after);

        let before = Stamp {
            at: UNIX_EPOCH - Duration::from_millis(1_500),
        };
        let json = serde_json::to_string(&before).unwrap();
        assert_eq!(json, r#"{"at":-1500}"#);
        assert_eq!(serde_json::from_str::<Stamp>(&json).unwrap(), before);
    }
}

mod json_tests {
    use crate::*;
    use serde_json::json;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn permission_json() {
        assert_eq!(
            serde_json::to_value(Permission::Accessibility).unwrap(),
            json!("accessibility")
        );
        assert_eq!(
            serde_json::to_value(Permission::ScreenRecording).unwrap(),
            json!("screenRecording")
        );
        assert_eq!(
            serde_json::to_value(Permission::Folder("Downloads")).unwrap(),
            json!({"folder": "Downloads"})
        );
    }

    #[test]
    fn event_at_is_a_number_of_milliseconds() {
        let event = Event {
            rule: "r".into(),
            kind: "k".into(),
            at: UNIX_EPOCH + Duration::from_millis(1_760_000_000_123),
            payload: BTreeMap::new(),
        };
        let v = serde_json::to_value(&event).unwrap();
        assert!(v["at"].is_number());
        assert_eq!(
            v,
            json!({"rule": "r", "kind": "k", "at": 1_760_000_000_123i64, "payload": {}})
        );
    }

    #[test]
    fn kind_info_json() {
        let kind = KindInfo {
            id: "fs.x",
            title: "T",
            summary: "s",
            platforms: Platforms::MACOS.or(Platforms::LINUX),
            permission: Some(Permission::Folder("Downloads")),
            when: &[FieldInfo {
                name: "path",
                doc: "d",
                required: true,
            }],
            payload: &[],
        };
        assert_eq!(
            serde_json::to_value(kind).unwrap(),
            json!({
                "id": "fs.x", "title": "T", "summary": "s",
                "platforms": ["macos", "linux"],
                "permission": {"folder": "Downloads"},
                "when": [{"name": "path", "doc": "d", "required": true}],
                "payload": []
            })
        );
    }

    #[test]
    fn availability_json() {
        assert_eq!(
            serde_json::to_value(Availability::Available).unwrap(),
            json!({"state": "available"})
        );
        assert_eq!(
            serde_json::to_value(Availability::NeedsPermission {
                permission: Permission::Folder("Downloads"),
                how: "x".into()
            })
            .unwrap(),
            json!({"state": "needsPermission", "permission": {"folder": "Downloads"}, "how": "x"})
        );
    }

    #[test]
    fn rule_reads_from_json() {
        let rule: Rule =
            serde_json::from_value(json!({"id": "a", "kind": "fs.x", "when": {"path": "/tmp"}}))
                .unwrap();
        assert_eq!(rule.when["path"], json!("/tmp"));
    }
}

mod handle_tests {
    use crate::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn dropping_a_handle_runs_stop_once() {
        let count = Arc::new(AtomicU32::new(0));
        let c = count.clone();
        let handle = Handle::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(count.load(Ordering::SeqCst), 0);
        drop(handle);
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn error_messages() {
        assert_eq!(
            RuleError::UnknownKind("x.y".into()).to_string(),
            "unknown kind x.y"
        );
        assert_eq!(
            RuleError::Field {
                field: "path".into(),
                problem: "is required".into()
            }
            .to_string(),
            "field path: is required"
        );
        assert_eq!(SourceError::Os("boom".into()).to_string(), "boom");
        assert_eq!(SourceError::Unavailable("no".into()).to_string(), "no");
    }
}

mod markdown_tests {
    use crate::*;

    const WHEN: &[FieldInfo] = &[
        FieldInfo {
            name: "path",
            doc: "Folder to watch",
            required: true,
        },
        FieldInfo {
            name: "glob",
            doc: "Which files",
            required: false,
        },
    ];
    const PAYLOAD: &[FieldInfo] = &[
        FieldInfo {
            name: "path",
            doc: "Where",
            required: true,
        },
        FieldInfo {
            name: "name",
            doc: "File name",
            required: true,
        },
    ];
    const KINDS: &[KindInfo] = &[
        KindInfo {
            id: "fs.file-saved",
            title: "File saved",
            summary: "A file was saved",
            platforms: Platforms::ALL,
            permission: Some(Permission::Folder("Downloads")),
            when: WHEN,
            payload: PAYLOAD,
        },
        KindInfo {
            id: "x.thing",
            title: "Thing",
            summary: "s",
            platforms: Platforms::MACOS.or(Platforms::LINUX),
            permission: None,
            when: &[],
            payload: &[],
        },
    ];

    #[test]
    fn renders_the_exact_table() {
        let expected = "\
| Kind | Title | Platforms | Permission | When | Payload |
| --- | --- | --- | --- | --- | --- |
| fs.file-saved | File saved | macOS, Windows, Linux | folder Downloads | path*, glob | path, name |
| x.thing | Thing | macOS, Linux | none | none | none |
";
        assert_eq!(kinds_markdown(KINDS), expected);
    }

    #[test]
    fn no_kinds_gives_only_the_header() {
        assert_eq!(
            kinds_markdown(&[]),
            "| Kind | Title | Platforms | Permission | When | Payload |\n| --- | --- | --- | --- | --- | --- |\n"
        );
    }
}
