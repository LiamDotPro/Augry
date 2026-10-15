---
id: T18
epic: 003-trigger-git
title: Add the uncommitted-older-than transition logic
status: done
depends_on: [T17]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-git/src/logic/polled.rs
  - src-tauri/crates/trigger-git/src/logic/polled_tests.rs
---

## Goal

`UncommittedWatch` is the pure rule for `git.uncommitted-older-than`: it fires once when the oldest change has waited the limit and arms again when the tree is clean. `uncommitted_payload` builds the payload. Unit tests need no git.

## Read first

- `plans/003-trigger-git/epic.md` Decision 10 (`uncommitted-older-than`)
- `src-tauri/crates/trigger-git/src/logic/polled.rs` the file you append to
- `src-tauri/crates/trigger-git/src/logic/polled_tests.rs` the tests you append to (`listed` and `keys` are already there)
- `src-tauri/crates/trigger-git/src/lib.rs` the `PAYLOAD_UNCOMMITTED` list

## Steps

1. Append this to the end of `src-tauri/crates/trigger-git/src/logic/polled.rs` (it starts with a blank line).

```rust

/// `git.uncommitted-older-than`: fires once when the oldest modified tracked
/// file has waited `older_than_min` minutes, and arms again when the tree is
/// clean.
#[derive(Debug, Default)]
pub struct UncommittedWatch {
    fired: bool,
}

impl UncommittedWatch {
    pub fn new() -> UncommittedWatch {
        UncommittedWatch::default()
    }

    /// Feeds one poll. `age_secs` is what `Repo::oldest_change_secs` returned.
    /// Returns the whole minutes the oldest change has waited when this poll
    /// fires.
    ///
    /// - None means a clean tree (or a failed read): the watch arms again.
    /// - At or past the limit it fires once, then stays quiet until a clean
    ///   tree. Committing some of the files does not arm it again.
    /// - There is no baseline: work that is already old when the rule starts
    ///   fires on the first poll.
    pub fn observe(&mut self, age_secs: Option<u64>, older_than_min: u64) -> Option<u64> {
        let Some(age) = age_secs else {
            self.fired = false;
            return None;
        };
        if self.fired || age < older_than_min.saturating_mul(60) {
            return None;
        }
        self.fired = true;
        Some(age / 60)
    }
}

/// The payload of `git.uncommitted-older-than`: exactly the fields listed in
/// `KINDS`.
pub fn uncommitted_payload(repo: &str, branch: Option<&str>, minutes: u64) -> Payload {
    let mut out = Payload::new();
    out.insert("repo".into(), Value::from(repo));
    out.insert("branch".into(), Value::from(branch.unwrap_or_default()));
    out.insert("minutes".into(), Value::from(minutes));
    out
}
```

2. Append this to the end of `src-tauri/crates/trigger-git/src/logic/polled_tests.rs` (it starts with a blank line).

```rust

#[test]
fn uncommitted_waits_for_the_limit() {
    let mut watch = UncommittedWatch::new();
    assert_eq!(watch.observe(Some(59 * 60), 60), None);
    assert_eq!(watch.observe(Some(60 * 60 - 1), 60), None);
    assert_eq!(watch.observe(Some(60 * 60), 60), Some(60));
}

#[test]
fn uncommitted_reports_whole_minutes() {
    let mut watch = UncommittedWatch::new();
    assert_eq!(watch.observe(Some(95 * 60 + 59), 60), Some(95));
}

#[test]
fn uncommitted_fires_once_until_the_tree_is_clean() {
    let mut watch = UncommittedWatch::new();
    assert_eq!(watch.observe(Some(7200), 60), Some(120));
    assert_eq!(watch.observe(Some(7500), 60), None);
    // Part of the work is committed: the rest is younger, still no new event.
    assert_eq!(watch.observe(Some(300), 60), None);
    assert_eq!(watch.observe(Some(7800), 60), None);
    // Clean tree, then old work again.
    assert_eq!(watch.observe(None, 60), None);
    assert_eq!(watch.observe(Some(300), 60), None);
    assert_eq!(watch.observe(Some(3600), 60), Some(60));
}

#[test]
fn uncommitted_work_that_is_already_old_fires_on_the_first_poll() {
    let mut watch = UncommittedWatch::new();
    assert_eq!(watch.observe(Some(10_000), 60), Some(166));
}

#[test]
fn uncommitted_with_a_zero_limit_fires_on_any_change() {
    let mut watch = UncommittedWatch::new();
    assert_eq!(watch.observe(Some(0), 0), Some(0));
    // A huge limit cannot overflow.
    let mut big = UncommittedWatch::new();
    assert_eq!(big.observe(Some(u64::MAX - 1), u64::MAX), None);
}

#[test]
fn uncommitted_payload_has_exactly_the_listed_fields() {
    let payload = uncommitted_payload("/work/app", Some("feat/x"), 75);
    assert_eq!(keys(&payload), listed("git.uncommitted-older-than"));
    assert_eq!(payload["repo"], "/work/app");
    assert_eq!(payload["branch"], "feat/x");
    assert_eq!(payload["minutes"], 75);
    assert_eq!(uncommitted_payload("/r", None, 1)["branch"], "");
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-git`.

Note on `check-crates.mjs`: it prints warnings, not errors, about template text. Until slice C it may print `README.md still has template text`, `src/lib.rs still has template code`, `src/os.rs still has template code` and a length warning for `src/logic/tests.rs`. Warnings are fine; only errors fail the check. Do not try to remove them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes: 88 tests in the library, 1 in `readme`, `live` shows 1 ignored
- [ ] `cd src-tauri && cargo test -p trigger-git polled_tests` runs 12 tests and they pass
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- Reading file times (that is `GitRepo::oldest_change_secs`, T16).
- Re-arming when only part of the work is committed. Decision 10 says it re-arms when the tree is clean, and the tests pin that.
- The interval timer and the thread (slice C).
- Changing `BehindWatch` or anything above the code you append.

## If stuck

- If the file does not end with the `behind_payload` function, T17 was built differently from its card. Set status blocked.
- If `uncommitted_with_a_zero_limit_fires_on_any_change` fails, quote the output and set status blocked. Do not change the test.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
