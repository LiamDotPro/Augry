---
id: T13
epic: 011-trigger-github
title: Build the payload of each kind
status: todo
depends_on: [T12]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-github/src/logic/payload.rs
  - src-tauri/crates/trigger-github/src/logic.rs
---

## Goal

The crate has `logic/payload.rs`: one function per payload shape (`issue`, `review`, `ci`, `comment`, `merged`, `release`, `dependency`) that turns one GitHub list item into the flat `BTreeMap<String, Value>` the kind documents. A test checks that the keys of every payload are exactly the payload fields `KINDS` documents in `lib.rs`.

## Read first

- `plans/011-trigger-github/epic.md` Decision 13 (the payload is R01's table, a comment body is cut to 200 characters)
- `plans/011-trigger-github/research/R01-endpoints.md` the table under "Implications for the cards", Payload fields column
- `src-tauri/crates/trigger-github/src/lib.rs` the `PAYLOAD_*` constants (the field names to match)
- `src-tauri/crates/trigger-github/src/logic/json.rs` the readers (created by T12)

## Steps

1. Create `src-tauri/crates/trigger-github/src/logic/payload.rs` with exactly this.

Payload names are flat (`author` is `user.login`, `base` is `base.ref`, `branch` is `head_branch`, `sha` is `head_sha`, `tag` is `tag_name`, `url` is `html_url`). Every value is text or a whole number, which is what the README says. `prerelease` is the text `true` or `false`, and `assignees` is the logins joined with a comma. A release without a name is shown by its tag. A comment body is cut to `BODY_CHARS` (200) characters.

```rust
//! The payload of each kind, built from one item of a GitHub list. Pure.
//!
//! The field names are the ones `KINDS` in `lib.rs` documents; the test at the
//! bottom keeps the two in step. Every value is text or a whole number.

use std::collections::BTreeMap;

use super::json::{logins, num, text, Value};
use super::text::cut;

pub type Payload = BTreeMap<String, Value>;

/// A comment body is cut to this many characters.
pub const BODY_CHARS: usize = 200;

fn build(fields: Vec<(&str, Value)>) -> Payload {
    fields
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect()
}

fn s(v: &Value, pointer: &str) -> Value {
    Value::from(text(v, pointer))
}

fn n(v: &Value, pointer: &str) -> Value {
    Value::from(num(v, pointer))
}

/// `github.issue-assigned`, from an item of `issues`.
pub fn issue(v: &Value) -> Payload {
    build(vec![
        ("number", n(v, "/number")),
        ("title", s(v, "/title")),
        ("url", s(v, "/html_url")),
        ("author", s(v, "/user/login")),
        ("assignees", Value::from(logins(v, "/assignees").join(","))),
    ])
}

/// `github.review-requested`, from an item of `pulls`.
pub fn review(v: &Value) -> Payload {
    build(vec![
        ("number", n(v, "/number")),
        ("title", s(v, "/title")),
        ("url", s(v, "/html_url")),
        ("author", s(v, "/user/login")),
        ("head", s(v, "/head/ref")),
        ("base", s(v, "/base/ref")),
    ])
}

/// `github.ci-failed` and `github.ci-passed`, from an item of `actions/runs`.
pub fn ci(v: &Value) -> Payload {
    build(vec![
        ("id", n(v, "/id")),
        ("name", s(v, "/name")),
        ("branch", s(v, "/head_branch")),
        ("sha", s(v, "/head_sha")),
        ("conclusion", s(v, "/conclusion")),
        ("url", s(v, "/html_url")),
        ("event", s(v, "/event")),
        ("actor", s(v, "/actor/login")),
        ("updated_at", s(v, "/updated_at")),
    ])
}

/// `github.pr-commented`, from an item of `issues/comments` or `pulls/comments`.
pub fn comment(v: &Value) -> Payload {
    build(vec![
        ("id", n(v, "/id")),
        ("url", s(v, "/html_url")),
        ("author", s(v, "/user/login")),
        ("body", Value::from(cut(text(v, "/body"), BODY_CHARS))),
        ("created_at", s(v, "/created_at")),
    ])
}

/// `github.pr-merged`, from an item of `pulls`.
pub fn merged(v: &Value) -> Payload {
    build(vec![
        ("number", n(v, "/number")),
        ("title", s(v, "/title")),
        ("url", s(v, "/html_url")),
        ("author", s(v, "/user/login")),
        ("merged_at", s(v, "/merged_at")),
        ("base", s(v, "/base/ref")),
    ])
}

/// `github.release-published`, from an item of `releases`. A release without a
/// name is shown by its tag.
pub fn release(v: &Value) -> Payload {
    let name = match text(v, "/name") {
        "" => text(v, "/tag_name"),
        name => name,
    };
    let prerelease = if super::json::flag(v, "/prerelease") {
        "true"
    } else {
        "false"
    };
    build(vec![
        ("tag", s(v, "/tag_name")),
        ("name", Value::from(name)),
        ("prerelease", Value::from(prerelease)),
        ("url", s(v, "/html_url")),
        ("published_at", s(v, "/published_at")),
        ("author", s(v, "/author/login")),
    ])
}

/// `github.dependency-pr-opened`, from an item of `pulls`.
pub fn dependency(v: &Value) -> Payload {
    build(vec![
        ("number", n(v, "/number")),
        ("title", s(v, "/title")),
        ("url", s(v, "/html_url")),
        ("author", s(v, "/user/login")),
        ("base", s(v, "/base/ref")),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pull() -> Value {
        json!({
            "id": 4830652819u64, "number": 14648,
            "title": "Fix the unclean background colors of gh-search-code",
            "merged_at": "2026-10-12T16:14:17Z",
            "html_url": "https://github.com/cli/cli/pull/14648",
            "user": { "login": "A4-Tacks", "type": "User" },
            "head": { "ref": "end-search-bg" }, "base": { "ref": "trunk" },
            "extra": { "ignored": true }
        })
    }

    fn keys(p: &Payload) -> Vec<&str> {
        p.keys().map(String::as_str).collect()
    }

    #[test]
    fn every_payload_has_exactly_the_documented_fields() {
        let run = json!({ "id": 1, "name": "CI", "head_branch": "trunk" });
        let release_item = json!({ "tag_name": "v1" });
        let cases: Vec<(&str, Payload)> = vec![
            ("github.issue-assigned", issue(&pull())),
            ("github.review-requested", review(&pull())),
            ("github.ci-failed", ci(&run)),
            ("github.ci-passed", ci(&run)),
            ("github.pr-commented", comment(&pull())),
            ("github.pr-merged", merged(&pull())),
            ("github.release-published", release(&release_item)),
            ("github.dependency-pr-opened", dependency(&pull())),
        ];
        for (kind, payload) in cases {
            let info = crate::KINDS.iter().find(|k| k.id == kind).unwrap();
            let mut documented: Vec<&str> = info.payload.iter().map(|f| f.name).collect();
            documented.sort_unstable();
            assert_eq!(keys(&payload), documented, "{kind}");
        }
    }

    #[test]
    fn a_merged_pull_request_payload() {
        let p = merged(&pull());
        assert_eq!(p["number"], json!(14648));
        assert_eq!(p["author"], json!("A4-Tacks"));
        assert_eq!(p["merged_at"], json!("2026-10-12T16:14:17Z"));
        assert_eq!(p["base"], json!("trunk"));
        assert_eq!(p["url"], json!("https://github.com/cli/cli/pull/14648"));
    }

    #[test]
    fn issue_assignees_are_comma_separated() {
        let v = json!({
            "number": 3, "title": "t", "html_url": "u", "user": { "login": "o" },
            "assignees": [{ "login": "babakks" }, { "login": "Copilot" }]
        });
        assert_eq!(issue(&v)["assignees"], json!("babakks,Copilot"));
        assert_eq!(issue(&json!({}))["assignees"], json!(""));
    }

    #[test]
    fn a_comment_body_is_cut_to_200_characters() {
        let long = "é".repeat(250);
        let p = comment(&json!({ "id": 9, "body": long }));
        assert_eq!(text_of(&p, "body").chars().count(), 200);
        let short = comment(&json!({ "id": 9, "body": "thanks" }));
        assert_eq!(short["body"], json!("thanks"));
        let none = comment(&json!({ "id": 9, "body": null }));
        assert_eq!(none["body"], json!(""));
    }

    fn text_of<'a>(p: &'a Payload, key: &str) -> &'a str {
        p[key].as_str().unwrap()
    }

    #[test]
    fn ci_payload_uses_flat_names() {
        let run = json!({
            "id": 38303985555u64, "name": "Triage", "head_branch": "trunk",
            "head_sha": "54a671d", "conclusion": "failure", "event": "push",
            "html_url": "https://github.com/cli/cli/actions/runs/1",
            "actor": { "login": "BagToad" }, "updated_at": "2026-10-12T20:24:27Z"
        });
        let p = ci(&run);
        assert_eq!(p["branch"], json!("trunk"));
        assert_eq!(p["sha"], json!("54a671d"));
        assert_eq!(p["actor"], json!("BagToad"));
        assert_eq!(p["id"], json!(38303985555u64));
    }

    #[test]
    fn a_release_falls_back_to_its_tag_and_prints_prerelease_as_text() {
        let v = json!({ "tag_name": "v2.102.0", "name": null, "prerelease": true,
            "author": { "login": "github-actions[bot]" } });
        let p = release(&v);
        assert_eq!(p["name"], json!("v2.102.0"));
        assert_eq!(p["prerelease"], json!("true"));
        assert_eq!(p["author"], json!("github-actions[bot]"));
        let named = json!({ "tag_name": "v1", "name": "One" });
        assert_eq!(release(&named)["name"], json!("One"));
        assert_eq!(release(&named)["prerelease"], json!("false"));
    }

    #[test]
    fn missing_fields_become_empty_not_a_panic() {
        let p = review(&json!({}));
        assert_eq!(p["number"], json!(0));
        assert_eq!(p["title"], json!(""));
        assert_eq!(p["head"], json!(""));
    }
}
```

2. Edit `logic.rs` so it declares the module. Replace the line `pub mod json;` with the two lines below. Use exactly this edit in `src-tauri/crates/trigger-github/src/logic.rs`.

Find this text, which appears once:

```rust
pub mod json;
```

Replace it with:

```rust
pub mod json;
pub mod payload;
```

3. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-github --lib` passes with 52 tests (7 new in this card)
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (the `source`, `readme` and `live` tests included)
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (warnings that `lib.rs` and `os.rs` still have template code are expected until slice C)
- [ ] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- Changing a payload field name or `PAYLOAD_*` in `lib.rs`. If the test `every_payload_has_exactly_the_documented_fields` fails, the card is wrong, not `lib.rs`: set status blocked.
- Deciding which items fire. That is `diff.rs` and the group files.
- Removing `#![allow(dead_code)]` from `logic.rs`.

## If stuck

- If a test fails, compare the file with step 1 character by character. Do not change a test.
- If `every_payload_has_exactly_the_documented_fields` fails and the file matches the card, `KINDS` in `lib.rs` is not what T02 wrote. Set status blocked and say which kind differs.
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
