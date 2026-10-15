---
id: T25
epic: 001-trigger-core
title: Add download finished to the logic
status: done
depends_on: [T24]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-fs/src/logic.rs
  - src-tauri/crates/trigger-fs/src/logic/tests.rs
---

## Goal

`logic.rs` also reports `fs.download-finished` for a file that was just created or saved after a `.crdownload`, `.part` or `.download` file of the same name went away. The state machine of Decision 22 is complete.

## Read first

- `plans/001-trigger-core/epic.md` Decision 22, the "Download finished" bullet
- `src-tauri/crates/trigger-fs/src/logic.rs` where `partials` is filled by `ingest` since T22

## Steps

1. In `src-tauri/crates/trigger-fs/src/logic.rs`, directly under the line `const PARTIAL_SUFFIXES: [&str; 3] = [".crdownload", ".part", ".download"];` add a blank line and then:

```rust
/// How long after its last event a partial download file still counts as the
/// start of a download.
const DOWNLOAD_WINDOW_MS: u64 = 10_000;
```

2. In the same file, replace everything from the line `fn settle(` up to, but not including, the line `impl Obs {` with the text below. That region is `settle` and `pair_renames` today. The new region keeps `pair_renames` as it was and changes `settle`, and adds `finished_downloads` (Decision 22, "Download finished"): a file that was just created or saved is also a finished download when a partial file named like it plus `.crdownload`, `.part` or `.download` had events within the last 10 s and is gone now. `settle` first forgets partials older than 10 s. Each partial is used once, so a later save of the same file is not a second download. Partial files are never reported themselves because the default ignores drop them.

Current region, for finding it:

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

New region:

```rust
fn settle(state: &mut State, armed: &Armed, now: u64, is_file: &dyn Fn(&Path) -> bool) -> Vec<Obs> {
    state
        .partials
        .retain(|(_, at)| now.saturating_sub(*at) <= DOWNLOAD_WINDOW_MS);
    let mut resolved = resolve(state, armed, now, is_file);
    let mut obs = pair_renames(state, &mut resolved);
    let finished = finished_downloads(state, &resolved, is_file);
    obs.extend(resolved.removed.into_iter().map(|r| Obs::Removed(r.path)));
    obs.extend(resolved.created.into_iter().map(|r| Obs::Created(r.path)));
    obs.extend(resolved.saved.into_iter().map(|r| Obs::Saved(r.path)));
    obs.extend(finished);
    obs
}

/// A file that was just created or saved is a finished download when a
/// partial file with its name plus `.crdownload`, `.part` or `.download` had
/// events lately and is gone now. Each partial is used once.
fn finished_downloads(
    state: &mut State,
    resolved: &Resolved,
    is_file: &dyn Fn(&Path) -> bool,
) -> Vec<Obs> {
    let mut finished = Vec::new();
    let mut used: Vec<PathBuf> = Vec::new();
    for r in resolved.created.iter().chain(resolved.saved.iter()) {
        let name = r.path.to_string_lossy().to_lowercase();
        let partial = state.partials.iter().map(|(p, _)| p).find(|p| {
            let p = p.to_string_lossy().to_lowercase();
            PARTIAL_SUFFIXES.iter().any(|s| p == format!("{name}{s}"))
        });
        if let Some(partial) = partial {
            if !is_file(partial) {
                used.push(partial.clone());
                finished.push(Obs::DownloadFinished(r.path.clone()));
            }
        }
    }
    state.partials.retain(|(p, _)| !used.contains(p));
    finished
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

3. Append these tests to the end of `src-tauri/crates/trigger-fs/src/logic/tests.rs`, after one blank line.

```rust
#[test]
fn chrome_download_is_created_and_download_finished() {
    // x.zip.crdownload is written for a while, then renamed to x.zip.
    let mut fx = Fx::new("fs.download-finished", json!({}), &[]);
    fx.put("x.zip.crdownload");
    fx.raw(1000, RawKind::Create, &["x.zip.crdownload"]);
    fx.raw(3000, RawKind::Modify, &["x.zip.crdownload"]);
    assert!(fx.tick(3600).is_empty(), "the partial file is ignored");
    fx.raw(5000, RawKind::Modify, &["x.zip.crdownload"]);
    fx.delete("x.zip.crdownload");
    fx.put("x.zip");
    fx.raw(6000, RawKind::Name, &["x.zip.crdownload"]);
    fx.raw(6000, RawKind::Name, &["x.zip"]);
    assert_eq!(
        fx.tick(6600),
        vec![
            Obs::Created(fx.p("x.zip")),
            Obs::DownloadFinished(fx.p("x.zip"))
        ]
    );
    assert!(fx.tick(7000).is_empty());
}

#[test]
fn a_finished_download_over_an_existing_file_is_saved_and_finished() {
    let mut fx = Fx::new("fs.download-finished", json!({}), &["x.zip"]);
    fx.put("x.zip.part");
    fx.raw(1000, RawKind::Create, &["x.zip.part"]);
    fx.delete("x.zip.part");
    fx.raw(2000, RawKind::Name, &["x.zip.part"]);
    fx.raw(2000, RawKind::Name, &["x.zip"]);
    assert_eq!(
        fx.tick(2600),
        vec![
            Obs::Saved(fx.p("x.zip")),
            Obs::DownloadFinished(fx.p("x.zip"))
        ]
    );
}

#[test]
fn partial_suffixes_match_in_any_case() {
    let mut fx = Fx::new("fs.download-finished", json!({}), &[]);
    fx.put("Photo.JPG.CRDOWNLOAD");
    fx.raw(1000, RawKind::Create, &["Photo.JPG.CRDOWNLOAD"]);
    fx.delete("Photo.JPG.CRDOWNLOAD");
    fx.put("Photo.JPG");
    fx.raw(2000, RawKind::Name, &["Photo.JPG"]);
    let out = fx.tick(2600);
    assert!(out.contains(&Obs::DownloadFinished(fx.p("Photo.JPG"))));
}

#[test]
fn an_old_partial_does_not_make_a_download() {
    let mut fx = Fx::new("fs.download-finished", json!({}), &[]);
    fx.put("x.zip.crdownload");
    fx.raw(1000, RawKind::Create, &["x.zip.crdownload"]);
    fx.delete("x.zip.crdownload");
    assert!(fx.tick(2000).is_empty());
    fx.put("x.zip");
    fx.raw(20_000, RawKind::Create, &["x.zip"]);
    assert_eq!(fx.tick(20_600), vec![Obs::Created(fx.p("x.zip"))]);
}

#[test]
fn a_partial_that_still_exists_is_not_finished() {
    let mut fx = Fx::new("fs.download-finished", json!({}), &[]);
    fx.put("x.zip.part");
    fx.put("x.zip");
    fx.raw(1000, RawKind::Create, &["x.zip.part"]);
    fx.raw(1000, RawKind::Create, &["x.zip"]);
    assert_eq!(fx.tick(1600), vec![Obs::Created(fx.p("x.zip"))]);
}

#[test]
fn a_later_save_of_the_downloaded_file_is_not_a_second_download() {
    let mut fx = Fx::new("fs.download-finished", json!({}), &[]);
    fx.put("x.zip.part");
    fx.raw(1000, RawKind::Create, &["x.zip.part"]);
    fx.delete("x.zip.part");
    fx.put("x.zip");
    fx.raw(2000, RawKind::Name, &["x.zip"]);
    assert_eq!(fx.tick(2600).len(), 2);
    fx.raw(4000, RawKind::Modify, &["x.zip"]);
    assert_eq!(fx.tick(4600), vec![Obs::Saved(fx.p("x.zip"))]);
}

#[test]
fn only_the_download_kind_fires_download_finished() {
    let mut fx = Fx::new("fs.file-created", json!({}), &[]);
    fx.put("x.zip.part");
    fx.raw(1000, RawKind::Create, &["x.zip.part"]);
    fx.delete("x.zip.part");
    fx.put("x.zip");
    fx.raw(2000, RawKind::Name, &["x.zip"]);
    let out = fx.step(Signal::Tick { now: 2600 });
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, "fs.file-created");
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-fs`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-fs --lib logic` passes: 29 tests
- [ ] `cd src-tauri && cargo test -p trigger-fs` passes (37 in the library)
- [ ] `wc -l src-tauri/crates/trigger-fs/src/logic.rs` prints fewer than 400
- [ ] `cd src-tauri && cargo clippy -p trigger-fs --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-fs --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Safari `.download` bundles (they are folders; a Known limit in the README).
- Treating `.tmp` as a download suffix. Only the three suffixes in `PARTIAL_SUFFIXES`.
- Changing the 10 s window or the settle logic.
- Removing the `#![allow(dead_code)]` line (T27).
- Editing the README (T29).

## If stuck

- If the "Current region" text is not what the file has between `fn settle(` and `impl Obs {`, T24 did not land as written. Set status blocked.
- If `check-crates.mjs` warns that `tests.rs` has more than 400 lines, that is only a warning: leave it and carry on.
- If a test fails, re-copy the code of step 2 or 3; never change an expected value in a test.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
