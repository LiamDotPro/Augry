---
id: T08
epic: 003-trigger-git
title: Add tag and stash to step and test the order
status: todo
depends_on: [T07]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/logic.rs
  - src-tauri/crates/trigger-git/src/logic/tests.rs
---

## Goal

`step` also reports `tag-created` and `stash-created`, `git gc` and `git fetch` are proved to fire nothing, and tests pin the order when several changes land in one window.

## Read first

- `plans/003-trigger-git/epic.md` Decisions 8 and 9
- `plans/003-trigger-git/research/R01-git-files.md` "Diff rules" (tag, stash, order, gc, fetch)
- `src-tauri/crates/trigger-git/src/logic.rs` the `step` function from T07

## Steps

1. In `src-tauri/crates/trigger-git/src/logic.rs`, find the line `    // T08 adds the new tag and the new stash here.` Replace that one line with this block.

```rust
    // Only the first new tag by name: the hub's cool down would drop a second
    // event for the same rule anyway.
    let new_tag = next
        .refs
        .keys()
        .filter(|name| name.starts_with("refs/tags/") && !prev.refs.contains_key(*name))
        .map(|name| name.trim_start_matches("refs/tags/"))
        .next();
    if let Some(tag) = new_tag {
        out.push(Change::TagCreated {
            branch: next.branch.clone(),
            tag: tag.to_string(),
        });
    }

    // A stash that appeared, or one pushed on top of another. Removing the
    // last stash (pop or drop) removes the ref and fires nothing.
    if let Some(stash) = next.refs.get("refs/stash") {
        if prev.refs.get("refs/stash") != Some(stash) {
            out.push(Change::StashCreated {
                branch: next.branch.clone(),
            });
        }
    }
```

Choices made by the epic: at most one `TagCreated` per step (the first new tag by name), because the hub's two second cool down would drop a second one anyway. `refs/stash` appearing or changing sha is `StashCreated`; it disappearing is not. `git gc` rewrites `packed-refs` but leaves the ref map unchanged, and `git fetch` only touches remote refs, which are never in the snapshot, so both fire nothing with no special code.

2. In `src-tauri/crates/trigger-git/src/logic/tests.rs`, append this to the end of the file, after a blank line.

```rust
fn with_ref(mut snap: RepoSnapshot, name: &str, sha: &str) -> RepoSnapshot {
    snap.refs.insert(name.into(), sha.into());
    snap
}

#[test]
fn a_new_tag_fires_tag_created() {
    let prev = with_ref(on("main", "aaaaaaa1"), "refs/heads/main", "aaaaaaa1");
    let next = with_ref(prev.clone(), "refs/tags/v1.0", "aaaaaaa1");
    assert_eq!(
        step(&prev, &next),
        vec![Change::TagCreated {
            branch: Some("main".into()),
            tag: "v1.0".into()
        }]
    );
}

#[test]
fn several_new_tags_fire_once_for_the_first_by_name() {
    let prev = on("main", "aaaaaaa1");
    let next = with_ref(
        with_ref(prev.clone(), "refs/tags/v2", "x"),
        "refs/tags/v1",
        "x",
    );
    assert_eq!(
        step(&prev, &next),
        vec![Change::TagCreated {
            branch: Some("main".into()),
            tag: "v1".into()
        }]
    );
}

#[test]
fn moving_or_deleting_a_tag_fires_nothing() {
    let prev = with_ref(on("main", "aaaaaaa1"), "refs/tags/v1", "x");
    let moved = with_ref(on("main", "aaaaaaa1"), "refs/tags/v1", "y");
    assert!(step(&prev, &moved).is_empty());
    assert!(step(&prev, &on("main", "aaaaaaa1")).is_empty());
}

#[test]
fn a_new_branch_ref_alone_is_not_a_tag() {
    let prev = on("main", "aaaaaaa1");
    let next = with_ref(prev.clone(), "refs/heads/other", "aaaaaaa1");
    assert!(step(&prev, &next).is_empty());
}

#[test]
fn a_stash_fires_when_the_ref_appears_or_changes() {
    let prev = on("main", "aaaaaaa1");
    let stashed = with_ref(prev.clone(), "refs/stash", "s1");
    assert_eq!(
        step(&prev, &stashed),
        vec![Change::StashCreated {
            branch: Some("main".into())
        }]
    );
    let again = with_ref(prev.clone(), "refs/stash", "s2");
    assert_eq!(kinds(&step(&stashed, &again)), vec!["git.stash-created"]);
}

#[test]
fn popping_or_dropping_the_last_stash_fires_nothing() {
    let prev = with_ref(on("main", "aaaaaaa1"), "refs/stash", "s1");
    let next = on("main", "aaaaaaa1");
    assert!(step(&prev, &next).is_empty());
}

#[test]
fn gc_and_fetch_fire_nothing() {
    // gc moves loose refs into packed-refs: the ref map is the same.
    // fetch only changes remote refs, which are never in the snapshot.
    let mut prev = on("main", "aaaaaaa1");
    prev.refs
        .insert("refs/heads/main".into(), "aaaaaaa1".into());
    prev.refs.insert("refs/tags/v1".into(), "t1".into());
    prev.refs.insert("refs/stash".into(), "s1".into());
    assert!(step(&prev, &prev.clone()).is_empty());
}

#[test]
fn everything_in_one_window_comes_in_the_documented_order() {
    let prev = on("main", "aaaaaaa1");
    let mut next = with_ref(
        with_ref(on("feature", "bbbbbbb2"), "refs/tags/v1", "x"),
        "refs/stash",
        "s1",
    );
    next.merge_head = Some("ccccccc3".into());
    next.unmerged = 2;
    assert_eq!(
        kinds(&step(&prev, &next)),
        vec![
            "git.merge-conflict",
            "git.branch-switched",
            "git.tag-created",
            "git.stash-created"
        ]
    );
}

#[test]
fn a_commit_and_a_tag_in_one_window_fire_both_commit_first() {
    let prev = on("main", "aaaaaaa1");
    let mut next = with_ref(on("main", "bbbbbbb2"), "refs/tags/v1", "x");
    next.reflog_subject = Some("commit: release".into());
    assert_eq!(
        kinds(&step(&prev, &next)),
        vec!["git.commit", "git.tag-created"]
    );
}

#[test]
fn a_pull_a_tag_and_a_stash_come_in_order() {
    let prev = on("main", "aaaaaaa1");
    let mut next = with_ref(
        with_ref(on("main", "bbbbbbb2"), "refs/tags/v1", "x"),
        "refs/stash",
        "s1",
    );
    next.reflog_subject = Some("pull: Fast-forward".into());
    assert_eq!(
        kinds(&step(&prev, &next)),
        vec!["git.pulled", "git.tag-created", "git.stash-created"]
    );
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-git`. It should change nothing.

4. Run `wc -l src-tauri/crates/trigger-git/src/logic.rs`. It prints under 400. The tests file is longer and `check-crates.mjs` prints a warning for it (`src/logic/tests.rs has ... lines; consider splitting it`); that warning is expected and is not an error.

Note on `check-crates.mjs`: `check-crates.mjs` prints warnings, not errors, about template text. The ones it may print for this crate until slice C are: `README.md still has template text`, `src/lib.rs still has template code`, `src/os.rs still has template code`. Warnings are fine here; only errors fail the check. Do not try to remove them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes: 39 tests in the library, 1 in `readme`, `live` shows 1 ignored
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (see the note under Steps about the warnings it prints)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Parsing `packed-refs` or special handling for gc or fetch. The ref map already covers both.
- Firing for every new tag, or detecting a stash drop that leaves an older stash behind. That case reports `StashCreated` (the ref changes sha); the epic accepts it.
- Splitting `tests.rs` into several files.
- Any change outside the two marked gaps and the tests file.

## If stuck

- If a test fails, compare the code with step 1 character by character; do not change the test.
- If the placeholder comment `// T08 adds ...` is missing, T06 did not land as written. Set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
