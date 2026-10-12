---
id: T23
epic: 001-trigger-core
title: Add glob and ignore filtering, payload and step
status: todo
depends_on: [T22]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-fs/src/logic.rs
  - src-tauri/crates/trigger-fs/src/logic/tests.rs
---

## Goal

`logic::step` now works for one rule: it filters decisions through the rule's `glob` and `ignore`, keeps only the rule's own kind, and builds the payload of Decision 8.

## Read first

- `plans/001-trigger-core/epic.md` Decisions 6 and 8
- `src-tauri/crates/trigger-fs/src/logic.rs` the file from T22 you edit in four places
- `src-tauri/crates/trigger-fs/src/rule.rs` `Parsed`, the compiled `glob` and `ignore` sets

## Steps

All edits are in `src-tauri/crates/trigger-fs/src/logic.rs` unless said otherwise.

1. Replace the stub `step` function, from its doc comment line `/// The events a signal fires for this rule: (kind, payload). ...` through its closing `}` (the text just above `/// Every observation a signal produces`), with the text below. It adds `accepts` (a path counts when it is under the root and passes `glob` and `ignore`, both matched against the path relative to the root; Decision 6) and the real `step`, which keeps only the observations of the rule's own kind and builds each payload.

Current text to replace:

```rust
/// The events a signal fires for this rule: (kind, payload). `is_file` says
/// whether a path is an existing regular file.
pub fn step(
    _state: &mut State,
    _armed: &Armed,
    _signal: Signal,
    _is_file: &dyn Fn(&Path) -> bool,
) -> Vec<(String, Payload)> {
    Vec::new()
}
```

New text:

```rust
/// True when the path is below the root and passes the rule's `glob` and
/// `ignore`. Both are matched against the path relative to the root.
fn accepts(armed: &Armed, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(&armed.root) else {
        return false;
    };
    !rel.as_os_str().is_empty() && armed.glob.is_match(rel) && !armed.ignore.is_match(rel)
}

/// The events a signal fires for this rule: (kind, payload). `is_file` says
/// whether a path is an existing regular file; directories are never
/// reported.
pub fn step(
    state: &mut State,
    armed: &Armed,
    signal: Signal,
    is_file: &dyn Fn(&Path) -> bool,
) -> Vec<(String, Payload)> {
    observe(state, armed, signal, is_file)
        .iter()
        .filter(|o| o.kind_id() == armed.kind)
        .map(|o| (o.kind_id().to_string(), o.payload(armed)))
        .collect()
}
```

2. In `resolve`, replace the doc comment above `fn resolve(` with:

```rust
/// Decides every pending path that has been quiet for `settle_ms`, using a
/// stat and the known set, and keeps the known set up to date. Paths that
/// the rule's glob and ignore reject are decided too (so the known set stays
/// right) but not returned.
```

and directly above the last two lines of that function (`    out` and `}`) add these three lines:

```rust
    out.created.retain(|r| accepts(armed, &r.path));
    out.saved.retain(|r| accepts(armed, &r.path));
    out.removed.retain(|r| accepts(armed, &r.path));
```

3. Insert this block directly above the line `#[cfg(test)]` at the bottom of the file, leaving one blank line before and after it. It names each observation's kind id and builds the payload of Decision 8: `path`, `name`, `relative` (with `/` separators), `ext`, and `from` for a rename.

```rust
impl Obs {
    pub fn kind_id(&self) -> &'static str {
        match self {
            Obs::Created(_) => "fs.file-created",
            Obs::Saved(_) => "fs.file-saved",
            Obs::Removed(_) => "fs.file-removed",
            Obs::Renamed { .. } => "fs.file-renamed",
            Obs::DownloadFinished(_) => "fs.download-finished",
        }
    }

    /// Decision 8: `path`, `name`, `relative`, `ext`, and `from` for a rename.
    pub fn payload(&self, armed: &Armed) -> Payload {
        let (path, from) = match self {
            Obs::Created(p) | Obs::Saved(p) | Obs::Removed(p) | Obs::DownloadFinished(p) => {
                (p, None)
            }
            Obs::Renamed { from, to } => (to, Some(from)),
        };
        let text = |s: &std::ffi::OsStr| Value::from(s.to_string_lossy().into_owned());
        let relative = path
            .strip_prefix(&armed.root)
            .map(|rel| {
                rel.components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .unwrap_or_default();
        let mut payload = Payload::new();
        payload.insert("path".into(), text(path.as_os_str()));
        payload.insert("name".into(), text(path.file_name().unwrap_or_default()));
        payload.insert("relative".into(), Value::from(relative));
        payload.insert("ext".into(), text(path.extension().unwrap_or_default()));
        if let Some(from) = from {
            payload.insert("from".into(), text(from.as_os_str()));
        }
        payload
    }
}
```

4. In `src-tauri/crates/trigger-fs/src/logic/tests.rs`, add this method to `impl Fx`, directly above the existing `fn observe(&mut self, signal: Signal) -> Vec<Obs> {`:

```rust
    fn step(&mut self, signal: Signal) -> Vec<(String, Payload)> {
        let disk = self.disk.clone();
        let is_file = move |p: &Path| disk.borrow().contains(p);
        step(&mut self.state, &self.armed, signal, &is_file)
    }
```

5. Append these three tests to the end of `src-tauri/crates/trigger-fs/src/logic/tests.rs`, after one blank line.

```rust
#[test]
fn glob_and_ignore_apply_when_emitting() {
    let mut fx = Fx::new(
        "fs.file-created",
        json!({ "glob": "**/*.md", "ignore": ["drafts/**"] }),
        &[],
    );
    for rel in ["a.md", "b.txt", "drafts/c.md", "d.md.tmp", ".git/e.md"] {
        fx.put(rel);
        fx.raw(1000, RawKind::Create, &[rel]);
    }
    assert_eq!(fx.tick(2000), vec![Obs::Created(fx.p("a.md"))]);
}

#[test]
fn step_keeps_only_the_rules_kind_and_builds_the_payload() {
    let mut fx = Fx::new("fs.file-saved", json!({}), &["docs/Plan.md"]);
    fx.raw(1000, RawKind::Modify, &["docs/Plan.md"]);
    let out = fx.step(Signal::Tick { now: 2000 });
    assert_eq!(out.len(), 1);
    let (kind, payload) = &out[0];
    assert_eq!(kind, "fs.file-saved");
    assert_eq!(payload["name"], json!("Plan.md"));
    assert_eq!(payload["relative"], json!("docs/Plan.md"));
    assert_eq!(payload["ext"], json!("md"));
    assert_eq!(
        payload["path"],
        json!(fx.p("docs/Plan.md").to_string_lossy())
    );
    assert!(!payload.contains_key("from"));

    // The same change is nothing to a rule of another kind.
    let mut other = Fx::new("fs.file-created", json!({}), &["docs/Plan.md"]);
    other.raw(1000, RawKind::Modify, &["docs/Plan.md"]);
    assert!(other.step(Signal::Tick { now: 2000 }).is_empty());
}

#[test]
fn a_file_without_an_extension_has_an_empty_ext() {
    let fx = Fx::saved();
    let payload = Obs::Saved(fx.p("Makefile")).payload(&fx.armed);
    assert_eq!(payload["ext"], json!(""));
    assert_eq!(payload["relative"], json!("Makefile"));
}
```

6. Run `cd src-tauri && cargo fmt -p trigger-fs`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-fs --lib logic` passes: 14 tests
- [ ] `cd src-tauri && cargo test -p trigger-fs` passes (22 in the library)
- [ ] `cd src-tauri && cargo clippy -p trigger-fs --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-fs --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Rename pairing and download finished (T24, T25).
- Applying `glob` or `ignore` in `ingest`. They are applied only when a decision is returned, so partial download names still reach the state machine (Decision 6).
- Changing the payload fields or their names (Decision 8).
- Removing the `#![allow(dead_code)]` line.
- Editing the README (T29).

## If stuck

- If step 1 finds no text matching "Current text to replace", T22 did not land or was edited. Set status blocked.
- If a test fails, re-copy the code of the step that changed it; never change an expected value in a test.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
