# Repo map

Where things live and which command does what. Every tier reads this. A card
that changes the layout or a command updates this file in the same card, so
it stays true for the current commit.

## Today (before epic 001)

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
| `src/cloud/` | `api.ts` (openapi-fetch client, React Query `$api`, `withSessionToken`), `schema.d.ts` (generated), `api.cloud.test.ts` (needs an API, skipped otherwise) |
| `src/styles/tokens.css` | Design tokens lifted from Claude Design. Colours and sizes live here, never in components |
| `src/styles/*.css` | One file per area: `chrome.css`, `pane.css`, `notes.css`, `overlays.css` (settings sheet, toast), `review.css` |
| `src-tauri/Cargo.toml` | The app crate `terminal-grid` (lib `terminal_grid_lib`). Rust 2021, stable toolchain |
| `src-tauri/src/lib.rs` | Builder, plugins, `AppState` (`pty`, `git`, `agent`, `notes`, `state`), the `generate_handler![...]` command list, `blocking()` |
| `src-tauri/src/error.rs` | `AppError`: thiserror enum, serialised with `code` tag in PascalCase |
| `src-tauri/src/pty/` | PTY sessions, shell integration, the reader thread. `create_terminal`, `write_terminal` |
| `src-tauri/src/agent/` | `AgentWatcher`: 1 s process tree poll, `step` state machine (pure, tested), `terminal://agent` and `terminal://stats` events, task file |
| `src-tauri/src/git/` | git CLI wrapper, worktree parser, `GitService` cache, review commands |
| `src-tauri/src/notes/` | `NotesService`: sandboxed file tree under a notes root, `notify` watcher for external changes |
| `src-tauri/src/state/` | `config.json` and `session.json` in the app config dir (`~/Library/Application Support/com.liamread.terminalgrid` on macOS), atomic writes, `Config` and `ConfigPatch` |
| `src-tauri/src/cloud/` | `generated.rs` (progenitor client from the API contract) and `mod.rs` (`client()`, `poll_github_token`, a test that needs `AUGRY_API_URL`) |
| `src-tauri/tauri.conf.json` | Window, CSP (`connect-src` allows the API), bundle |
| `src-tauri/capabilities/default.json` | Permissions for the webview: window, dialog, opener |
| `scripts/api-generate.sh`, `tools/api-codegen/` | Regenerate both cloud clients from `libs/api-contract/openapi.yaml` (`AUGRY_OPENAPI` overrides the path) |
| `libs/api-contract/` | The released API contract, delivered by the cloud repo's "Sync API contract" pull request. Not present until the first sync is merged |
| `docs/technical-design.md` | The design of the core: modules, IPC contract (section 10), persistence (9), testing (14) |
| `docs/process-triggers.md` | The trigger catalogue (Part 1) and the one crate per integration design (Part 2) that the trigger epics implement |
| `design/` | Export of the Claude Design project. `CLAUDE.md` says how to port it |
| `.github/workflows/ci.yml` | On every push and pull request, on macOS, Windows and Ubuntu: `npm test`, `npm run build`, `cargo test` in `src-tauri` |
| `plans/`, `.claude/` | The planning pipeline. See `plans/README.md` |

### Commands

Run from the repo root unless stated.

| Check | Command |
|---|---|
| Frontend tests | `npm test` |
| Type check and build | `npm run build` |
| Rust tests | `cd src-tauri && cargo test` (after epic 001: `cargo test --workspace`) |
| Rust formatting | `cd src-tauri && cargo fmt --check` |
| Rust lints | `cd src-tauri && cargo clippy --all-targets -- -D warnings` (the app crate has a few existing warnings; new crates must be clean) |
| Run the app | `npm run tauri dev` |
| Regenerate the cloud clients | `npm run api:generate` |
| Cloud tests against a running API | `npm run test:cloud`; `cd src-tauri && AUGRY_API_URL=http://localhost:8080 cargo test cloud` |

The checker runs the first five for every card that touches `src/` or
`src-tauri/`.

### Things a card must not assume

- There is no keychain crate, no secure storage and no network code in the app
  apart from `src-tauri/src/cloud/`.
- There is no `src-tauri/crates/` folder and no Cargo workspace until epic 001.
- `src/cloud/api.ts` creates React Query's `queryClient`, but no component uses
  it yet.
- `libs/api-contract/` may not exist on the machine building a card.

## After epic 001

Epic 001's last card replaces this section with the real layout: the
workspace, `src-tauri/crates/trigger-core`, `trigger-hub`, `trigger-fs`,
`src-tauri/src/triggers/`, the new commands and the `trigger://fired` event.
