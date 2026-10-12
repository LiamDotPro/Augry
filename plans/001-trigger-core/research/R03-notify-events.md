---
id: R03
epic: 001-trigger-core
question: What `notify` 8 reports on each OS (macOS FSEvents, Windows ReadDirectoryChangesW, Linux inotify) for a plain save, an atomic save (write temp then rename over the target, as VS Code and vim do), a create, a delete, and a rename within the watched tree?
status: answered
---

## Answer

Current stable is notify 8.2.0 (9.0.0-rc.5 exists, ignore it). macOS FSEvents is lossy: flags are sticky and merged per path, so a plain save of an existing file arrives as Create + Metadata + Data(Content), and rename halves are both `Modify(Name(Any))` with no pairing. Do not trust EventKind to tell save from create. Use the kind only to mark a path dirty, wait `settle_ms` of quiet for that path, then decide from a stat: existed before and exists now means saved (or renamed over), did not exist and exists means created, existed and gone means removed.
Paths on macOS come canonicalised (`/private/var/...` even when watching `/var/...`), so canonicalise `when.path` before computing `relative`.
macOS was run; Linux and Windows were read from notify 8.2.0 source only.

## Evidence

Run on this Mac (Darwin 25.2.0), notify 8.2.0, `recommended_watcher` + `RecursiveMode::Recursive`, scratch project in the session scratchpad `r03/`. Each step slept 1.5 s then drained the channel. Watched path was given as `/var/folders/.../tmp.X`; every event path was `/private/var/folders/.../tmp.X/...`. Observed (paths shortened):

- Create new file (`fs::write`): `Create(File)`, `Modify(Metadata(Extended))`, `Modify(Data(Content))`.
- Plain save of existing file (`fs::write` truncate): `Create(File)`, `Modify(Metadata(Any))`, `Modify(Metadata(Extended))`, `Modify(Data(Content))`. Note the spurious Create, identical in shape to a real create.
- Append plus fsync: `Modify(Data(Content))` only.
- Atomic save (write `a.txt.tmp`, rename over `a.txt`): on `a.txt.tmp`: `Create(File)`, `Modify(Name(Any))`, `Metadata(Extended)`, `Data(Content)`; on `a.txt`: `Modify(Name(Any))` twice. No data event on the target, no Create on the target, and no From/To/Both.
- Vim style (rename `a.txt` to `a.txt~`, write new `a.txt`, remove `a.txt~`): `Name(Any)` on `a.txt`, `Name(Any)` on `a.txt~`, `Create(File)`, `Metadata(Extended)`, `Data(Content)` on `a.txt`, `Remove(File)` then `Name(Any)` on `a.txt~`.
- Rename `a.txt` to `b.txt` in the same dir: one `Name(Any)` for each path, separate events, no tracker pairing visible in the kind.
- Move into a subdir: same, one `Name(Any)` per path.
- mkdir: `Create(Folder)`, `Metadata(Extended)`.
- Delete: `Remove(File)` then `Modify(Name(Any))` on the same path (a stale rename flag also appears after a remove).
- Create then two quick writes: coalesced into one `Create`, `Metadata`, `Data(Content)` group. Writes inside the FSEvents latency window are not individually visible.
- Delete then recreate immediately: `Create, Remove, Metadata, Data, Create, Metadata, Data` all on one path. The history is merged and ordered unreliably, and the final state is only knowable by stat.

Read, not run:
- Linux, `notify-8.2.0/src/inotify.rs` lines 233-356 (cargo registry copy): MOVED_FROM gives `Modify(Name(From))`, MOVED_TO gives `Modify(Name(To))`, and when cookies match an extra `Modify(Name(Both))` with paths `[from, to]` is emitted after the To. CREATE gives `Create(File|Folder)`, DELETE gives `Remove(File|Folder)`, MODIFY gives `Modify(Data(Any))`, CLOSE_WRITE gives `Access(Close(Write))`, attribute changes give `Modify(Metadata(..))`. A plain save (truncate + write) gives one or two `Modify(Data(Any))` and a CLOSE_WRITE (the `Access(Close(Write))` is the best "write finished" signal on Linux). Atomic save: `Create(File)` temp, `Data(Any)`, `Access(Close(Write))` on temp, then `Name(From)` temp, `Name(To)` target, `Name(Both)` [temp, target]. Moves in or out of the watched tree show only a lone To or a lone From. Paths are not canonicalised, they are what you passed plus the child name. inotify does not coalesce, and the queue can overflow, which notify reports as `EventKind::Other` with `Flag::Rescan` (line 213).
- Windows, `notify-8.2.0/src/windows.rs` lines 284-290 and 425-450: filter is FILE_NAME, DIR_NAME, ATTRIBUTES, SIZE, LAST_WRITE, CREATION, SECURITY. Mapping: ADDED gives `Create(Any)`, REMOVED gives `Remove(Any)`, MODIFIED gives `Modify(Any)`, RENAMED_OLD_NAME gives `Modify(Name(From))`, RENAMED_NEW_NAME gives `Modify(Name(To))`. No `Both`, no File/Folder distinction. A plain save is typically several `Modify(Any)` (size, then last-write). An atomic save over an existing target is typically Create(temp), Modify(temp)..., Rename From(temp), then Rename To(target) (replacement may also surface as Remove(target) before To, depending on the writer). Watched paths are canonicalised by notify (`windows.rs` lines 376, 392), which on Windows yields `\\?\C:\...` verbatim prefixes; strip with `dunce` or compare after canonicalising both sides. This table is from source reading and general ReadDirectoryChangesW behaviour, not a run.
- Crate version: crates.io API, 2026-10-12, newest non-yanked: 9.0.0-rc.5, then 8.2.0, 8.1.0, 8.0.0.

## Implications for the cards

- Pin `notify = "8.2"` in `[workspace.dependencies]` (resolves to 8.2.0). Do not use `default-features = false`; the default macOS backend is FSEvents.
- Canonicalise `when.path` at `start` (`std::fs::canonicalize`) before `watch()` and before computing `relative`. On macOS event paths are `/private/var/...`; on Windows they gain `\\?\`. If canonicalize fails, return `StartError`. Report `path` in the payload as the canonical path.
- `logic.rs` design (pure, driven by `(now, RawEvent{kind, paths})` plus a `Fs` trait or a stat result passed in):
  1. Ignore these kinds: `Access(*)` except optionally `Access(Close(Write))` as a hint, `Modify(Metadata(*))` alone (but still mark dirty if the path is already pending), and `EventKind::Other` (on Rescan: mark the whole root dirty, or log and skip).
  2. Any `Create`, `Remove`, `Modify(Data|Name|Any|Metadata)` on a path marks it pending: `last_event = now`, and on first sight record `existed_before` (from a per-path known-set the watcher seeds lazily: on first event, existence before is unknown, so keep a `known: HashSet<PathBuf>` filled by an initial directory scan at start and updated after each decision).
  3. When `now - last_event >= settle_ms`, stat the path once and decide: `!known && exists` is `fs.file-created`; `known && exists` is `fs.file-saved`; `known && !exists` is `fs.file-removed`; `!known && !exists` is dropped (temp file that came and went, for example vim swap). Then update `known` and clear pending. One decision per path per settle window is what prevents double `file-saved`.
  4. Rename: do not trust `Both` (Linux only). Treat a rename as pending on both halves, then resolve at settle. If a pair carries `Name(Both)` or a Linux cookie, emit `fs.file-renamed` with `from`. On macOS and Windows, pair heuristically: within one settle window, exactly one path that is `known && !exists` and exactly one that is `!known && exists` in the same batch with a Name event on both gives `file-renamed(from, to)`. Otherwise fall back to removed plus created.
  5. Atomic save over an existing target: the target is `known` and still exists at settle, so it resolves to `file-saved` regardless of the platform's event shape. The temp name is `!known && !exists` and is dropped (and `.tmp` is in the default ignores anyway). Vim's `a.txt~` backup likewise vanishes; add `~` and `.swp`/`.swx` suffixes to the ignore defaults as a suggestion to the architect.
  6. `fs.download-finished`: key off the rename or removal of a partial suffix (`.crdownload`, `.part`, `.download`, `.tmp`) to a final name: a `!known && exists` path whose sibling with a partial suffix was pending or known within the last N seconds. Because ignores drop partial names from emission, the state machine must still observe them (apply ignores at emission, not at ingestion).
- Tests for `logic.rs` should replay these exact sequences as fixtures: macOS plain save (Create, Meta, Meta, Data on a known path must yield exactly one `file-saved`), macOS atomic save, Linux atomic save with Both, Windows rename From/To, delete-then-recreate (known, exists: one `file-saved`), and create plus quick writes (one `file-created`).
- Timing: FSEvents latency in notify is about 0 s but events still arrive in bursts up to a second late in the runs above (1.5 s drains captured everything). The default `settle_ms = 500` is adequate for quiet bursts; test with fake clock, not real sleeps.

## Doubts

- Linux and Windows sequences are derived from source, not executed. The exact Windows atomic-save shape (whether `Remove(target)` precedes `Rename To`) varies by writer and is the reason for the stat-at-settle design. If a Windows or Linux CI runner is available, rerun the scratch program (`r03/src/main.rs` pattern above) there.
- macOS runs used a path under `/var/folders`; a path under `~/Downloads` or a network volume may differ in latency but should keep the same sticky-flag behaviour.
- The known-set seeding by initial scan could be costly on huge trees; the card writer may prefer lazy seeding (treat unknown path with a Create flag as created, with a Data-only event as saved), which reintroduces the macOS spurious Create on plain saves. Initial scan is recommended.
