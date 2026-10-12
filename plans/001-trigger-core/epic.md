---
id: 001-trigger-core
title: Trigger contract, hub, file watcher and app wiring
status: draft
depends_on: []
---

# 001 Trigger contract, hub, file watcher and app wiring

## Goal

A process definition saved in `<app config dir>/processes/<id>.json` with a
`triggers` list can be armed from Settings. When a matching file changes, the
app shows a toast ("Trigger fs.file-saved fired for Keep the docs tidy") and
logs it. Behind that: `src-tauri` is a Cargo workspace, the crates
`trigger-core` (the contract and test kit), `trigger-hub` (registry, rules,
cool down) and `trigger-fs` (the reference source) exist with tests, and CI
runs them on macOS, Windows and Linux. Every later source epic adds one crate
on exactly this pattern and registers it with one line.

Source: `docs/process-triggers.md` Part 2, which this epic implements. The
types there are the contract; this file writes them out in full.

## Non goals

- Running a process when a trigger fires. The run engine is a later epic;
  this epic ends at the `trigger://fired` event and a toast.
- A process editor, a trigger picker, or any screen beyond two rows in the
  existing Settings dialog.
- Any source other than `trigger-fs`. Triggers from inside the app are epic
  002; every other source has its own epic.
- Syncing definitions with the cloud. Definitions are local files here; the
  sync epic later writes into the same folder.
- OS permission dialogs. `trigger-fs` needs none on Windows and Linux. On
  macOS the OS itself asks on first access to Desktop, Documents or Downloads,
  which is accepted for now and documented.

## Decisions

1. **Workspace.** `src-tauri/Cargo.toml` gains `[workspace] members =
   ["crates/*"]` and `[workspace.dependencies]` for `serde`, `serde_json`,
   `thiserror`, `notify`, `uuid` and the versions the lock file already has.
   The app crate stays the workspace root package. CI and docs use `cargo test
   --workspace`; whether `default-members` is also set follows R01.
2. **Three crates**, `publish = false`, edition 2021, under `src-tauri/crates/`:
   `trigger-core`, `trigger-hub`, `trigger-fs`. Lib names `trigger_core`,
   `trigger_hub`, `trigger_fs`. Each has a README with the kinds table.
3. **The contract** is the Shape below, in `trigger-core`. `Platforms` is a
   hand written bit set (no `bitflags` dependency). `Sink` is
   `std::sync::mpsc::Sender<Event>`. `Handle` stops on drop.
4. **The test kit** `trigger_core::testkit` (behind no feature; it is small)
   has `RecordingSink`, `rule()`, `Clock` with `FixedClock`, and `FakeSource`.
5. **README tables are checked, not written.** `trigger_core::kinds_markdown`
   renders `KINDS` as a markdown table. Each source crate has
   `tests/readme.rs` asserting its README contains that table between
   `<!-- kinds -->` and `<!-- /kinds -->`, and `examples/readme.rs` printing it.
6. **`trigger-fs` uses `notify` 8** through `notify::recommended_watcher`, with
   its own debounce and settle state machine in `logic.rs` (pure, tested).
   No debouncer crate. Globs use `globset` at the version R02 finds. Default
   ignores: `.git/`, `node_modules/`, `target/`, `.DS_Store`, and the partial
   download suffixes `.crdownload`, `.part`, `.download`, `.tmp`.
7. **`trigger-fs` kinds** are exactly: `fs.file-saved`, `fs.file-created`,
   `fs.file-removed`, `fs.file-renamed`, `fs.download-finished`. The `when`
   fields: `path` (required, absolute after variables), `glob` (default
   `**/*`), `ignore` (list of globs, added to the defaults), `settle_ms`
   (default 500). Unknown fields are a `RuleError`.
8. **Payload** of every `fs.*` event: `path`, `name`, `relative` (to
   `when.path`), `ext`. `fs.file-renamed` adds `from`. Never file contents.
9. **No replay.** `trigger-fs` fires only for changes after `start`. A file
   that already exists is not "created".
10. **Local definitions** live in `<app config dir>/processes/<id>.json`, the
    `id` being the file stem. `src-tauri/src/triggers/store.rs` reads them and
    accepts a file when it is a JSON object with integer `schemaVersion` 1 and
    a non empty `name`; everything else is kept as is and `triggers`, when
    present, is an array of `{ id, kind, when }`. A file that fails is skipped
    and named in `triggers_status`. The folder is created on first use.
11. **Arming is local config.** `Config` gains `triggers: { enabled: bool,
    armed: Vec<String> }` (process ids), default off and empty, in
    `config.json`, never in the session or the cloud.
12. **Variables** in `when` string values are expanded by
    `src-tauri/src/triggers/variables.rs`: `${home}`, `${downloads}`,
    `${notes.root}`, `${screenshots}` (per platform, R05) and `${repo.root}`,
    which expands to one rule per distinct repository root among the open
    panes (`GitInfo.root`), with the rule id suffixed `#<n>`. A rule whose
    variable has no value on this machine is skipped with a reason.
13. **The service.** `src-tauri/src/triggers/mod.rs` has `TriggerService` in
    `AppState`, built with `sources()` which lists `trigger_fs::FsSource`
    behind the cargo feature `trigger-fs` (on by default). `refresh()`
    recomputes rules from config, the store and the open panes, and calls
    `hub.apply`. The app calls `refresh()` after config changes, after a
    terminal is created or closed, after a cwd change event, and on start.
14. **Hub rules.** Cool down two seconds per rule; events inside the cool down
    are dropped and counted, and the next `Fired` carries `coalesced`. A
    source whose `start` fails is reported in status and does not stop the
    others. The hub thread is named `trigger-hub`.
15. **Commands and event.** `triggers_catalogue`, `triggers_check`,
    `triggers_status`, `triggers_arm` as in the Shape. One event,
    `trigger://fired`. All added to `src/ipc/types.ts` and section 10 of the
    technical design.
16. **UI.** The Settings dialog gains a "Processes" row group: a toggle
    "Process triggers (experimental)" and, when on, the list of local
    definitions each with an "Armed" checkbox and a one line status
    (active rules, skipped rules with reason). It reuses `.row`, `.field` and
    `.segmented` from `overlays.css`; no new screen and no design export.
    A fired trigger shows a notice toast: the reducer gains `ui/notice`, the
    same toast as errors in a neutral colour, auto dismissed after six seconds.
17. **CI.** `ci.yml` runs, in `src-tauri`: `cargo fmt --check`, `cargo clippy
    -p trigger-core -p trigger-hub -p trigger-fs --all-targets -- -D warnings`,
    `cargo test --workspace`, and the live tests `cargo test -p trigger-fs --
    --ignored` on all three runners (R04 confirms they can).
18. **No new frontend dependencies.** New Rust dependencies: `globset` only.
19. **Every crate comes from the template.** `trigger-fs` is created with
    `node plans/_tools/new-trigger-crate.mjs fs "<summary>"` from
    `plans/_templates/trigger-crate/` and then filled in, so the template is
    proven by the first real crate. If the template turns out wrong against the
    real `trigger-core`, the same card fixes the template.
20. **Isolation is checked.** `plans/_tools/check-crates.mjs` runs in CI after
    the Rust checks and must report no errors. `trigger-core` and `trigger-hub`
    have a README with at least a Develop section; `trigger-fs` has all eight
    sections and no template text left.
21. **`os.rs` per platform.** `trigger-fs` keeps one `os.rs` because `notify`
    hides the platform differences. That is the expected case for crates built on
    a cross platform library.

## Shape

### Workspace (`src-tauri/Cargo.toml`, added)

```toml
[workspace]
members = ["crates/*"]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
notify = "8"
uuid = { version = "1", features = ["v4"] }
globset = "<R02>"

[features]
default = ["trigger-fs"]
trigger-fs = ["dep:trigger-fs"]

[dependencies]
trigger-core = { path = "crates/trigger-core" }
trigger-hub = { path = "crates/trigger-hub" }
trigger-fs = { path = "crates/trigger-fs", optional = true }
```

### `trigger-core/src/lib.rs`

```rust
//! The contract between trigger sources and whatever runs them. A source
//! depends on this crate only. See docs/process-triggers.md Part 2.

pub mod testkit;

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
    pub const fn or(self, other: Platforms) -> Platforms { Platforms(self.0 | other.0) }
    pub const fn has(self, other: Platforms) -> bool { self.0 & other.0 == other.0 }
    /// The platform this binary runs on.
    pub fn here() -> Platforms { /* cfg! per target_os */ }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    Accessibility, ScreenRecording, Location, Calendar, Reminders,
    Folder(&'static str), Microphone, Camera, Notifications,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldInfo { pub name: &'static str, pub doc: &'static str, pub required: bool }

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
    pub fn new(stop: impl FnOnce() + Send + 'static) -> Handle { Handle(Some(Box::new(stop))) }
}
impl Drop for Handle { fn drop(&mut self) { if let Some(stop) = self.0.take() { stop() } } }

pub trait Source: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn kinds(&self) -> &'static [KindInfo];
    fn availability(&self) -> Availability;
    fn check(&self, rule: &Rule) -> Result<(), RuleError>;
    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError>;
}

/// Renders a kinds table for a README. Checked by each crate's tests/readme.rs.
pub fn kinds_markdown(kinds: &[KindInfo]) -> String { /* id | title | platforms | permission | when | payload */ }
```

### `trigger-core/src/testkit.rs`

```rust
pub struct RecordingSink { pub tx: Sink, rx: Receiver<Event> }
impl RecordingSink {
    pub fn new() -> Self;
    pub fn wait_for(&self, kind: &str, timeout: Duration) -> Option<Event>;
    pub fn drain(&self) -> Vec<Event>;
}
pub fn rule(id: &str, kind: &str, when: serde_json::Value) -> Rule;
pub trait Clock: Send + Sync { fn now(&self) -> SystemTime; }
pub struct SystemClock; pub struct FixedClock(pub Mutex<SystemTime>);
/// A source for the hub's tests: records the rules it was started with and
/// fires an event when told to.
pub struct FakeSource { pub id: &'static str, pub kinds: &'static [KindInfo], pub started: Mutex<Vec<Vec<Rule>>>, pub availability: Availability }
impl FakeSource { pub fn fire(&self, event: Event); }
```

### `trigger-hub/src/lib.rs`

```rust
pub struct Hub { /* sources, running handles per source id, cool downs, the drain thread */ }

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo { pub id: &'static str, pub kinds: Vec<KindInfo>, pub availability: Availability }

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fired { pub event: Event, pub coalesced: u32 }

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum RuleState { Active, Rejected { reason: String }, SourceFailed { reason: String } }

impl Hub {
    pub fn new(sources: Vec<Box<dyn Source>>) -> Hub;
    pub fn with_cool_down(self, d: Duration) -> Hub;
    pub fn catalogue(&self) -> Vec<SourceInfo>;
    pub fn check(&self, rule: &Rule) -> Result<(), RuleError>;
    /// Starts, restarts or stops sources so that exactly these rules are live.
    pub fn apply(&self, rules: Vec<Rule>) -> BTreeMap<RuleId, RuleState>;
    pub fn publish(&self, event: Event);
    pub fn subscribe(&self) -> Receiver<Fired>;
}
```

### `trigger-fs/src/rule.rs`

```rust
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct When {
    pub path: PathBuf,
    #[serde(default = "default_glob")] pub glob: String,
    #[serde(default)] pub ignore: Vec<String>,
    #[serde(default = "default_settle")] pub settle_ms: u64,
}
pub fn parse(rule: &Rule) -> Result<When, RuleError>;
```

### App side

```
src-tauri/src/triggers/
  mod.rs          TriggerService { hub, store, armed, last_fired }, sources(), refresh(), the forward thread
  store.rs        LocalDefinition { id, name, triggers: Vec<DefinitionTrigger>, problem: Option<String> }, load(dir)
  variables.rs    expand(when, &Context) -> Vec<(suffix, when)> | Skipped(reason)
```

`Config` (`src-tauri/src/state/mod.rs`) adds:

```rust
#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TriggersConfig { pub enabled: bool, pub armed: Vec<String> }
```

Commands (`src/ipc/types.ts` entries and Rust names):

| Command | In | Out |
|---|---|---|
| `triggers_catalogue` | `{}` | `SourceInfo[]` |
| `triggers_check` | `{ rule: Rule }` | `{ ok: true } \| { ok: false, reason }` |
| `triggers_status` | `{}` | `{ enabled, processes: { id, name, armed, problem?, rules: { id, kind, state }[] }[] }` |
| `triggers_arm` | `{ processId, armed }` | `void` |

Event `trigger://fired`: `{ processId, processName, ruleId, kind, at, payload, coalesced }`.

Reducer: `triggers: { status: TriggersStatus | null }`, actions
`triggers/status` and `ui/notice`.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A (T01 to T09) | workspace, `trigger-core` types, test kit, `kinds_markdown`, README | |
| B (T10 to T19) | `trigger-hub`: registry, apply, cool down, publish, subscribe, tests with `FakeSource` | A |
| C (T20 to T29) | `trigger-fs`: scaffold from the template, `rule.rs`, `logic.rs` (debounce and settle), `os.rs` over notify, live tests, README | A |
| D (T30 to T39) | app: store, variables, service, config, commands, event, Settings rows, notice toast | B, C |
| E (T40 to T49) | CI (including `check-crates.mjs`), technical design sections 2, 3, 9, 10 and 14, repo map, architecture table | D |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | With `src-tauri/Cargo.toml` as both a package and a workspace root, do `npm run tauri dev` and `tauri build` work unchanged, and does `default-members = [".", "crates/*"]` make a bare `cargo test` cover every crate? | Decision 1 and every acceptance line | |
| R02 | Current `globset` version; does `**/*.md` match `docs\a.md` on Windows, or must paths be normalised to `/` first? | Decision 6 | |
| R03 | What `notify` 8 reports on each OS for a plain save, an atomic save (write temp then rename, as VS Code and vim do), a create and a delete | The matcher state machine in slice C | |
| R04 | Do GitHub's macOS, Windows and Ubuntu runners deliver file events to `notify` quickly enough for live tests with a 2 s timeout, and is the inotify watch limit on Ubuntu a problem? | Decision 17 | |
| R05 | Where screenshots land by default on each OS, and how to read the configured folder on macOS (`com.apple.screencapture location`) and Windows | `${screenshots}` in Decision 12 | |
| R06 | How `thiserror` 2 and `serde` derive behave on a `&'static str` payload (`Permission::Folder`) and `SystemTime` | The contract compiles as written | |

## Risks

- `notify` behaves differently per OS and editor; the matcher must be designed
  from R03's facts, not from assumptions, or `fs.file-saved` fires twice or not
  at all.
- Live tests on CI can be flaky under load. Keep timeouts generous and the
  number of live tests small.
- macOS shows its own folder access prompt the first time a rule watches
  Desktop, Documents or Downloads. Accepted, but it must be written down in
  the README of `trigger-fs` and the Settings status.
- The Settings rows are built without a design export, against `CLAUDE.md`'s
  rule that screens come from the design. Accepted for two rows; the picker
  UI later comes from the design.
- `${repo.root}` fan out recomputes on every pane change. Keep `refresh()`
  cheap: compare the new rule set with the last one and call `apply` only on
  a change.
