# Repo map

Where things live and which command does what. Every tier reads this. A card
that changes the layout or a command updates this file in the same card, so
it stays true for the current commit.

## Today

A Tauri 2 desktop app: a Rust core in `src-tauri/` and a React frontend in
`src/`. The IPC contract between them is `docs/technical-design.md` section
10 and `src/ipc/types.ts`.

| Path | What |
|---|---|
| `package.json` | npm scripts: `dev`, `build` (`tsc --noEmit && vite build`), `test` (vitest), `tauri`, `api:generate`, `test:cloud`, `design:sync` |
| `src/App.tsx` | The shell: `Chrome`, the terminals and notes views, overlays (`Splash`, `HotkeyPopover`, `SettingsDialog`, the error toast). The global hotkey handler |
| `src/components/` | React components by area: `Chrome.tsx`, `PaneGrid.tsx`, `TerminalPane.tsx`, `SettingsDialog.tsx`, `EmptyState.tsx`, `notes/`, `review/` |
| `src/state/reducer.ts` | The one reducer: `AppState`, `Action` (types like `ui/settings`, `notes/toggle-panel`), `toSession` |
| `src/state/AppProvider.tsx` | `useReducer` plus an `AppActions` object. Components never call IPC directly; they call actions. Bootstrap, event subscriptions, debounced session save |
| `src/ipc/types.ts` | `Commands` (name to `{in, out}`), `EVENTS` (`area://name` strings), `AppError` and its codes. Mirrors technical-design section 10 |
| `src/ipc/client.ts` | `call(name, args)` typed over `invoke`; `on(event, handler)` |
| `src/lib/` | Pure helpers with tests: `hotkeys.ts`, `shellCommand.ts`, `sessionNotes.ts`, `agentTask.ts`, `appearance.ts`, `diff.ts`, `markdown.ts` |
| `src/lib/triggers.ts` | Pure helpers for process triggers: the toast text for a fired trigger (`firedNotice`) and the one line status of a process (`processStatusLine`) |
| `src/ipc/types.ts` (triggers) | `TriggersStatus`, `TriggerSourceInfo`, `TriggerFiredEvent` and friends; the `triggers_*` entries in `Commands`; `EVENTS.triggerFired` |
| `src/cloud/` | `api.ts` (openapi-fetch client, React Query `$api`, `withSessionToken`), `schema.d.ts` (generated), `api.cloud.test.ts` (needs an API, skipped otherwise) |
| `src/styles/tokens.css` | Design tokens lifted from Claude Design. Colours and sizes live here, never in components |
| `src/styles/*.css` | One file per area: `chrome.css`, `pane.css`, `notes.css`, `overlays.css` (settings sheet, toast), `review.css` |
| `src-tauri/Cargo.toml` | The app crate `terminal-grid` (lib `terminal_grid_lib`), also the root of a Cargo workspace whose members are `crates/*`. `[workspace.dependencies]` holds the shared versions. Features `trigger-fs` and `trigger-git` (both on by default) wire in the file and git sources. Rust 2021, stable toolchain |
| `src-tauri/src/lib.rs` | Builder, plugins, `AppState` (`pty`, `git`, `agent`, `notes`, `state`), the `generate_handler![...]` command list, `blocking()` |
| `src-tauri/src/error.rs` | `AppError`: thiserror enum, serialised with `code` tag in PascalCase |
| `src-tauri/src/pty/` | PTY sessions, shell integration, the reader thread. `create_terminal`, `write_terminal`. `shell.rs` picks the shell and its launch arguments and `env` (zsh `ZDOTDIR` wrapper, bash `--rcfile`, `sh` bare) and writes the scripts and wrapper files to the app data dir |
| `src-tauri/src/pty/marks.rs`, `src-tauri/src/pty/tap.rs` | `marks.rs`: the pure `Scanner` that reads a shell's bytes for `OSC 133` marks, the bell and (on request) printable lines. `tap.rs`: `CommandClock` (command timing) and `Tap`, which turn marks into `InternalSignal`s for the reader thread to publish |
| `src-tauri/src/pty/shell/live.rs` | Unix only tests that start the real zsh and bash with the app's launch arguments in a fake home; a shell that is not installed skips |
| `src-tauri/resources/` | `shell-integration.sh` and `.ps1`, the bash wrapper `bashrc` and the zsh wrapper `zsh/.zshenv`, `.zprofile`, `.zshrc`, `.zlogin`. The wrapper files are compiled in with `include_str!` and written by `install_scripts` |
| `src-tauri/src/agent/` | `AgentWatcher`: 1 s process tree poll, `step` state machine (pure, tested), `terminal://agent` and `terminal://stats` events, task file |
| `src-tauri/src/git/` | git CLI wrapper, worktree parser, `GitService` cache, review commands |
| `src-tauri/src/notes/` | `NotesService`: sandboxed file tree under a notes root, `notify` watcher for external changes |
| `src-tauri/src/state/` | `config.json` and `session.json` in the app config dir (`~/Library/Application Support/com.liamread.terminalgrid` on macOS), atomic writes, `Config` and `ConfigPatch` |
| `src-tauri/src/triggers/` | `mod.rs` (`TriggerService` in `AppState`, `sources()`, `refresh()`, the forward thread, the four commands), `store.rs` (definition files in `<config dir>/processes/`), `variables.rs` (`${home}`, `${downloads}`, `${notes.root}`, `${screenshots}`, `${repo.root}`), `rules.rs` (the pure planner), `status.rs` (status and `trigger://fired` JSON), `feeds.rs` (`publish` and the small builders the feed points call) |
| `src-tauri/src/triggers/internal/` | The `tg` source, not a crate because only the app sees these events. `mod.rs` (`InternalSource`, `InternalSignal`), `kinds.rs` (`KINDS`, thirteen `tg.*` kinds), `rule.rs`, `logic.rs` (pure `step`, tests in `logic/tests.rs`), `README.md` with the same sections as a crate |
| `src-tauri/crates/trigger-core/` | The contract (`Source`, `Rule`, `Event`, `KindInfo`, `kinds_markdown`) and `testkit` (`RecordingSink`, `FixedClock`, `FakeSource`) |
| `src-tauri/crates/trigger-hub/` | `Hub`: catalogue, `check`, `apply`, `publish`, `subscribe`, the two second cool down, the `trigger-hub` thread |
| `src-tauri/crates/trigger-fs/` | The reference source. Kinds `fs.file-saved`, `fs.file-created`, `fs.file-removed`, `fs.file-renamed`, `fs.download-finished`, over `notify` |
| `src-tauri/crates/trigger-git/` | Fires on what happens in a repository: commit, branch switch, pull, merge conflict, stopped rebase, tag, stash, detached HEAD, behind remote, old uncommitted work, new clone in a folder. Kinds `git.commit`, `git.branch-switched`, `git.pulled`, `git.merge-conflict`, `git.rebase-stopped`, `git.tag-created`, `git.stash-created`, `git.detached-head`, `git.behind-remote`, `git.uncommitted-older-than`, `git.repo-cloned`. `logic.rs` (pure `RepoSnapshot` and `step`), `rule.rs`, `git.rs` (the git CLI and `GitRepo`), `os.rs` (`notify` watching, settle and lock retry, folder scan), `worker*.rs` (the `trigger-git` thread) |
| `src-tauri/src/cloud/` | `generated.rs` (progenitor client from the API contract) and `mod.rs` (`client()`, `poll_github_token`, a test that needs `AUGRY_API_URL`) |
| `src-tauri/tauri.conf.json` | Window, CSP (`connect-src` allows the API), bundle |
| `src-tauri/capabilities/default.json` | Permissions for the webview: window, dialog, opener |
| `scripts/api-generate.sh`, `tools/api-codegen/` | Regenerate both cloud clients from `libs/api-contract/openapi.yaml` (`AUGRY_OPENAPI` overrides the path) |
| `libs/api-contract/` | The released API contract, delivered by the cloud repo's "Sync API contract" pull request. Not present until the first sync is merged |
| `docs/technical-design.md` | The design of the core: modules, IPC contract (section 10), persistence (9), testing (14) |
| `docs/process-triggers.md` | The trigger catalogue (Part 1) and the one crate per integration design (Part 2) that the trigger epics implement |
| `design/` | Export of the Claude Design project. `CLAUDE.md` says how to port it |
| `.github/workflows/ci.yml` | On every pull request and push to main, on macOS, Windows and Ubuntu: `npm test`, `npm run build`, `cargo fmt --check`, clippy with `-D warnings` on the trigger crates, `cargo test --workspace`, `node plans/_tools/check-crates.mjs`, and the live tests of `trigger-fs` and `trigger-git` |
| `plans/`, `.claude/` | The planning pipeline. See `plans/README.md` |
| `plans/_templates/trigger-crate/` | The template every trigger crate is made from: Cargo.toml, README with the eight sections, `src/lib.rs`, `rule.rs`, `logic.rs`, `os.rs`, `examples/readme.rs`, `tests/readme.rs`, `tests/live.rs` |

### Commands

Run from the repo root unless stated.

| Check | Command |
|---|---|
| Frontend tests | `npm test` |
| Type check and build | `npm run build` |
| Rust tests | `cd src-tauri && cargo test --workspace` |
| Live file watching tests | `cd src-tauri && cargo test -p trigger-fs -- --ignored --test-threads=1` |
| Live git tests | `cd src-tauri && cargo test -p trigger-git -- --ignored --test-threads=1` (needs `git` on the PATH; each test makes its own temporary repository) |
| Rust formatting | `cd src-tauri && cargo fmt --check` |
| Rust lints | `cd src-tauri && cargo clippy --all-targets -- -D warnings` (the app crate has a few existing warnings; new crates must be clean; CI lints the trigger crates by name with `-p`) |
| Run the app | `npm run tauri dev` |
| Regenerate the cloud clients | `npm run api:generate` |
| Cloud tests against a running API | `npm run test:cloud`; `cd src-tauri && AUGRY_API_URL=http://localhost:8080 cargo test cloud` |
| New trigger crate | `node plans/_tools/new-trigger-crate.mjs <name> "<summary>"` |
| Trigger crate rules (isolation, README, size) | `node plans/_tools/check-crates.mjs` |

The checker runs the first five for every card that touches `src/` or
`src-tauri/`.

### Things a card must not assume

- There is no keychain crate, no secure storage and no network code in the app
  apart from `src-tauri/src/cloud/`.
- `src/cloud/api.ts` creates React Query's `queryClient`, but no component uses
  it yet.
- `libs/api-contract/` may not exist on the machine building a card.

## Process triggers (epic 001)

How a trigger gets from a file on disk to a toast:

1. A definition file `<app config dir>/processes/<id>.json` has a `triggers` list. Settings, under "Process triggers (experimental)", switches triggers on (`Config.triggers.enabled`) and arms processes (`Config.triggers.armed`, set by `triggers_arm`).
2. `TriggerService::refresh()` reads the files, expands variables, and gives the rules to `Hub::apply`. Rule ids are `<processId>/<triggerId>`, with `#<n>` added when `${repo.root}` fans out. It runs on start, after config saves, and after a terminal is created, closed or changes directory.
3. `trigger-hub` starts the source that owns each rule (`fs.*` goes to `trigger-fs`), applies the two second cool down, and publishes `Fired`.
4. The `trigger-forward` thread logs one line (ids and kind only) and emits `trigger://fired`. The reducer shows a neutral toast for six seconds. Running a process on a trigger is a later epic.

IPC added by the epic: commands `triggers_catalogue`, `triggers_check`, `triggers_status`, `triggers_arm`, and the event `trigger://fired`. `triggers_status` returns, per rule, the state `active`, `rejected`, `sourceFailed` or `skipped`.

To add a source: `node plans/_tools/new-trigger-crate.mjs <name> "<summary>"`, fill in the crate, then add its optional dependency and feature to `src-tauri/Cargo.toml` and one line to `sources()` in `src-tauri/src/triggers/mod.rs`. `node plans/_tools/check-crates.mjs` must exit 0.

## Internal triggers (epic 002)

The `tg` source is registered first in `sources()` and is always on. `TriggerService::publish_internal(signal)` is the one entry point; every feed builds an `InternalSignal` and calls `triggers::feeds::publish` after the work it reports has succeeded. Design: `docs/technical-design.md` 9.2. The kinds and their rule fields are in `src-tauri/src/triggers/internal/README.md`.

Where each signal comes from:

| Signal | Feed point |
|---|---|
| `CommandEnded`, `Bell`, `OutputLine` | the reader thread in `pty/mod.rs`, through `pty::tap::Tap` and `pty::marks::Scanner` |
| `AgentFinished` | `agent/mod.rs` |
| `TaskReported` | `agent/task.rs` |
| `PaneOpened` | `create_terminal` in `pty/mod.rs` |
| `FocusEntered` | `set_focused_terminal` in `pty/mod.rs` |
| `WorktreeCreated`, `WorktreeRemoved` | `git/worktree.rs` |
| `ReviewStaged` | `git/review.rs` |
| `NoteSaved` | `notes/mod.rs` |
| `AppStarted` | `triggers::start` in `triggers/mod.rs` |

To add a kind: add it to `KINDS` in `kinds.rs`, parse its `when` in `rule.rs`, handle its signal in `logic.rs` with a test, call `feeds::publish` from the feed point, and refresh the Kinds table in `internal/README.md` (a test fails while it is stale).
