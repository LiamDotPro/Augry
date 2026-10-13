---
id: T05
epic: 003-trigger-git
title: Build the event payload for each Change
status: done
depends_on: [T04]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/logic.rs
  - src-tauri/crates/trigger-git/src/logic/tests.rs
---

## Goal

`Change::payload(repo)` returns exactly the payload fields that `KINDS` lists for the change's kind, and a unit test proves the two stay in step.

## Read first

- `plans/003-trigger-git/epic.md` Decision 5 and the line below Shape that lists the payload per kind
- `src-tauri/crates/trigger-git/src/lib.rs:60-110` the payload field lists the test compares against
- `src-tauri/crates/trigger-git/src/logic.rs` the file you add to

## Steps

1. In `src-tauri/crates/trigger-git/src/logic.rs`, insert this block directly above the two lines `#[cfg(test)]` and `mod tests;` at the end of the file (leave one blank line between). Every payload has the same keys every time: an absent value is the empty string, so `${trigger.branch}` never meets a missing key. `files` is a number, everything else is text. For a switch, `branch` is the new branch; for a detach, `branch` is the branch left.

```rust
/// Absent text becomes an empty string, so every payload field is always
/// there and `${trigger.branch}` never has to cope with a missing key.
fn text(value: &Option<String>) -> Value {
    Value::from(value.clone().unwrap_or_default())
}

impl Change {
    /// The event payload: the fields listed for this kind in `KINDS`, and no
    /// others. `repo` is the rule's repository path.
    pub fn payload(&self, repo: &str) -> Payload {
        let mut out = Payload::new();
        let mut put = |key: &str, value: Value| {
            out.insert(key.to_string(), value);
        };
        put("repo", Value::from(repo));
        match self {
            Change::RebaseStopped { branch, commit } => {
                put("branch", text(branch));
                put("commit", text(commit));
            }
            Change::MergeConflict { branch, files } => {
                put("branch", text(branch));
                put("files", Value::from(*files));
            }
            Change::BranchSwitched { from, to, commit } => {
                put("branch", Value::from(to.as_str()));
                put("from", Value::from(from.as_str()));
                put("to", Value::from(to.as_str()));
                put("commit", text(commit));
            }
            Change::DetachedHead { from, commit } => {
                put("branch", Value::from(from.as_str()));
                put("commit", text(commit));
            }
            Change::Pulled {
                branch,
                commit,
                message,
            }
            | Change::Commit {
                branch,
                commit,
                message,
            } => {
                put("branch", text(branch));
                put("commit", text(commit));
                put("message", text(message));
            }
            Change::TagCreated { branch, tag } => {
                put("branch", text(branch));
                put("tag", Value::from(tag.as_str()));
            }
            Change::StashCreated { branch } => {
                put("branch", text(branch));
            }
        }
        out
    }
}
```

2. In `src-tauri/crates/trigger-git/src/logic/tests.rs`, append this to the end of the file, after a blank line.

```rust
fn keys(payload: &Payload) -> Vec<&str> {
    payload.keys().map(String::as_str).collect()
}

fn names(fields: &[trigger_core::FieldInfo]) -> Vec<&str> {
    let mut names: Vec<&str> = fields.iter().map(|f| f.name).collect();
    names.sort_unstable();
    names
}

#[test]
fn payloads_carry_exactly_the_fields_listed_in_kinds() {
    let changes = [
        Change::RebaseStopped {
            branch: Some("main".into()),
            commit: Some("abc1234".into()),
        },
        Change::MergeConflict {
            branch: Some("main".into()),
            files: 2,
        },
        Change::BranchSwitched {
            from: "a".into(),
            to: "b".into(),
            commit: Some("abc1234".into()),
        },
        Change::DetachedHead {
            from: "main".into(),
            commit: Some("abc1234".into()),
        },
        Change::Pulled {
            branch: Some("main".into()),
            commit: Some("abc1234".into()),
            message: Some("Merge x".into()),
        },
        Change::Commit {
            branch: Some("main".into()),
            commit: Some("abc1234".into()),
            message: Some("Fix y".into()),
        },
        Change::TagCreated {
            branch: Some("main".into()),
            tag: "v1".into(),
        },
        Change::StashCreated {
            branch: Some("main".into()),
        },
    ];
    for change in &changes {
        let info = crate::KINDS
            .iter()
            .find(|k| k.id == change.kind())
            .expect("kind exists");
        let payload = change.payload("/work/app");
        assert_eq!(keys(&payload), names(info.payload), "{}", change.kind());
    }
}

#[test]
fn payload_values() {
    let commit = Change::Commit {
        branch: Some("main".into()),
        commit: Some("abc1234".into()),
        message: Some("Fix y".into()),
    };
    let p = commit.payload("/work/app");
    assert_eq!(p["repo"], "/work/app");
    assert_eq!(p["branch"], "main");
    assert_eq!(p["commit"], "abc1234");
    assert_eq!(p["message"], "Fix y");

    let switched = Change::BranchSwitched {
        from: "a".into(),
        to: "b".into(),
        commit: None,
    };
    let p = switched.payload("/work/app");
    assert_eq!(p["branch"], "b");
    assert_eq!(p["from"], "a");
    assert_eq!(p["to"], "b");
    assert_eq!(p["commit"], "");

    let conflict = Change::MergeConflict {
        branch: None,
        files: 3,
    };
    let p = conflict.payload("/work/app");
    assert_eq!(p["files"], 3);
    assert_eq!(p["branch"], "");
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-git`. It should change nothing.

Note on `check-crates.mjs`: `check-crates.mjs` prints warnings, not errors, about template text. The ones it may print for this crate until slice C are: `README.md still has template text`, `src/lib.rs still has template code`, `src/os.rs still has template code`. Warnings are fine here; only errors fail the check. Do not try to remove them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes: 12 tests in the library, 1 in `readme`, `live` shows 1 ignored
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (see the note under Steps about the warnings it prints)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Payloads for the polled kinds (slice B).
- Adding fields to a payload or to `KINDS`. The test fails if they drift apart; fix the payload, not the list.
- `step` (T06 to T08).

## If stuck

- If the test `payloads_carry_exactly_the_fields_listed_in_kinds` fails, the code differs from step 1. Re-copy it. Do not edit `KINDS`.
- If `Value::from(*files)` does not compile, `files` must be `u32`; check T04's `Change::MergeConflict`.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
