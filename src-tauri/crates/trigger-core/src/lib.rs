//! The contract between trigger sources and whatever runs them. A source
//! depends on this crate only. See docs/process-triggers.md Part 2.

use std::collections::BTreeMap;
use std::sync::mpsc::Sender;
use std::time::SystemTime;

pub use serde_json::Value;

/// Bit set of the platforms a kind works on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Platforms(u8);

impl Platforms {
    pub const MACOS: Platforms = Platforms(1);
    pub const WINDOWS: Platforms = Platforms(2);
    pub const LINUX: Platforms = Platforms(4);
    pub const ALL: Platforms = Platforms(7);

    pub const fn or(self, other: Platforms) -> Platforms {
        Platforms(self.0 | other.0)
    }

    pub const fn has(self, other: Platforms) -> bool {
        self.0 & other.0 == other.0
    }

    /// The platform this binary runs on.
    pub fn here() -> Platforms {
        if cfg!(target_os = "macos") {
            Platforms::MACOS
        } else if cfg!(target_os = "windows") {
            Platforms::WINDOWS
        } else {
            Platforms::LINUX
        }
    }
}

// Serialises as the names that are set, in this order: macos, windows, linux.
impl serde::Serialize for Platforms {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let all = [
            (Platforms::MACOS, "macos"),
            (Platforms::WINDOWS, "windows"),
            (Platforms::LINUX, "linux"),
        ];
        let names: Vec<&str> = all
            .iter()
            .filter(|(p, _)| self.has(*p))
            .map(|(_, n)| *n)
            .collect();
        let mut seq = s.serialize_seq(Some(names.len()))?;
        for n in names {
            seq.serialize_element(n)?;
        }
        seq.end()
    }
}

/// serde helper: SystemTime as whole milliseconds since the Unix epoch.
pub mod ms_since_epoch {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    pub fn serialize<S: serde::Serializer>(t: &SystemTime, s: S) -> Result<S::Ok, S::Error> {
        let ms = match t.duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_millis() as i64,
            Err(e) => -(e.duration().as_millis() as i64),
        };
        s.serialize_i64(ms)
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<SystemTime, D::Error> {
        let ms = <i64 as serde::Deserialize>::deserialize(d)?;
        Ok(if ms >= 0 {
            UNIX_EPOCH + Duration::from_millis(ms as u64)
        } else {
            UNIX_EPOCH - Duration::from_millis(ms.unsigned_abs())
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    Accessibility,
    ScreenRecording,
    Location,
    Calendar,
    Reminders,
    Folder(&'static str),
    Microphone,
    Camera,
    Notifications,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldInfo {
    pub name: &'static str,
    pub doc: &'static str,
    pub required: bool,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub platforms: Platforms,
    pub permission: Option<Permission>,
    pub when: &'static [FieldInfo],
    pub payload: &'static [FieldInfo],
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Availability {
    Available,
    Unsupported { reason: String },
    NeedsPermission { permission: Permission, how: String },
}

pub type RuleId = String;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rule {
    pub id: RuleId,
    pub kind: String,
    pub when: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub rule: RuleId,
    pub kind: String,
    #[serde(with = "ms_since_epoch")]
    pub at: SystemTime,
    pub payload: BTreeMap<String, Value>,
}

pub type Sink = Sender<Event>;

#[derive(Debug, thiserror::Error)]
pub enum RuleError {
    #[error("unknown kind {0}")]
    UnknownKind(String),
    #[error("field {field}: {problem}")]
    Field { field: String, problem: String },
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Os(String),
}

/// Dropping it stops the watcher. Stop must not block for more than a second.
pub struct Handle(Option<Box<dyn FnOnce() + Send>>);

impl Handle {
    pub fn new(stop: impl FnOnce() + Send + 'static) -> Handle {
        Handle(Some(Box::new(stop)))
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if let Some(stop) = self.0.take() {
            stop()
        }
    }
}

pub trait Source: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn kinds(&self) -> &'static [KindInfo];
    fn availability(&self) -> Availability;
    fn check(&self, rule: &Rule) -> Result<(), RuleError>;
    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError>;
}

#[cfg(test)]
mod platforms_tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Stamp {
        #[serde(with = "super::ms_since_epoch")]
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

#[cfg(test)]
mod json_tests {
    use super::*;
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

#[cfg(test)]
mod handle_tests {
    use super::*;
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
