---
id: T24
epic: 001-trigger-core
title: Add rename pairing to the logic
status: todo
depends_on: [T23]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-fs/src/logic.rs
  - src-tauri/crates/trigger-fs/src/logic/tests.rs
---

## Goal

`logic.rs` reports `fs.file-renamed` with a `from` path when the OS gives a rename pair (Linux) or when exactly one file vanished and one appeared with rename events on both (macOS, Windows), and leaves everything else as removed plus created.

## Read first

- `plans/001-trigger-core/epic.md` Decision 22, the "Rename" bullet and the fixture list
- `plans/001-trigger-core/research/R03-notify-events.md` the rename and atomic save sequences per OS
- `src-tauri/crates/trigger-fs/src/logic.rs` the `settle` function you replace

## Steps

1. In `src-tauri/crates/trigger-fs/src/logic.rs`, replace everything from the line `fn settle(` up to, but not including, the line `impl Obs {` with the text below. That region is only the `settle` function today. The new `settle` calls `pair_renames`, which takes renames out of the resolved lists (Decision 22, "Rename"): first the `[from, to]` pairs that Linux reports as `Name(Both)`, then, for macOS and Windows where the two halves arrive as separate events, exactly one removed and exactly one created in the same batch that were both touched by a rename event. Anything else stays removed plus created. Renames are decided after the glob and ignore filter, so a rename to or from an ignored name is not a rename.

Current region, for finding it:

```rust
fn settle(state: &mut State, armed: &Armed, now: u64, is_file: &dyn Fn(&Path) -> bool) -> Vec<Obs> {
    let resolved = resolve(state, armed, now, is_file);
    let mut obs = Vec::new();
    obs.extend(resolved.removed.into_iter().map(|r| Obs::Removed(r.path)));
    obs.extend(resolved.created.into_iter().map(|r| Obs::Created(r.path)));
    obs.extend(resolved.saved.into_iter().map(|r| Obs::Saved(r.path)));
    obs
}
```

New region:

```rust
fn settle(state: &mut State, armed: &Armed, now: u64, is_file: &dyn Fn(&Path) -> bool) -> Vec<Obs> {
    let mut resolved = resolve(state, armed, now, is_file);
    let mut obs = pair_renames(state, &mut resolved);
    obs.extend(resolved.removed.into_iter().map(|r| Obs::Removed(r.path)));
    obs.extend(resolved.created.into_iter().map(|r| Obs::Created(r.path)));
    obs.extend(resolved.saved.into_iter().map(|r| Obs::Saved(r.path)));
    obs
}

/// Takes renames out of `resolved`. First the `[from, to]` pairs the OS gave
/// us (Linux). Then, for macOS and Windows where the halves arrive unpaired:
/// exactly one removed and exactly one created in the batch, both touched by
/// a rename event. Anything else stays removed plus created.
fn pair_renames(state: &mut State, resolved: &mut Resolved) -> Vec<Obs> {
    let mut renamed = Vec::new();
    let mut keep = Vec::new();
    for (from, to) in std::mem::take(&mut state.pairs) {
        let removed = resolved.removed.iter().position(|r| r.path == from);
        let created = resolved.created.iter().position(|r| r.path == to);
        if let (Some(i), Some(j)) = (removed, created) {
            resolved.removed.remove(i);
            resolved.created.remove(j);
            renamed.push(Obs::Renamed { from, to });
        } else if state.pending.contains_key(&from) || state.pending.contains_key(&to) {
            // One half has not settled yet: keep the pair for the next batch.
            keep.push((from, to));
        }
    }
    state.pairs = keep;
    if let ([removed], [created]) = (resolved.removed.as_slice(), resolved.created.as_slice()) {
        if removed.name && created.name {
            renamed.push(Obs::Renamed {
                from: removed.path.clone(),
                to: created.path.clone(),
            });
            resolved.removed.clear();
            resolved.created.clear();
        }
    }
    renamed
}
```

2. Append these tests to the end of `src-tauri/crates/trigger-fs/src/logic/tests.rs`, after one blank line. They replay R03: Windows `From`/`To`, macOS `Name(Any)` per path, Linux `Name(Both)`, and the Linux atomic save whose pair must not become a rename.

```rust
#[test]
fn windows_rename_from_and_to_is_one_renamed() {
    let mut fx = Fx::new("fs.file-renamed", json!({}), &["a.txt"]);
    fx.delete("a.txt");
    fx.put("b.txt");
    fx.raw(1000, RawKind::Name, &["a.txt"]);
    fx.raw(1000, RawKind::Name, &["b.txt"]);
    assert_eq!(
        fx.tick(1600),
        vec![Obs::Renamed {
            from: fx.p("a.txt"),
            to: fx.p("b.txt")
        }]
    );
}

#[test]
fn macos_rename_into_a_subfolder_is_one_renamed() {
    // R03: one Name(Any) per path, no pairing in the kind.
    let mut fx = Fx::new("fs.file-renamed", json!({}), &["a.txt"]);
    fx.delete("a.txt");
    fx.put("sub/a.txt");
    fx.raw(1000, RawKind::Name, &["a.txt"]);
    fx.raw(1000, RawKind::Name, &["sub/a.txt"]);
    let out = fx.tick(1600);
    assert_eq!(
        out,
        vec![Obs::Renamed {
            from: fx.p("a.txt"),
            to: fx.p("sub/a.txt")
        }]
    );
    let payload = out[0].payload(&fx.armed);
    assert_eq!(payload["from"], json!(fx.p("a.txt").to_string_lossy()));
    assert_eq!(payload["relative"], json!("sub/a.txt"));
}

#[test]
fn linux_rename_with_both_is_one_renamed() {
    let mut fx = Fx::new("fs.file-renamed", json!({}), &["a.txt"]);
    fx.delete("a.txt");
    fx.put("b.txt");
    fx.raw(1000, RawKind::Name, &["a.txt"]);
    fx.raw(1000, RawKind::Name, &["b.txt"]);
    fx.raw(1000, RawKind::Both, &["a.txt", "b.txt"]);
    assert_eq!(
        fx.tick(1600),
        vec![Obs::Renamed {
            from: fx.p("a.txt"),
            to: fx.p("b.txt")
        }]
    );
    assert!(fx.state.pairs.is_empty());
}

#[test]
fn both_pairs_two_removes_and_creates_without_guessing() {
    let mut fx = Fx::new("fs.file-renamed", json!({}), &["a.txt", "c.txt"]);
    fx.delete("a.txt");
    fx.delete("c.txt");
    fx.put("b.txt");
    fx.put("d.txt");
    fx.raw(1000, RawKind::Both, &["a.txt", "b.txt"]);
    fx.raw(1000, RawKind::Both, &["c.txt", "d.txt"]);
    let out = fx.tick(1600);
    assert_eq!(out.len(), 2);
    assert!(out.contains(&Obs::Renamed {
        from: fx.p("a.txt"),
        to: fx.p("b.txt")
    }));
    assert!(out.contains(&Obs::Renamed {
        from: fx.p("c.txt"),
        to: fx.p("d.txt")
    }));
}

#[test]
fn linux_atomic_save_with_both_is_one_saved() {
    // R03: Create temp, Data temp, Name(From) temp, Name(To) target,
    // Name(Both) [temp, target].
    let mut fx = Fx::new("fs.file-saved", json!({}), &["a.txt"]);
    fx.put("a.txt.swp-new");
    fx.raw(1000, RawKind::Create, &["a.txt.swp-new"]);
    fx.raw(1000, RawKind::Modify, &["a.txt.swp-new"]);
    fx.delete("a.txt.swp-new");
    fx.raw(1001, RawKind::Name, &["a.txt.swp-new"]);
    fx.raw(1001, RawKind::Name, &["a.txt"]);
    fx.raw(1001, RawKind::Both, &["a.txt.swp-new", "a.txt"]);
    assert_eq!(fx.tick(1600), vec![Obs::Saved(fx.p("a.txt"))]);
    assert!(fx.state.pairs.is_empty(), "the pair is forgotten");
}

#[test]
fn a_delete_and_an_unrelated_create_are_not_a_rename() {
    // macOS puts a stale rename flag on a deleted path, but a new file
    // has none.
    let mut fx = Fx::new("fs.file-renamed", json!({}), &["a.txt"]);
    fx.delete("a.txt");
    fx.put("b.txt");
    fx.raw(1000, RawKind::Remove, &["a.txt"]);
    fx.raw(1000, RawKind::Name, &["a.txt"]);
    fx.raw(1000, RawKind::Create, &["b.txt"]);
    assert_eq!(
        fx.tick(1600),
        vec![Obs::Removed(fx.p("a.txt")), Obs::Created(fx.p("b.txt"))]
    );
}

#[test]
fn two_removed_and_one_created_stay_separate() {
    let mut fx = Fx::new("fs.file-renamed", json!({}), &["a.txt", "b.txt"]);
    fx.delete("a.txt");
    fx.delete("b.txt");
    fx.put("c.txt");
    for rel in ["a.txt", "b.txt", "c.txt"] {
        fx.raw(1000, RawKind::Name, &[rel]);
    }
    let out = fx.tick(1600);
    assert_eq!(out.len(), 3);
    assert!(out.iter().all(|o| !matches!(o, Obs::Renamed { .. })));
}

#[test]
fn renaming_to_an_ignored_name_is_only_a_removal() {
    let mut fx = Fx::new("fs.file-renamed", json!({}), &["a.txt"]);
    fx.delete("a.txt");
    fx.put("a.txt~");
    fx.raw(1000, RawKind::Name, &["a.txt"]);
    fx.raw(1000, RawKind::Name, &["a.txt~"]);
    assert_eq!(fx.tick(1600), vec![Obs::Removed(fx.p("a.txt"))]);
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-fs`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-fs --lib logic` passes: 22 tests
- [ ] `cd src-tauri && cargo test -p trigger-fs` passes (30 in the library)
- [ ] `cd src-tauri && cargo clippy -p trigger-fs --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-fs --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Download finished (T25).
- Matching renames across more than one batch, or guessing among several removed and created files. Several of either stays removed plus created.
- Changing `ingest`, `resolve` or the payload.
- Removing the `#![allow(dead_code)]` line.
- Editing the README (T29).

## If stuck

- If the "Current region" text is not what the file has between `fn settle(` and `impl Obs {`, T22 or T23 did not land as written. Set status blocked.
- If a test fails, re-copy the code of step 1 or 2; never change an expected value in a test.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
