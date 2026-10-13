---
id: T12
epic: 011-trigger-github
title: Add the text and JSON helpers for the diff
status: done
depends_on: [T11]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-github/src/logic/text.rs
  - src-tauri/crates/trigger-github/src/logic/json.rs
  - src-tauri/crates/trigger-github/src/logic.rs
---

## Goal

The crate has two small pure helper files used by every later card: `logic/text.rs` (percent encoding for request paths, cutting text to N characters, the number at the end of a URL, login comparison, epoch seconds to an ISO 8601 string) and `logic/json.rs` (read GitHub lists and fields by JSON pointer, empty when missing or `null`, plus `BodyError`).

## Read first

- `plans/011-trigger-github/research/R01-endpoints.md` the item shapes under "Evidence" (why fields are read by pointer: items carry many more fields, and some are `null`)
- `plans/011-trigger-github/epic.md` Decisions 9 and 13 (seen keys, the 200 character cut)
- `src-tauri/crates/trigger-github/src/logic.rs` the file that gets two new `pub mod` lines

## Steps

1. Create `src-tauri/crates/trigger-github/src/logic/text.rs` with exactly this.

`iso` turns whole seconds since the epoch into the form GitHub uses (`2026-10-12T20:24:14Z`), so two of them compare correctly as plain text. `encode` leaves only letters, digits and `-._~` alone. `cut` counts characters, not bytes.

```rust
//! Small text helpers for request paths and payloads. Pure.

/// Percent-encodes everything except letters, digits and `-._~`, so a branch
/// like `feature/x` or a time like `12:30` is safe inside a query string.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The first `max` characters (not bytes), so a cut never splits a letter.
pub fn cut(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// The number at the end of a URL path, such as 13923 for
/// `https://api.github.com/repos/cli/cli/issues/13923`.
pub fn last_number(url: &str) -> Option<u64> {
    url.split(['?', '#'])
        .next()?
        .rsplit('/')
        .next()?
        .parse()
        .ok()
}

/// Logins are not case sensitive on GitHub.
pub fn same_login(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Seconds since the Unix epoch as `2026-10-12T20:24:14Z`, the form GitHub
/// uses, so two of them compare correctly as plain text.
pub fn iso(epoch_s: u64) -> String {
    let days = (epoch_s / 86_400) as i64;
    let rest = epoch_s % 86_400;
    // Civil date from a day count (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_keeps_safe_characters_and_escapes_the_rest() {
        assert_eq!(encode("trunk"), "trunk");
        assert_eq!(encode("feature/x y"), "feature%2Fx%20y");
        assert_eq!(encode("dependabot[bot]"), "dependabot%5Bbot%5D");
        assert_eq!(encode("2026-10-12T20:24:14Z"), "2026-10-12T20%3A24%3A14Z");
        assert_eq!(encode("é"), "%C3%A9");
    }

    #[test]
    fn cut_counts_characters() {
        assert_eq!(cut("hello", 10), "hello");
        assert_eq!(cut("hello", 3), "hel");
        assert_eq!(cut("ééé", 2), "éé");
        assert_eq!(cut("", 5), "");
    }

    #[test]
    fn last_number_reads_the_end_of_a_url() {
        let issue = "https://api.github.com/repos/cli/cli/issues/13923";
        assert_eq!(last_number(issue), Some(13923));
        assert_eq!(last_number("https://x/pulls/14568?a=1"), Some(14568));
        assert_eq!(last_number("https://x/pulls/abc"), None);
        assert_eq!(last_number(""), None);
    }

    #[test]
    fn logins_ignore_case() {
        assert!(same_login("LiamDotPro", "liamdotpro"));
        assert!(!same_login("a", "b"));
    }

    #[test]
    fn iso_matches_known_times() {
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso(1_760_300_654), "2025-10-12T20:24:14Z");
        assert_eq!(iso(4_102_444_799), "2099-12-31T23:59:59Z");
    }

    #[test]
    fn iso_times_compare_as_text() {
        assert!(iso(1_000_000_000) < iso(1_000_000_001));
        assert!(iso(999_999_999) < iso(1_000_000_000));
    }
}
```

2. Create `src-tauri/crates/trigger-github/src/logic/json.rs` with exactly this.

Items are kept as `serde_json::Value` instead of one struct per endpoint, because real items carry dozens of fields we do not use and some used fields are `null` (a release without a name, a run without a branch). A reader gives an empty value, never an error, for something missing. `id_key` is the item `id` as text for a seen set, `None` when the id is missing or 0. `BodyError` carries a short message and never the body.

```rust
//! Reading GitHub's JSON without a struct per endpoint. Real list items carry
//! dozens of fields we never use, and some of the ones we do use are `null`
//! (a release without a name, a run without a branch), so every reader here
//! gives an empty value for a missing or `null` field instead of failing.

use std::fmt;

pub use serde_json::Value;

/// The body was not the JSON we expected. Carries a short message, never the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyError(pub String);

impl fmt::Display for BodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BodyError {}

/// The items of a list response. With `wrapper` the list is inside an object
/// under that key (`workflow_runs` for `actions/runs`), otherwise the body is
/// the array itself.
pub fn list(body: &str, wrapper: Option<&str>) -> Result<Vec<Value>, BodyError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|e| BodyError(format!("the answer was not JSON: {e}")))?;
    let array = match wrapper {
        Some(key) => value.get(key),
        None => Some(&value),
    };
    match array.and_then(Value::as_array) {
        Some(items) => Ok(items.clone()),
        None => Err(BodyError("the answer was not a list".into())),
    }
}

/// A string at a JSON pointer such as `/user/login`. Empty when missing or not text.
pub fn text<'a>(v: &'a Value, pointer: &str) -> &'a str {
    v.pointer(pointer).and_then(Value::as_str).unwrap_or("")
}

/// A number at a JSON pointer. 0 when missing or not a whole number.
pub fn num(v: &Value, pointer: &str) -> u64 {
    v.pointer(pointer).and_then(Value::as_u64).unwrap_or(0)
}

/// A boolean at a JSON pointer. False when missing or not a boolean.
pub fn flag(v: &Value, pointer: &str) -> bool {
    v.pointer(pointer).and_then(Value::as_bool).unwrap_or(false)
}

/// The item's `id` as text, for a seen set. `None` when it has no usable id.
pub fn id_key(v: &Value) -> Option<String> {
    match num(v, "/id") {
        0 => None,
        id => Some(id.to_string()),
    }
}

/// True when the pointer finds something that is not `null`.
pub fn has(v: &Value, pointer: &str) -> bool {
    v.pointer(pointer).is_some_and(|x| !x.is_null())
}

/// The `login` of each object in an array at the pointer, such as `/assignees`.
pub fn logins<'a>(v: &'a Value, pointer: &str) -> Vec<&'a str> {
    v.pointer(pointer)
        .and_then(Value::as_array)
        .map(|list| list.iter().map(|u| text(u, "/login")).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn list_reads_an_array_or_a_wrapped_array() {
        assert_eq!(list("[{\"id\":1},{\"id\":2}]", None).unwrap().len(), 2);
        let wrapped = "{\"total_count\":1,\"workflow_runs\":[{\"id\":1}]}";
        assert_eq!(list(wrapped, Some("workflow_runs")).unwrap().len(), 1);
        assert_eq!(list("[]", None).unwrap().len(), 0);
    }

    #[test]
    fn list_refuses_anything_else_without_echoing_it() {
        for (body, wrapper) in [
            ("not json", None),
            ("{\"message\":\"Not Found\"}", None),
            ("{\"a\":1}", Some("workflow_runs")),
            ("", None),
        ] {
            let err = list(body, wrapper).unwrap_err();
            assert!(!err.to_string().contains("Not Found"), "{err}");
        }
    }

    #[test]
    fn readers_give_empty_values_for_missing_and_null() {
        let v = json!({
            "id": 7, "name": null, "draft": true,
            "user": { "login": "octocat" },
            "assignees": [{ "login": "a" }, { "login": "b" }]
        });
        assert_eq!(num(&v, "/id"), 7);
        assert_eq!(num(&v, "/missing"), 0);
        assert_eq!(text(&v, "/user/login"), "octocat");
        assert_eq!(text(&v, "/name"), "");
        assert_eq!(text(&v, "/nope/deeper"), "");
        assert!(flag(&v, "/draft"));
        assert!(!flag(&v, "/other"));
        assert!(has(&v, "/user"));
        assert!(!has(&v, "/name"));
        assert!(!has(&v, "/missing"));
        assert_eq!(logins(&v, "/assignees"), ["a", "b"]);
        assert_eq!(id_key(&v), Some("7".to_string()));
        assert_eq!(id_key(&json!({ "id": null })), None);
        assert_eq!(id_key(&json!({})), None);
        assert!(logins(&v, "/user").is_empty());
    }
}
```

3. Edit `logic.rs` so it declares the two modules. Replace the line `pub mod response;` with the three lines below. Use exactly this edit in `src-tauri/crates/trigger-github/src/logic.rs`.

Find this text, which appears once:

```rust
pub mod response;
```

Replace it with:

```rust
pub mod json;
pub mod response;
pub mod text;
```

4. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib` passes with 45 tests (9 new in this card)
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (the `source`, `readme` and `live` tests included)
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (warnings that `lib.rs` and `os.rs` still have template code are expected until slice C)
- [ ] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- Adding `chrono`, `time`, `url` or any other dependency. `Cargo.toml` is not touched.
- Struct types for the GitHub items. Everything reads `Value` by pointer.
- Removing `#![allow(dead_code)]` from `logic.rs`.

## If stuck

- If a test fails, compare the file with step 1 character by character. Do not change a test. In particular a date in `iso_matches_known_times` is right as written.
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (45 lib tests, 9 new in text.rs and json.rs; full crate tests, clippy -D warnings, fmt check, check-crates exit 0; scope lists only logic.rs, logic/text.rs and logic/json.rs)
- 2026-10-12 checker: pass
