//! The contract between trigger sources and whatever runs them. A source
//! depends on this crate only. See docs/process-triggers.md Part 2.

pub mod testkit;
#[cfg(test)]
mod tests;

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

/// Renders a kinds table for a README. Checked by each crate's tests/readme.rs.
pub fn kinds_markdown(kinds: &[KindInfo]) -> String {
    let mut out = String::from("| Kind | Title | Platforms | Permission | When | Payload |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for k in kinds {
        let permission = k.permission.map_or("none".to_string(), permission_label);
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            k.id,
            k.title,
            platforms_label(k.platforms),
            permission,
            fields_label(k.when, true),
            fields_label(k.payload, false),
        ));
    }
    out
}

fn platforms_label(p: Platforms) -> String {
    let all = [
        (Platforms::MACOS, "macOS"),
        (Platforms::WINDOWS, "Windows"),
        (Platforms::LINUX, "Linux"),
    ];
    let names: Vec<&str> = all
        .iter()
        .filter(|(f, _)| p.has(*f))
        .map(|(_, n)| *n)
        .collect();
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

fn permission_label(p: Permission) -> String {
    match p {
        Permission::Accessibility => "accessibility".to_string(),
        Permission::ScreenRecording => "screen recording".to_string(),
        Permission::Location => "location".to_string(),
        Permission::Calendar => "calendar".to_string(),
        Permission::Reminders => "reminders".to_string(),
        Permission::Folder(name) => format!("folder {name}"),
        Permission::Microphone => "microphone".to_string(),
        Permission::Camera => "camera".to_string(),
        Permission::Notifications => "notifications".to_string(),
    }
}

fn fields_label(fields: &[FieldInfo], mark_required: bool) -> String {
    if fields.is_empty() {
        return "none".to_string();
    }
    let names: Vec<String> = fields
        .iter()
        .map(|f| {
            if mark_required && f.required {
                format!("{}*", f.name)
            } else {
                f.name.to_string()
            }
        })
        .collect();
    names.join(", ")
}
