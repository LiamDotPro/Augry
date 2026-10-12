---
id: 001-trigger-core
title: Trigger contract, hub, file watcher and app wiring
status: building
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
   The app crate stays the workspace root package. `default-members = [".",
   "crates/*"]` is set too (R01), so a bare `cargo test` also covers every
   crate, but CI, docs and every acceptance line say `cargo test --workspace`.
   A glob member without a `Cargo.toml` breaks every cargo command, so the
   card that adds the `[workspace]` block also creates `trigger-core` (R01).
   The app's own dependency lines stay as they are; the app depends on the
   crates by path only from slice D.
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
6. **`trigger-fs` uses `notify` 8.2** through `notify::recommended_watcher`
   with `RecursiveMode::Recursive`, with its own settle state machine in
   `logic.rs` (pure, tested; Decision 22). No debouncer crate. Globs use
   `globset` 0.4.20, every pattern built with
   `GlobBuilder::new(p).literal_separator(true).case_insensitive(true)`, and
   matched against the path relative to the canonical `when.path` as a
   `&Path` (globset turns `\` into `/` itself on Windows, R02). Default
   ignores, exactly: `**/.git`, `**/.git/**`, `**/node_modules`,
   `**/node_modules/**`, `**/target`, `**/target/**`, `**/.DS_Store`,
   `**/*.crdownload`, `**/*.part`, `**/*.download`, `**/*.tmp`, `**/*~`,
   `**/*.swp`, `**/*.swx`. User `ignore` globs are added to these. Ignores and
   `glob` are applied when an observation is emitted, not when a raw event
   comes in, so partial download names still reach the state machine.
   `when.path` is canonicalised (`std::fs::canonicalize`) in `start` before
   `watch()`, because macOS reports `/private/var/...` for `/var/...` and
   Windows adds `\\?\` (R03); a path that cannot be canonicalised is a
   `SourceError::Os`. Glob sets are compiled once per rule in `start`; a bad
   user glob is a `RuleError` from `check`.
7. **`trigger-fs` kinds** are exactly: `fs.file-saved`, `fs.file-created`,
   `fs.file-removed`, `fs.file-renamed`, `fs.download-finished`. The `when`
   fields: `path` (required, absolute after variables), `glob` (default
   `**/*`), `ignore` (list of globs, added to the defaults), `settle_ms`
   (default 500). Unknown fields are a `RuleError`.
8. **Payload** of every `fs.*` event: `path` (canonical, absolute), `name`,
   `relative` (to the canonical `when.path`, with `/` separators), `ext`
   (without the dot, empty string when none). `fs.file-renamed` adds `from`
   (canonical, absolute). Never file contents.
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
    `${screenshots}` follows R05 with no new dependency (`dirs` 7 and
    `std::process::Command`): macOS reads `defaults read
    com.apple.screencapture location` (expand a leading `~`, trim a trailing
    `/`; unset or failed means `~/Desktop`); Windows reads `reg query` on
    `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\User Shell
    Folders` value `{B7BEDE81-DF94-4682-A7D8-57A52620B86F}` (expand `%VAR%`;
    missing means `dirs::picture_dir()/Screenshots`); Linux uses
    `dirs::picture_dir()/Screenshots` if it is a directory, else
    `dirs::picture_dir()`. The result has a value only when it is an existing
    directory; a configured macOS location that does not exist has no value
    (no fall back to Desktop, since screenshots would not land there). The
    string parsing (tilde, `reg` output, `%VAR%`) is in plain functions with
    tests that run on every OS.
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
    `cargo test --workspace`, a Linux only step printing the inotify limits,
    and the live tests `cargo test -p trigger-fs -- --ignored
    --test-threads=1` with `timeout-minutes: 10`, on all three runners. The
    toolchain step gains `components: rustfmt, clippy`. The exact YAML is in
    R04. No retry action; if macOS flakes in practice, that is a later fix.
    Live tests (five: created, saved, removed, glob excluded, atomic save)
    each use their own canonicalised temp dir under `std::env::temp_dir()`
    with a uuid, wait 400 ms after `start` before touching files, and wait
    for events with `RecordingSink::wait_for` and a 10 s ceiling. A negative
    check waits for a positive event on a sentinel file first, then asserts
    nothing else arrived (R04).
18. **No new frontend dependencies.** New Rust dependencies: `globset` only.
    Versions in `[workspace.dependencies]`: `notify = "8.2"`, `globset =
    "0.4.20"`; the others as the lock file has them. `uuid` is a
    dev-dependency of `trigger-fs` for temp dir names; no `tempfile`.
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
22. **The matcher decides by stat, not by event kind** (R03: on macOS a plain
    save arrives as `Create`, and rename halves are unpaired `Name(Any)`).
    `logic.rs` is pure and driven by the OS layer:
    - **Known set.** `start` walks each canonical `when.path` once (not
      descending into the default ignored directories) and seeds a set of
      existing file paths. That is the "existed before" every decision uses,
      and why nothing replays (Decision 9). The set is updated after every
      decision.
    - **Ingest.** Every raw event with kind `Create`, `Remove` or `Modify` (any
      sub kind) marks each of its paths pending with `last = now` and remembers
      whether a `Name` event was seen. `Access` events are ignored.
      `EventKind::Other` with the rescan flag re-seeds the known set and drops
      pending paths without emitting. Directories are never reported.
    - **Settle.** On a tick (the OS layer ticks every 100 ms), every pending
      path with `now - last >= settle_ms` is resolved in one batch using
      `exists` from the OS layer: not known and exists is *created*; known and
      exists is *saved*; known and gone is *removed*; not known and gone is
      dropped (a temp file that came and went).
    - **Rename.** In one batch, if exactly one path resolved *removed* and
      exactly one *created*, both with a `Name` event, they become one
      *renamed* (`from` the removed one). A Linux `Name(Both)` event pairs its
      two paths directly. Anything else stays removed plus created.
    - **Download finished.** A path `F` that resolves *created* or *saved*
      emits *download-finished* too when some path `F` + one of
      `.crdownload`, `.part`, `.download` was pending within the last 10 s and
      no longer exists. Each rule then emits only observations of its own
      kind.
    - **Tests** replay R03's sequences as fixtures with a fake clock: macOS
      plain save of a known file gives exactly one *saved*; macOS atomic save
      gives one *saved* and nothing for the temp file; Linux atomic save with
      `Both` gives one *saved*; Windows rename `From`/`To` gives one
      *renamed*; delete then recreate gives one *saved*; create plus quick
      writes gives one *created*; Chrome style `x.zip.crdownload` renamed to
      `x.zip` gives *created* and *download-finished*.
    Watching a very large tree makes the start walk slow; that is a Known
    limit in the README, not a cap.

## Shape

### Workspace (`src-tauri/Cargo.toml`, added)

The `[workspace]` and `[workspace.dependencies]` blocks go above `[lib]` in
slice A. The `[features]` block and the three path dependencies are added in
slice D, when the app starts using the crates.

```toml
[workspace]
members = ["crates/*"]
default-members = [".", "crates/*"]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
notify = "8.2"
uuid = { version = "1", features = ["v4"] }
globset = "0.4.20"

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

// Serialises as ["macos","windows","linux"] (R06; verified code in the note).
impl serde::Serialize for Platforms { /* serialize_seq of the names that are set */ }

/// serde helper: SystemTime as whole milliseconds since the Unix epoch (R06).
pub mod ms_since_epoch { /* serialize and deserialize, as in R06 */ }

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
    #[serde(with = "ms_since_epoch")]
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
impl Default for RecordingSink { fn default() -> Self { Self::new() } } // clippy new_without_default
pub fn rule(id: &str, kind: &str, when: serde_json::Value) -> Rule;
pub trait Clock: Send + Sync { fn now(&self) -> SystemTime; }
pub struct SystemClock; pub struct FixedClock(pub Mutex<SystemTime>);
/// A source for the hub's tests: records the rules it was started with and
/// fires an event when told to.
pub struct FakeSource {
    pub id: &'static str,
    pub kinds: &'static [KindInfo],
    pub started: Mutex<Vec<Vec<Rule>>>,
    pub availability: Availability,
    pub sink: Mutex<Option<Sink>>,   // set by start, used by fire (R06)
    pub fail_start: bool,            // start returns SourceError::Os when true
}
impl FakeSource { pub fn new(id: &'static str, kinds: &'static [KindInfo]) -> FakeSource; pub fn fire(&self, event: Event); }
```

A unit test in `trigger-core` pins the JSON the frontend sees:
`Platforms::ALL` is `["macos","windows","linux"]`, `Event.at` is a number,
`Permission::Folder("Downloads")` is `{"folder":"Downloads"}`.

Frontend types (R06): `at: number` (ms since epoch); `platforms: ("macos" |
"windows" | "linux")[]`; `permission: "accessibility" | "screenRecording" |
"location" | "calendar" | "reminders" | "microphone" | "camera" |
"notifications" | { folder: string } | null`.

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
| R01 | With `src-tauri/Cargo.toml` as both a package and a workspace root, do `npm run tauri dev` and `tauri build` work unchanged, and does `default-members = [".", "crates/*"]` make a bare `cargo test` cover every crate? | Decision 1 and every acceptance line | Yes to both (run). No change to `tauri.conf.json`, `build.rs` or npm scripts. Set `default-members`, still use `--workspace`. A glob member with no manifest breaks cargo, so workspace and first crate land together. |
| R02 | Current `globset` version; does `**/*.md` match `docs\a.md` on Windows, or must paths be normalised to `/` first? | Decision 6 | 0.4.20. globset normalises separators itself on Windows (source). Use `literal_separator(true)`, relative `&Path`, `**/name` plus `**/name/**` for directory ignores. |
| R03 | What `notify` 8 reports on each OS for a plain save, an atomic save (write temp then rename, as VS Code and vim do), a create and a delete | The matcher state machine in slice C | notify 8.2.0. Event kinds are unreliable (macOS plain save looks like Create), so decide at settle by stat against a known set (Decision 22). Canonicalise `when.path`. |
| R04 | Do GitHub's macOS, Windows and Ubuntu runners deliver file events to `notify` quickly enough for live tests with a 2 s timeout, and is the inotify watch limit on Ubuntu a problem? | Decision 17 | Partial: viable, but use a 10 s ceiling, 400 ms start settle, `--test-threads=1`. inotify limit not a concern; a CI step logs it. macOS latency confirmed on first CI run. |
| R05 | Where screenshots land by default on each OS, and how to read the configured folder on macOS (`com.apple.screencapture location`) and Windows | `${screenshots}` in Decision 12 | `defaults read` on macOS, `reg query` on Windows, `dirs::picture_dir()` on Linux. No new dependency. Folded into Decision 12. |
| R06 | How `thiserror` 2 and `serde` derive behave on a `&'static str` payload (`Permission::Folder`) and `SystemTime` | The contract compiles as written | One error: `Platforms` needs a hand written `Serialize`. `at` as ms via `ms_since_epoch`. `RecordingSink: Default`. `FakeSource` holds its sink. Folded into Shape. |

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
