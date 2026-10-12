---
id: T02
epic: 001-trigger-core
title: Add Platforms and the ms_since_epoch helper
status: todo
depends_on: [T01]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-core/src/lib.rs
---

## Goal

`trigger_core::Platforms` (a bit set that serialises as `["macos","windows","linux"]`) and `trigger_core::ms_since_epoch` (a serde helper for `SystemTime`) exist, with unit tests.

## Read first

- `src-tauri/crates/trigger-core/src/lib.rs` the file you append to (created by T01)
- `plans/001-trigger-core/research/R06-contract-compiles.md` the code below was compiled there

## Steps

1. Append this to `src-tauri/crates/trigger-core/src/lib.rs`, after the `//!` doc comment, leaving one blank line between:

```rust
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
```

2. Append this test module at the very end of the file:

```rust
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
        let after = Stamp { at: UNIX_EPOCH + Duration::from_millis(1_760_000_000_123) };
        let json = serde_json::to_string(&after).unwrap();
        assert_eq!(json, r#"{"at":1760000000123}"#);
        assert_eq!(serde_json::from_str::<Stamp>(&json).unwrap(), after);

        let before = Stamp { at: UNIX_EPOCH - Duration::from_millis(1_500) };
        let json = serde_json::to_string(&before).unwrap();
        assert_eq!(json, r#"{"at":-1500}"#);
        assert_eq!(serde_json::from_str::<Stamp>(&json).unwrap(), before);
    }
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-core` to format, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-core` passes with 4 tests in `platforms_tests`
- [ ] `cd src-tauri && cargo clippy -p trigger-core --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-core --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Any other type from the Shape (`Permission`, `KindInfo`, and so on). T03 adds them.
- Adding `Deserialize` to `Platforms`.
- Changing the code above, even if it looks improvable. It was compiled and checked in R06.

## If stuck

- If a test fails because the JSON differs, do not change the expected strings. Compare your code to step 1 character by character.
- If clippy complains about something in the code from step 1, set status blocked and quote the lint in the Log.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
