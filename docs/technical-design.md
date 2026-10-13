# Terminal Grid: technical design

Working name: **Terminal Grid** (rename freely). Desktop app built on Tauri 2 with a Rust core. The frontend visual layer is out of scope here; it will come from Claude Design. This document defines everything the frontend needs to plug into: the process model, the Rust subsystems, the IPC contract, the state model, and the Windows specifics.

Versions checked on crates.io and npm on 7 September 2026:

| Layer | Package | Version |
|---|---|---|
| Rust | tauri | 2.11.5 |
| Rust | tauri-build | 2.6.3 |
| Rust | tauri-plugin-dialog | 2.7.3 |
| Rust | portable-pty | 0.9.0 |
| Rust | sysinfo | 0.39.6 |
| Rust | trash | 5.2.7 |
| Rust | notify | 8.2.0 |
| Rust | base64 | 0.23.1 |
| Rust | serde / serde_json | 1.0.229 / 1.0.151 |
| Rust | thiserror | 2.0.20 |
| JS | @tauri-apps/api | 2.11.1 |
| JS | @tauri-apps/cli | 2.11.4 |
| JS | @tauri-apps/plugin-dialog | 2.7.3 |
| JS | @xterm/xterm | 6.0.0 |
| JS | @xterm/addon-fit / addon-webgl / addon-serialize | 0.11.0 / 0.19.0 / 0.14.0 |

---

## 1. Requirements mapped to design decisions

| # | Requirement | Where it lives |
|---|---|---|
| 1 | One to many terminals in a single window | Rust `pty` module owns N sessions; frontend renders one xterm.js instance per session |
| 2 | Responsive grid up to 9, then pages | Pure layout function in the frontend (section 6), no backend involvement |
| 3 | Outline on focused terminal | Frontend focus state; backend only needs to know the focused id for git refresh priority |
| 4 | Git labels per terminal | Rust `git` module, refreshed on cwd change and command completion via shell integration |
| 5 | Worktrees visible for the active terminal | `git worktree list --porcelain` parsed in Rust, cross referenced with the cwd of every open session |
| 6 | Full screen | `tauri.conf.json` window `fullscreen: true`, F11 toggle through the window API |
| 7 | Ctrl+Alt+Arrow moves terminals | Frontend keyboard handler in capture phase, xterm custom key handler to stop xterm swallowing it |
| 8 | Subtle animation when an LLM agent finishes | Rust `agent` watcher (process tree) plus shell integration and BEL as fallbacks, emits `terminal://agent` events |
| 9 | Terminals and Notes tabs | Frontend routing; both trees stay mounted so PTYs keep streaming |
| 10 | Notes: tree, editor, viewer, create/open/delete | Rust `notes` module with sandboxed file operations under a notes root |

---

## 2. Stack and repository layout

**Core:** Rust, Tauri 2, portable-pty (ConPTY on Windows), git CLI invoked as a subprocess (no libgit2, keeps the Windows build simple and respects the user's git config, credential helpers and worktrees exactly as the CLI sees them).

**Frontend runtime:** the WebView (WebView2 on Windows). Whatever Claude Design produces needs two non visual libraries: `@xterm/xterm` for the terminal surface and `@tauri-apps/api` for IPC. Everything else (framework, markdown editor, markdown renderer) is the frontend's choice.

```
terminal-grid/
  package.json
  src/                      # frontend, delivered separately
  src-tauri/
    Cargo.toml
    build.rs
    tauri.conf.json
    capabilities/default.json
    icons/
    src/
      main.rs               # calls terminal_grid_lib::run()
      lib.rs                # builder, plugin registration, command handler list, managed state
      error.rs              # AppError (thiserror) with serde Serialize so commands can return it
      pty/
        mod.rs              # PtyManager, Session, spawn/write/resize/close
        shell.rs            # shell discovery and integration script generation
        reader.rs           # reader thread, chunk coalescing, event emission
        marks.rs            # Scanner: OSC 133 marks, bell and printable lines from a shell's bytes (pure, tested)
        tap.rs              # CommandClock and Tap: marks to InternalSignals for the tg source
        shell/live.rs       # pty tests that start the real zsh and bash (unix only)
      git/
        mod.rs              # GitInfo model, GitService (cache + refresh)
        cli.rs              # subprocess wrapper with CREATE_NO_WINDOW
        worktree.rs         # porcelain parser
      agent/
        mod.rs              # AgentWatcher: process tree polling, state transitions
      notes/
        mod.rs              # NotesService: tree, read, write, create, delete, rename
        paths.rs            # root sandboxing and validation
      state/
        mod.rs              # Config + Session persistence (app config dir)
      triggers/
        mod.rs              # TriggerService: the hub, refresh(), the forward thread, the four triggers_* commands
        store.rs            # local process definitions, one JSON file each in <app config dir>/processes/
        variables.rs        # ${home}, ${downloads}, ${notes.root}, ${screenshots}, ${repo.root}
        rules.rs            # definitions + armed list + open repositories -> the rules the hub runs
        status.rs           # the triggers_status answer and the trigger://fired payload
        feeds.rs            # publish() and the small builders the feed points call (9.2)
        internal/           # the tg source: mod.rs, kinds.rs, rule.rs, logic.rs, README.md (9.2)
    crates/                 # trigger crates, one per integration, all built from plans/_templates/trigger-crate
      trigger-core/         # the contract (Source, Rule, Event, KindInfo) and the test kit
      trigger-hub/          # registry, rule checks, cool down, one stream of Fired events
      trigger-fs/           # the reference source: fs.file-saved, fs.file-created, fs.file-removed, fs.file-renamed, fs.download-finished
    resources/
      shell-integration.ps1 # bundled, copied to app data dir on first run
      shell-integration.sh
      bashrc                # wrapper for bash --rcfile, installed beside the scripts
      zsh/                  # .zshenv .zprofile .zshrc .zlogin, the ZDOTDIR wrapper (4.5)
```

Cargo.toml dependencies:

```toml
[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-dialog = "2"
portable-pty = "0.9"
sysinfo = "0.39"
trash = "5"
notify = "8"
base64 = "0.23"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
uuid = { version = "1", features = ["v4"] }
dirs = "7"

[build-dependencies]
tauri-build = { version = "2", features = [] }
```

Windows only dependency for process creation flags is already in std (`std::os::windows::process::CommandExt`).

`src-tauri` is a Cargo workspace. The app crate `terminal-grid` is the root package; every folder under `crates/` is a member. Shared dependency versions live in `[workspace.dependencies]` and the crates use them with `{ workspace = true }`:

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

A trigger crate depends on `trigger-core` and on outside crates only, never on another trigger crate, on `tauri` or on the app. `node plans/_tools/check-crates.mjs` checks this. The app wires each source in through one line in `sources()` in `src-tauri/src/triggers/mod.rs`, behind its own cargo feature. The rules for crates are in `docs/process-triggers.md` Part 2.

---

## 3. Process model

```
┌──────────────────────────────── Tauri process ─────────────────────────────────┐
│                                                                                 │
│  Rust core (main thread + worker threads)                                       │
│  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐    │
│  │ PtyManager    │  │ GitService    │  │ AgentWatcher  │  │ NotesService  │    │
│  │ Mutex<HashMap>│  │ cache + TTL   │  │ 1s poll thread│  │ sandboxed fs  │    │
│  └──────┬────────┘  └──────┬────────┘  └──────┬────────┘  └──────┬────────┘    │
│         │ reader thread    │                   │                  │             │
│         │ per session      │                   │                  │             │
│         ▼                  ▼                   ▼                  ▼             │
│     emit events        emit events         emit events       command results   │
│  ══════════════════════════ Tauri IPC (commands + events) ═════════════════════ │
│                                                                                 │
│  WebView2 (frontend)                                                            │
│  xterm instances ⇄ terminal://output   grid state   notes state   hotkeys       │
└─────────────────────────────────────────────────────────────────────────────────┘
        │ ConPTY                       │ git.exe (hidden)        │ fs
        ▼                              ▼                         ▼
   pwsh.exe / cmd.exe / bash        repo                      notes root
```

Managed state registered in `lib.rs` via `app.manage(...)`:

```rust
pub struct AppState {
    pub pty: PtyManager,          // Arc<Mutex<HashMap<TerminalId, Session>>>
    pub git: GitService,          // Arc<Mutex<HashMap<PathBuf, CachedGitInfo>>>
    pub agent: AgentWatcher,      // handle to the polling thread + shared status map
    pub notes: NotesService,      // Arc<RwLock<NotesConfig>>
    pub config: Arc<RwLock<Config>>,
    pub triggers: TriggerService, // the trigger hub and the rules worked out from process definitions
}
```

Threads:

- One reader thread per PTY session (blocking read on the master).
- One waiter thread per session (`child.wait()`), emits exit.
- One AgentWatcher thread (1 s tick).
- One optional `notify` watcher thread for the notes root.
- One `trigger-hub` thread: drains events from the trigger sources, applies the two second cool down per rule, and publishes `Fired`.
- One `trigger-forward` thread: turns each `Fired` into the `trigger://fired` event and one log line (ids and the kind only, never the payload).
- One thread per running trigger source (for `trigger-fs`, named `trigger-fs` and `trigger-fs-watch`, which owns the `notify` watcher). `TriggerService::refresh()` starts and stops them whenever the rules change; it is cheap when nothing changed, so it runs after every pane change and config save.
- Git commands run on Tauri's async command pool (`#[tauri::command] async fn`) so they never block the main thread.

---

## 4. Terminal subsystem (`pty`)

### 4.1 Session model

```rust
pub type TerminalId = String; // uuid v4

pub struct Session {
    pub id: TerminalId,
    pub master: Box<dyn portable_pty::MasterPty + Send>,
    pub writer: Box<dyn std::io::Write + Send>,
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
    pub shell_pid: Option<u32>,
    pub cwd: PathBuf,                 // last known, updated by OSC 7
    pub spawned_at: std::time::Instant,
    pub shell: ShellKind,             // Pwsh, WindowsPowerShell, Cmd, Bash, Zsh, Other
}
```

### 4.2 Spawn

```rust
let pty_system = native_pty_system();
let pair = pty_system.openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })?;
let mut cmd = CommandBuilder::new(shell.program());
cmd.args(shell.args(&integration_script_path));
cmd.cwd(&cwd);
cmd.env("TERM", "xterm-256color");
cmd.env("COLORTERM", "truecolor");
cmd.env("TERMINAL_GRID_ID", &id);
cmd.env("TERMINAL_GRID", "1");
let child = pair.slave.spawn_command(cmd)?;
drop(pair.slave);
let reader = pair.master.try_clone_reader()?;
let writer = pair.master.take_writer()?;
```

Shell discovery order on Windows: `config.shell` if set, else `pwsh.exe` on PATH, else `powershell.exe`, else `%COMSPEC%`. On other platforms: `$SHELL`, else `/bin/bash`.

`pair.slave` must be dropped after spawn or the reader never sees EOF on exit.

### 4.3 Reader thread and output events

The reader loops on `read()` into an 8 KiB buffer and coalesces chunks: flush to the frontend when either 16 KiB has accumulated or 8 ms have passed since the first unflushed byte. Payload is base64 so multi byte UTF-8 split across chunks is never corrupted by JSON string conversion; the frontend decodes to `Uint8Array` and calls `term.write(bytes)`.

```rust
app.emit_to("main", "terminal://output", OutputEvent { id, data_b64 })
```

Backpressure: if a single session emits more than 4 MiB in one second (a runaway `cat`), the reader inserts a 20 ms sleep per chunk. This keeps the UI responsive without dropping bytes.

Marks and the tap: after a chunk is queued for the window, the reader thread also gives the same bytes to the session's `tap::Tap` (`pty/tap.rs`), so the screen is never delayed. A `Tap` owns a `marks::Scanner` (`pty/marks.rs`), a pure byte state machine that carries its state across chunks, so a read that ends inside an escape sequence gives the same result as one that does not. The scanner reports `Mark::PromptStart` (`OSC 133;A`), `Mark::CommandEnd { exit }` (`OSC 133;D;<code>`, no code means 0), `Mark::Bell` (a BEL that does not end an OSC string) and, only while `TriggerService::wants_output()` is true, the complete printable lines with CSI, OSC and two byte escapes removed. Nothing is scanned for lines in a grid without an armed `tg.output-matched` rule. The tap turns marks into `InternalSignal`s (`CommandEnded` with a duration, `Bell`, `OutputLine` cut to 200 characters) and the reader thread hands each to `TriggerService::publish_internal` (9.2). The call runs inside `catch_unwind`: if the tap ever panics, that pane stops being tapped and its output keeps flowing. The webview keeps its own OSC parsing; nothing in the frontend changed.

Command timing is measured in Rust by a `CommandClock` shared by the reader thread and `PtyManager::write`. A command starts at the first `write` containing `\r` after a prompt mark (or after the last command end) and ends at the next `CommandEnd`. `write` locks the clock while it holds the sessions lock, so the reader thread locks the clock for one statement at a time and never while it asks for the sessions lock (it needs the working directory of the pane for the signal).

Exit: the waiter thread emits `terminal://exit { id, code }`. The session stays in the map with `child` exited so the frontend can show the exit and offer restart; `close_terminal` removes it.

### 4.4 Commands

| Command | Input | Output |
|---|---|---|
| `create_terminal` | `{ cwd?: string, cols: u16, rows: u16 }` | `{ id, cwd, shell }` |
| `write_terminal` | `{ id, data: string }` (UTF-8 text, xterm `onData`) | `()` |
| `write_terminal_bytes` | `{ id, data_b64 }` (for binary paste) | `()` |
| `resize_terminal` | `{ id, cols, rows }` | `()` |
| `close_terminal` | `{ id }` | `()` kills child, drops session |
| `restart_terminal` | `{ id }` | `{ id, cwd, shell }` respawn in last cwd, same id |
| `list_terminals` | | `TerminalInfo[]` |
| `set_terminal_cwd` | `{ id, cwd }` | `()` manual override when integration is absent |

### 4.5 Shell integration

The app writes `shell-integration.ps1` and `shell-integration.sh` into `app_data_dir()` on startup, together with the wrapper files `bashrc` and `zsh/.zshenv`, `zsh/.zprofile`, `zsh/.zshrc`, `zsh/.zlogin`, and launches the shell with them. The scripts are copied from the bundled `resources/` folder. The wrapper files are compiled into the binary with `include_str!` and written by the same `install_scripts` function, because Tauri's `resources/*` glob skips folders. Each file is removed before it is rewritten. The `Shell` struct that `resolve` returns has `program`, `kind`, `args` and `env`; `env` holds the variables the integration needs, and `PtyManager::spawn` sets them on the child. PowerShell launch:

```
pwsh.exe -NoLogo -NoExit -Command ". '<app_data>\shell-integration.ps1'"
```

The script dot sources the user's `$PROFILE` first if it exists, then wraps `prompt`:

```powershell
$global:__tg_original_prompt = $function:prompt
$global:__tg_last_history_id = -1
$global:__tg_last_error = $null

function global:prompt {
    # Read $? and $LASTEXITCODE first: any other statement changes them.
    $ok = $?
    $native = $global:LASTEXITCODE
    # A failed native command sets $LASTEXITCODE and adds no error record. A
    # failed cmdlet adds one, and then a stale $LASTEXITCODE must not be reported.
    $lastError = if ($global:Error.Count -gt 0) { $global:Error[0] } else { $null }
    $newError = -not [object]::ReferenceEquals($lastError, $global:__tg_last_error)
    $global:__tg_last_error = $lastError
    $exit = if ($ok) { 0 } elseif (-not $newError -and $native -is [int] -and $native -ne 0) { $native } else { 1 }
    $esc = [char]27; $bel = [char]7
    # Command finished (OSC 133;D) with exit code and last command text (custom OSC 7777)
    $h = Get-History -Count 1
    if ($h -and $h.Id -ne $global:__tg_last_history_id) {
        $global:__tg_last_history_id = $h.Id
        $b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($h.CommandLine))
        Write-Host -NoNewline "$esc]7777;cmd;$b64$bel"
        Write-Host -NoNewline "$esc]133;D;$exit$bel"
    }
    # Current directory (OSC 7)
    $p = (Get-Location).ProviderPath -replace '\\','/'
    Write-Host -NoNewline "$esc]7;file://localhost/$p$bel"
    # Prompt start (OSC 133;A)
    Write-Host -NoNewline "$esc]133;A$bel"
    $out = & $global:__tg_original_prompt
    Write-Host -NoNewline "$esc]133;B$bel"
    return $out
}
```

The exit code in `OSC 133;D` is the native command's `$LASTEXITCODE` when the last command was a native program that failed, and 1 for any other failure, so a trigger rule with `exit_code = 2` can match on PowerShell. This was never run on macOS or Linux; the Rust test `powershell_script_reports_the_native_exit_code` only reads the script.

The frontend registers OSC handlers on xterm's parser:

- `7` → `set_terminal_cwd` is not called; instead the frontend calls `report_cwd { id, cwd }` which updates the session and triggers a git refresh.
- `133` → `A/B/D` drive the fallback agent state machine (section 5).
- `7777` → last command text, used to decide whether the finished command was an agent.

For bash and zsh the `.sh` file does the same with `PROMPT_COMMAND` / `precmd_functions`. How each shell loads it:

| Shell | Launch | How the script is loaded |
|---|---|---|
| zsh | `zsh -i`, with `ZDOTDIR=<app_data>/zsh`, `TG_USER_ZDOTDIR` (the user's own `ZDOTDIR`, or `$HOME`) and `TG_SHELL_INTEGRATION` (path of the script) set in the child's environment | The VS Code approach. The four wrapper files `.zshenv`, `.zprofile`, `.zshrc` and `.zlogin` each source the user's own file of the same name with `ZDOTDIR` pointing at the user's folder, then put `ZDOTDIR` back. `.zshrc` then sources the script. A user `.zshenv` that moves `ZDOTDIR` is followed. The last wrapper step hands `ZDOTDIR` back to the user. zsh is not started as a login shell |
| bash | `bash --rcfile <app_data>/bashrc -i` | The wrapper `bashrc` sources `~/.bashrc` when it exists, then the script. `-i` is needed because `--rcfile` is only read by interactive shells |
| sh | no arguments | No integration. `sh` is classed as bash for the frontend, but it may be dash, which has no `--rcfile` |
| PowerShell | as above | The script dot sources the profile, then wraps `prompt` |
| cmd.exe | none | No integration |

Until this change bash and zsh integration never ran. The old launch was `-c '. script; exec <shell> -i'`, and `exec` replaced the shell that had sourced the script with a fresh one, so the functions and hooks the script defined were gone. Now the script is loaded by the shell that stays, and a pty test per shell (`pty/shell/live.rs`) starts the real zsh or bash with the real arguments and variables in a fake home, and asserts that `OSC 133;D` appears and that a variable set in the user's own rc file is visible. A shell that is not installed makes its test skip.

The script reports a command once per command, by history number and not by command text, so running `make` twice reports twice. zsh compares `$HISTCMD`. bash cannot: `$HISTCMD` reads `1` inside `PROMPT_COMMAND` on the bash 3.2 that macOS ships, so bash compares the number that `history 1` prints. The first prompt only records the number, because the history file was loaded and nothing ran. After each reported command the script prints `OSC 7777` (the command text in base64), `OSC 133;D;<code>`, then at every prompt `OSC 7` and `OSC 133;A`. Commands the shell does not record in its history are not reported; see 9.2.

cmd.exe gets no integration; the process tree watcher still works there, cwd stays whatever was set at creation unless the user changes it from the pane header.

---

## 5. Agent finish detection (`agent`)

Three signals, ordered by reliability. The frontend animates on the first `finished` it receives and ignores duplicates for 5 s.

### 5.1 Primary: process tree watcher (Rust)

`AgentWatcher` ticks every 1000 ms. One `sysinfo::System::refresh_processes` per tick, then for each session:

1. Collect descendants of `shell_pid` (walk `parent()` links, depth up to 8).
2. A descendant is an agent if its exe name or command line matches one of `config.agent_patterns` (default: `claude`, `codex`, `gemini`, `aider`, `gpt`, `copilot`). Claude Code on Windows shows up as `node.exe` with `claude` in the command line, so matching on the full command line is required, not just the exe name.
3. State per session: `Idle` or `Running { name, since }`.
4. Transition `Running → Idle` emits `terminal://agent { id, event: "finished", name, duration_ms }`. Transition `Idle → Running` emits `{ event: "started", name }`.

A minimum running time of 1500 ms filters out `claude --version` style invocations. The watcher only runs while at least one session exists.

### 5.2 Secondary: shell integration

On `OSC 133;D` the frontend checks the last command (from `OSC 7777`) against the same patterns. If it matches and the process watcher has not already reported `finished` in the last 5 s, treat it as finished. Covers the case where the agent exits so quickly between ticks that the watcher misses it. The Rust side also reads `OSC 133;D` (4.3), but only to publish `tg.command-failed` and `tg.command-slow` (9.2); agent detection does not use it.

### 5.3 Tertiary: BEL

`term.onBell` while the pane is not focused → finished. Claude Code emits a bell when `preferredNotifChannel` is set to `terminal_bell`; document this in the README as an optional one liner. Codex and others may also ring. The Rust scanner also sees every bell and publishes `tg.bell` (9.2) for any pane, focused or not.

### 5.4 Idle fallback for shells without integration

Only when the session shell is `Cmd` and the process watcher found no agent process: if the user's typed line matched a pattern, then output stayed above 0 bytes/s for at least 3 s and is now silent for `config.idle_timeout_ms` (default 4000) → finished. This is a heuristic and is labelled as such in settings.

### 5.5 Task reports

What the agent is working on comes from the agent itself, not from detection. `PtyManager::spawn` gives every shell `TERMINAL_GRID_TASK_FILE=<config dir>/tasks/<id>.txt` and creates the file empty. Agents write one line there. The AgentWatcher tick stats each live session's file, and when the mtime or length changes it reads the first 4 KiB and emits `terminal://task { id, task, updatedAtMs }`. The line is decoded as UTF-8 or as UTF-16 with a BOM, since Windows PowerShell 5.1's `>` writes UTF-16LE. Whitespace collapses to single spaces, the line is capped at 280 characters, and an empty file sends `task: null`. Closing a pane deletes its file, and the tasks folder is emptied at launch.

A file was chosen over an OSC sequence because agents capture their tools' output, so an escape sequence printed by a tool call never reaches the terminal. A file also needs no port or token. The frontend drops the task when a new agent run starts or the shell exits. A reported task shows as one ellipsized line under the pane header's first row (it takes the place of the last command line); clicking it drops down a popover with the full text. Before the first report the header has a small `task` button that opens the same popover. From there, "Ask agent" types a one line request into the pty (the Enter is sent 150 ms after the text, so agent TUIs don't treat it as part of a paste), and "Copy instructions" copies a CLAUDE.md / AGENTS.md section.

The animation itself and the visual "done" marker are frontend concerns; the contract is only the event.

---

## 6. Grid, paging and movement (frontend logic, backend agnostic)

Included because it is deterministic logic the design should not have to invent.

```ts
type LayoutSpec = { cols: number; rows: number };
export function layoutFor(count: number): LayoutSpec {
  if (count <= 1) return { cols: 1, rows: 1 };
  if (count === 2) return { cols: 2, rows: 1 };
  if (count <= 4) return { cols: 2, rows: 2 };
  if (count <= 6) return { cols: 3, rows: 2 };
  return { cols: 3, rows: 3 };
}
export const PAGE_SIZE = 9;
export const pageOf = (index: number) => Math.floor(index / PAGE_SIZE);
```

State: `order: TerminalId[]`, `focusedId`, `page`. Layout is computed from the number of terminals on the current page, so page 2 with two terminals renders 2x1, not 3x3 with gaps.

Move with Ctrl+Alt+Arrow: compute the focused terminal's `(row, col)` inside its page, target `(row±1, col)` or `(row, col±1)`; if the target exists on the page, swap the two ids in `order`. Ctrl+Alt+Right on the last cell of a full page moves the terminal to the first slot of the next page (and creates the page if needed); Ctrl+Alt+Left on the first cell of page n>0 moves it to the last slot of the previous page. `order` is persisted.

Other hotkeys (all Ctrl+Alt, chosen to not collide with common terminal shortcuts):

| Keys | Action |
|---|---|
| Ctrl+Alt+Arrow | Move focused terminal |
| Ctrl+Alt+Shift+Arrow | Move focus only |
| Ctrl+Alt+N | New terminal (folder picker via `tauri-plugin-dialog`) |
| Ctrl+Alt+Shift+N | New terminal in the same cwd as the focused one |
| Ctrl+Alt+W | Close focused terminal (confirm if agent running) |
| Ctrl+Alt+PageUp / PageDown | Previous / next page |
| Ctrl+Alt+1..9 | Focus terminal n on the current page |
| Ctrl+Alt+T / Ctrl+Alt+M | Switch to Terminals / Notes tab |
| F11 | Toggle fullscreen |

Implementation notes:

- Register one `keydown` listener on `window` in the capture phase. Handle and `preventDefault` when the combo matches.
- Every xterm instance gets `term.attachCustomKeyEventHandler(e => !(e.ctrlKey && e.altKey))` so xterm never sees Ctrl+Alt combos. Arrow keys produce no character under AltGr layouts, so the Dutch and US international layouts on Windows are fine. Ctrl+Alt+N and the digits do collide with AltGr on some layouts; the settings screen should allow rebinding the modifier to Ctrl+Shift.
- Hidden pages keep their xterm instances mounted but out of view (`visibility: hidden; position: absolute`), never `display: none`, so `FitAddon.fit()` keeps working and scrollback survives. On page switch, call `fit()` and `resize_terminal` for every pane that became visible.

Fullscreen: `"fullscreen": true` in the window config. F11 calls `getCurrentWindow().setFullscreen(!await isFullscreen())`. Alt+F4 still closes; the app intercepts `onCloseRequested` and asks for confirmation when any agent is running.

---

## 7. Git subsystem (`git`)

### 7.1 Model

```rust
#[derive(Serialize, Clone)]
pub struct GitInfo {
    pub in_repo: bool,
    pub repo_root: Option<PathBuf>,       // rev-parse --show-toplevel
    pub common_dir: Option<PathBuf>,      // rev-parse --git-common-dir (main repo, for worktrees)
    pub repo_name: Option<String>,        // last segment of the main repo path
    pub branch: Option<String>,           // abbrev-ref HEAD, "HEAD" when detached
    pub head_short: Option<String>,       // rev-parse --short HEAD
    pub user_name: Option<String>,        // git config user.name (effective, includes includeIf)
    pub user_email: Option<String>,
    pub remote_url: Option<String>,       // remote get-url origin
    pub dirty: bool,                      // status --porcelain non empty
    pub ahead: u32, pub behind: u32,      // rev-list --left-right --count @{u}...HEAD
    pub is_worktree: bool,                // repo_root != common_dir parent
    pub worktrees: Vec<Worktree>,
}

#[derive(Serialize, Clone)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: Option<String>,
    pub branch: Option<String>,           // refs/heads/x → x
    pub is_main: bool,
    pub is_current: bool,                 // this terminal's cwd is inside it
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
    pub open_in: Vec<TerminalId>,         // other sessions whose cwd is inside this worktree
}
```

`open_in` is what satisfies requirement 5. The pane header puts a small fork glyph in front of the branch when `is_worktree` is set. In any repo the branch opens a menu listing the worktrees, marking its own and the ones open in other panes (with the pane number). Its foot has "New worktree…": a branch name field that runs `worktree_create` (`git worktree add` into `../<repo>-<branch>`, checking out an existing branch or creating one from the pane's HEAD) and opens a pane there. Linked worktrees no pane is in get a remove button (`worktree_remove`, after a confirm; the branch stays). Picking one focuses the pane that already has it, otherwise types a shell specific `cd` at the prompt, or opens a new pane there when an agent is running in this pane (Shift+click always opens a new pane).

### 7.2 Execution

`git/cli.rs` runs `git` with:

- `current_dir(cwd)`
- `env("GIT_OPTIONAL_LOCKS", "0")` so status never blocks on index lock
- `env("GIT_TERMINAL_PROMPT", "0")`
- Windows: `creation_flags(0x08000000)` (CREATE_NO_WINDOW) so no console flashes
- a 3 s timeout (spawn, then poll `try_wait` with a deadline; kill on expiry)

All fields are gathered by a small number of calls: `rev-parse --show-toplevel --git-common-dir --abbrev-ref HEAD --short HEAD` in one invocation, `config --get user.name`, `config --get user.email`, `remote get-url origin`, `status --porcelain --untracked-files=no` (skipped when the repo has more than 50k tracked files, detected via `git ls-files | wc -l` once per repo root and cached), `rev-list --left-right --count @{upstream}...HEAD`, `worktree list --porcelain`.

### 7.3 Porcelain parser

`git worktree list --porcelain` emits blocks separated by blank lines:

```
worktree C:/dev/app
HEAD 3f2a...
branch refs/heads/main

worktree C:/dev/app-feature
HEAD 91bc...
branch refs/heads/feature/x
locked

worktree C:/dev/app-old
HEAD 77ab...
detached
prunable gitdir file points to non-existent location
```

The first block is always the main worktree. Parser is a pure function with unit tests over these fixtures, including paths with spaces and Windows drive letters.

### 7.4 Refresh policy

Cache keyed by canonical cwd, TTL 5 s. Refresh triggers:

1. `report_cwd` (OSC 7) with a changed path → immediate refresh.
2. `OSC 133;D` (a command finished) → refresh, debounced 500 ms per session.
3. Focus change → refresh the newly focused session if the cache entry is older than 2 s.
4. Background timer every 15 s for every visible session (branch may change from another window).
5. `notify` watcher on `<common_dir>/HEAD`, `<common_dir>/worktrees/`, and `<repo_root>/.git/HEAD` for worktrees → refresh all sessions in that repo. This is what makes `git worktree add` in one pane show up in another within a second.

Result is emitted as `terminal://git { id, info }` rather than returned, so a single refresh can fan out to every session in the same repo. `get_git_info { id }` exists for the initial paint.

Labels: the backend supplies fields only; the design decides how to compose them. Suggested primary label is `repo_name` and `branch`, secondary is `user_name` and the dirty/ahead/behind markers.

### 7.5 Focus + review (`git/review.rs`)

`<mod>+Enter` puts the focused pane next to a review panel for its repository (design turns 2 to 4). The core only reads and writes git; diffs, hunks, comments and the message sent to the agent are frontend logic (`src/lib/diff.ts`, `src/lib/review.ts`).

| Command | Does |
|---|---|
| `review_status { id }` | `git status --porcelain=v1 -z` split into `staged` and `unstaged` `ChangedFile`s with `--numstat` line counts, plus `repoRoot`, `branch`, `head`. Untracked files are listed as unstaged with status `?` |
| `review_file { id, path, oldPath, staged }` | The two versions a diff is drawn between (HEAD → index when staged, index → working tree when not) and the working tree copy for the editor, with its mtime. Binary files and files over 2 MiB come back flagged, without text |
| `review_files { id }` | Every file the edit tree lists: `git ls-files --cached --others --exclude-standard`, sorted |
| `review_stage { id, paths }` / `review_unstage { id, paths }` | `git add -A` / `git restore --staged` (`git rm --cached` before the first commit), with literal pathspecs |
| `review_write_file { id, path, content, expectedMtimeMs? }` | Atomic write of the editor's buffer; a newer mtime on disk is a `Conflict`, as for notes |

Paths are relative to the repository root and refused when they leave it or point into `.git`. These calls get a 15 s timeout instead of the 3 s used for labels. Staging and saving refresh the pane's git labels.

The panel polls `review_status` every 2.5 s while it is open and on every `terminal://git` for its pane. A new HEAD that took staged files with it is shown as a commit, and the comments on those files are dropped. Comments live in memory, keyed by repository, and are sent to the pane as one bracketed paste followed by Enter, or as one line when the program in the pane has not enabled bracketed paste.

---

## 8. Notes subsystem (`notes`)

### 8.1 Storage

Plain `.md` files in a notes root, default `%USERPROFILE%\Documents\TerminalGrid\Notes`, changeable through a folder picker. Subfolders allowed. Nothing else is stored; the folder is safe to sync or put in a repo.

### 8.2 Commands

| Command | Input | Output |
|---|---|---|
| `notes_get_root` | | `{ root }` |
| `notes_set_root` | `{ root }` | `{ root }` creates it if missing |
| `notes_tree` | | `NoteNode[]` recursive, folders first, sorted case insensitively, `.md` and folders only, hidden entries skipped |
| `notes_read` | `{ rel_path }` | `{ content, mtime_ms }` |
| `notes_write` | `{ rel_path, content, expected_mtime_ms? }` | `{ mtime_ms }` |
| `notes_create` | `{ rel_path, kind: "file" \| "folder" }` | `NoteNode` |
| `notes_rename` | `{ rel_path, new_rel_path }` | `NoteNode` |
| `notes_delete` | `{ rel_path }` | `()` |
| `notes_reveal` | `{ rel_path }` | `()` opens in Explorer via `tauri-plugin-opener` |

```rust
#[derive(Serialize)]
pub struct NoteNode {
    pub name: String,
    pub rel_path: String,           // forward slashes, relative to root
    pub kind: NodeKind,             // File | Folder
    pub mtime_ms: u64,
    pub size: u64,
    pub children: Option<Vec<NoteNode>>,
}
```

### 8.3 Safety rules

- Every `rel_path` is joined to the root and canonicalized; the result must start with the canonical root or the command returns `AppError::OutsideRoot`. Rejects `..`, absolute paths, and symlinks that escape.
- File names are validated against Windows reserved names (`CON`, `PRN`, `AUX`, `NUL`, `COM1..9`, `LPT1..9`) and illegal characters `<>:"/\|?*`.
- `notes_write` is atomic: write to `<name>.md.tmp-<uuid>` in the same directory, then rename over the target. If `expected_mtime_ms` is given and does not match the file on disk, return `AppError::Conflict { current_mtime_ms }` so the frontend can offer overwrite or reload.
- `notes_delete` moves to the Recycle Bin via the `trash` crate, never a hard delete. Folder deletion requires the folder to be empty unless `force: true`.
- `notes_create` for a file writes a single `# <name>` line so the editor opens with something.

### 8.4 External changes

A `notify` watcher on the root (recursive) emits `notes://changed { rel_path, kind: "create" | "modify" | "remove" | "rename" }`, debounced 300 ms. The frontend refreshes the tree and, if the open note changed on disk and the editor is clean, reloads it.

Editor behaviour (frontend contract): autosave 600 ms after the last keystroke and on blur, tab switch, and window close. The frontend keeps `mtime_ms` from the last read or write and passes it as `expected_mtime_ms`.

---

## 9. Persistence (`state`)

Two files in `app.path().app_config_dir()` (`%APPDATA%\com.liamread.terminalgrid\` on Windows, adjust identifier):

`config.json` (user settings):

```json
{
  "version": 1,
  "shell": null,
  "shellArgs": [],
  "notesRoot": null,
  "agentPatterns": ["claude", "codex", "gemini", "aider", "gpt", "copilot"],
  "idleTimeoutMs": 4000,
  "hotkeyModifier": "ctrl+alt",
  "fontFamily": null,
  "fontSize": 14,
  "scrollback": 10000,
  "restoreSessionOnLaunch": true,
  "triggers": { "enabled": false, "armed": [] }
}
```

`session.json` (last layout, written on every change, debounced 1 s, and on exit):

```json
{
  "version": 1,
  "activeTab": "terminals",
  "focusedId": "…",
  "page": 0,
  "terminals": [
    { "id": "…", "cwd": "C:/dev/app", "labelOverride": null },
    { "id": "…", "cwd": "C:/dev/app-feature", "labelOverride": "feature box" }
  ],
  "notes": { "openRelPath": "ideas/kafka.md", "expandedFolders": ["ideas"] }
}
```

On launch, if `restoreSessionOnLaunch` is true, the frontend calls `create_terminal` for each saved entry in order (cwd only; shells are not resumed, that is not possible with ConPTY). If a saved cwd no longer exists, the terminal starts in the user's home and the header shows a notice.

Commands: `config_get`, `config_set { patch }`, `session_get`, `session_set { session }`.

### 9.1 Process definitions and arming

`config.json` has a `triggers` block: `enabled` is the master switch (off by default) and `armed` lists the ids of the process definitions the user has armed. It is local only and never goes in `session.json` or to the cloud.

Local process definitions are JSON files in `processes/` inside the same folder, one per process; the file name without `.json` is the process id. The folder is created on first use. Only the fields triggers need are read, the rest of the file is left alone:

```json
{
  "schemaVersion": 1,
  "name": "Keep the docs tidy",
  "triggers": [
    { "id": "saved", "kind": "fs.file-saved", "when": { "path": "${notes.root}", "glob": "**/*.md" } }
  ]
}
```

A file is usable when it is a JSON object with `schemaVersion` 1 and a non empty `name`, and `triggers`, when present, is an array of `{ id, kind, when }` with unique ids. A file that is not usable is still listed by `triggers_status`, with a `problem` text, and has no rules. Files over 1 MB are not read.

Strings in `when` may use `${home}`, `${downloads}`, `${notes.root}`, `${screenshots}` and `${repo.root}`. `${repo.root}` makes one rule per distinct repository root among the open panes. A rule whose variable has no value on this machine is `skipped` with a reason. Rule ids are `<processId>/<triggerId>`, with `#<n>` added (n from 1) when `${repo.root}` produced several.

### 9.2 Internal triggers (`tg`)

Things the app already sees become triggers with no new crate, because only the app sees them and a crate would need the app to feed it. The source lives in `src-tauri/src/triggers/internal/`: `mod.rs` (`InternalSource`, source id `tg`, and the `InternalSignal` enum), `kinds.rs` (`KINDS`), `rule.rs` (parsing a rule's `when` per kind), `logic.rs` (the pure `step(&[Armed], &InternalSignal)`, tested in `logic/tests.rs`) and a `README.md` with the same sections as a crate. The catalogue and the editor see it like any other source. `triggers/feeds.rs` holds the helpers the feed points call.

`InternalSource` is cloneable (an `Arc` inside). `sources()` boxes one clone for the hub and `TriggerService` keeps another. `start` stores the parsed rules and the sink, and dropping the handle clears them. The app feeds it with `TriggerService::publish_internal(signal)`, which runs `logic::step` and sends an event for every rule that matches. `TriggerService::wants_output()` reads an `AtomicBool` that is true only while a `tg.output-matched` rule is armed, so the reader thread skips line assembly otherwise.

| Kind | Signal | Fed from |
|---|---|---|
| `tg.agent-finished` | `AgentFinished` | `agent/mod.rs`, after the finished event is emitted (starts publish nothing) |
| `tg.command-failed`, `tg.command-slow` | `CommandEnded` | the reader thread tap, on `OSC 133;D` (4.3) |
| `tg.output-matched` | `OutputLine` | the reader thread tap, only while a rule is armed |
| `tg.bell` | `Bell` | the reader thread tap |
| `tg.task-reported` | `TaskReported` | `agent/task.rs`, when a new task line is read (emptying the file publishes nothing) |
| `tg.pane-opened-in-repo` | `PaneOpened` | `create_terminal`, on a spawned blocking thread after the spawn and a refresh; the repository root comes from the git cache and a pane outside a repository fires nothing |
| `tg.worktree-created`, `tg.worktree-removed` | `WorktreeCreated`, `WorktreeRemoved` | `git/worktree.rs`, after git succeeded |
| `tg.review-staged` | `ReviewStaged` | `git/review.rs`, `review_stage`, after `git add` succeeded |
| `tg.note-saved` | `NoteSaved` | `notes_write`, after the write returned Ok |
| `tg.focus-entered` | `FocusEntered` | `set_focused_terminal`, only when focus moves onto a different pane |
| `tg.app-started` | `AppStarted` | `triggers::start`, once, after the first refresh has armed the rules |

Feeds publish after the work they report has succeeded. The pane-opened feed checks that process triggers are switched on before its git lookup, so nothing runs while the feature is off.

Payloads carry `pane`, `cwd`, `repo`, `agent`, `exit_code`, `duration_ms`, `line`, `path`, `relative`, `branch` and `files` as each signal has them, never the full output. The `when` fields per kind: `agent` (glob) for `tg.agent-finished`; `exit_code` for `tg.command-failed` (default: any non zero); `longer_than_ms` (required) and `exit_code` for `tg.command-slow`; `pattern` (required) for `tg.output-matched`, a `regex` 1.13 expression, case insensitive, with a 256 KiB size limit, compiled once when the rule starts; `repo` (absolute path) on every pane, worktree and review kind except `tg.bell`, where pane kinds match it against the pane's working directory; `glob` (relative to the notes root) for `tg.note-saved`. Unknown fields are a rule error. `tg.agent-waiting`, `tg.note-tagged`, `tg.file-dropped`, `tg.file-opened` and `tg.tray-menu` do not exist yet.

Known limits:

- Commands the shell does not record in its history are not reported, so `tg.command-failed` and `tg.command-slow` miss them: bash with `HISTCONTROL=ignoredups` or `ignorespace`, and zsh with `HIST_IGNORE_DUPS`, `HIST_IGNORE_SPACE` and the other `HIST_IGNORE_*` options. The script reports a command only when the history number changed (4.5).
- `tg.output-matched` can match the echoed command line the shell prints back as the user types, not only the command's own output.
- A rule sees at most 200 characters of a line, and the scanner keeps at most 1024 bytes of one line.
- An `OSC 133;D` with no code counts as exit 0.
- A command's duration starts at the first Enter after a prompt mark (or after the last command end); later Enters typed into a running program do not restart it. A shell with no marks (cmd.exe, `sh`) reports no command ends at all.
- zsh runs with `-i` and is not a login shell, so the user's `.zprofile` and `.zlogin` do not run for a pane.
- Each rule has the hub's two second cool down, and rules run on the thread that publishes the signal, so a pattern is matched on the terminal reader thread. The reader thread guards the tap with `catch_unwind`: if the tap panics, that pane stops being tapped and its output keeps flowing.

---

## 10. IPC contract summary

Events (Rust → frontend), all emitted to the `main` window:

| Event | Payload |
|---|---|
| `terminal://output` | `{ id, dataB64 }` |
| `terminal://exit` | `{ id, code: number \| null }` |
| `terminal://cwd` | `{ id, cwd }` echo after `report_cwd`, lets other components react |
| `terminal://git` | `{ id, info: GitInfo }` |
| `terminal://agent` | `{ id, event: "started" \| "finished", name, durationMs? , source: "process" \| "shell" \| "bell" \| "idle" }` |
| `terminal://stats` | `{ id, cpuPercent, memBytes }` sampled on the AgentWatcher tick for the session process tree; the pane header renders it next to the agent badge |
| `terminal://task` | `{ id, task: string \| null, updatedAtMs }` when a session's `TERMINAL_GRID_TASK_FILE` changes (section 5.5) |
| `notes://changed` | `{ relPath, kind }` |
| `trigger://fired` | `{ processId, processName, ruleId, kind, at, payload, coalesced }` a process trigger fired. `at` is epoch ms, `payload` holds the source's fields (for `trigger-fs`: path, name, relative, ext), `coalesced` counts events the 2 s cool down dropped since the last one for this rule |

Frontend → Rust commands are listed in sections 4.4, 7 (review commands in 7.5), 8, 9 and the process trigger commands below. All commands return `Result<T, AppError>` where `AppError` serializes to `{ code: string, message: string, ...extra }`. Codes: `NotFound`, `SpawnFailed`, `Io`, `OutsideRoot`, `Conflict`, `InvalidName`, `GitTimeout`, `GitNotInstalled`.

Process trigger commands (`src-tauri/src/triggers/`):

| Command | In | Out |
|---|---|---|
| `triggers_catalogue` | `{}` | `SourceInfo[]`: every source with its kinds and availability |
| `triggers_check` | `{ rule: { id, kind, when } }` | `{ ok: true }` or `{ ok: false, reason }`; the rule is checked as given, variables are not expanded |
| `triggers_status` | `{}` | `{ enabled, processes: { id, name, armed, problem?, rules: { id, kind, state, reason? }[] }[] }`; recomputes first. `state` is `active`, `rejected`, `sourceFailed` or `skipped` |
| `triggers_arm` | `{ processId, armed }` | `void`; `NotFound` when arming an id that has no definition file |

Turning triggers on or off is a `config_set` patch: `{ triggers: { enabled } }`.

Field naming: Rust structs use `#[serde(rename_all = "camelCase")]` so the frontend sees camelCase throughout. Command argument names are camelCase on the JS side and Tauri maps them to snake_case Rust parameters.

TypeScript types for the contract live in `src/ipc/types.ts` and are the single file the design needs to import from. They are generated by hand for v1; `tauri-specta` can replace that later.

---

## 11. Tauri configuration

`tauri.conf.json` (relevant parts):

```json
{
  "productName": "Terminal Grid",
  "identifier": "com.liamread.terminalgrid",
  "app": {
    "windows": [{
      "label": "main",
      "title": "Terminal Grid",
      "fullscreen": true,
      "resizable": true,
      "decorations": true,
      "minWidth": 1024,
      "minHeight": 640
    }],
    "security": {
      "csp": "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self' data:"
    }
  },
  "bundle": {
    "active": true,
    "targets": ["nsis", "msi"],
    "windows": {
      "webviewInstallMode": { "type": "embedBootstrapper" },
      "nsis": { "installMode": "perUser" }
    },
    "resources": ["resources/*"]
  }
}
```

`capabilities/default.json`:

```json
{
  "identifier": "default",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:window:allow-set-fullscreen",
    "core:window:allow-is-fullscreen",
    "core:window:allow-close",
    "dialog:allow-open",
    "dialog:allow-ask",
    "opener:allow-reveal-item-in-dir"
  ]
}
```

No shell plugin and no fs plugin: every file and process operation goes through the Rust commands above, which keeps the capability surface small and the sandboxing rules in one place.

`'unsafe-inline'` for styles is needed by xterm.js; scripts stay strict.

---

## 12. Windows specifics

- ConPTY requires Windows 10 1809 or later. portable-pty uses it by default on Windows; no WinPTY fallback is shipped.
- Prefer `pwsh.exe` (PowerShell 7) over `powershell.exe`: better ANSI handling and faster startup. Detected via `where.exe pwsh` at launch, cached.
- Set `$OutputEncoding` and `[Console]::OutputEncoding` to UTF-8 in the integration script so non ASCII output renders correctly in xterm.
- All `git.exe` and `where.exe` spawns use `CREATE_NO_WINDOW`.
- Paths are stored with forward slashes internally and converted with `PathBuf` at the boundary; git returns forward slashes on Windows anyway.
- `sysinfo` on Windows needs `refresh_processes` with `ProcessRefreshKind::new().with_cmd(UpdateKind::Always)` or command lines are empty and the Claude Code (`node.exe`) match fails.
- Killing a shell kills the ConPTY, which sends the children a close; agents get a chance to clean up. `close_terminal` calls `child.kill()` then waits up to 2 s before dropping.
- Build machine needs the MSVC toolchain (`rustup default stable-msvc`), Visual Studio Build Tools with the C++ workload, and Node 22. WebView2 runtime is bootstrapped by the installer.

---

## 13. Performance notes

- Use `@xterm/addon-webgl` with a canvas fallback; 9 visible terminals on the DOM renderer is noticeably slower on 4K.
- `scrollback` 10 000 lines per terminal by default; 9 terminals at that size is roughly 50 MB, acceptable.
- Git refresh never runs more than 3 concurrent subprocesses (a semaphore in `GitService`); extra requests coalesce onto the pending one for the same repo.
- AgentWatcher refreshes the process list once per tick, not once per session.
- Output events go through `emit_to("main", …)` rather than a broadcast `emit`, and the frontend keeps a single listener that routes by id rather than one listener per pane.

---

## 14. Testing

Unit tests in Rust:

- `git::worktree::parse` over the fixtures in 7.3 plus edge cases (bare, missing branch line, path with spaces, trailing newline variations).
- `notes::paths::resolve` rejects `..`, absolute paths, reserved names, symlink escapes.
- `agent::state` transitions with minimum running time and duplicate suppression.
- `pty::reader` coalescing with a fake reader (size and time thresholds).

Unit tests for process triggers, all run by `cargo test --workspace` offline and without a display:

- `trigger-core`: the JSON the frontend sees (`Platforms`, `Permission`, `Event.at`), `kinds_markdown` against an exact table, the test kit (`RecordingSink`, `FixedClock`, `FakeSource`).
- `trigger-hub`: apply, restart only the sources whose rules changed, rejected rules, a source whose start fails, the two second cool down and the `coalesced` count, using `FakeSource`.
- `trigger-fs`: `rule.rs` parsing and `logic.rs` (globs and default ignores, settle and debounce, rename pairing, download finished) as pure tests over observed signals. No timers and no disk.
- Each source crate's `tests/readme.rs` fails when the Kinds table in its README is stale. Refresh it with `cargo run -p trigger-<name> --example readme`.
- The app side (`src-tauri/src/triggers/`): definition parsing, variable expansion with string level tests for `${screenshots}` on every OS, the rule planner, the status and `trigger://fired` JSON, and `TriggerService::refresh` applying only when the rules change, using a hub with no sources.
- The internal source (`src-tauri/src/triggers/internal/`): `rule.rs` parsing per kind, `logic::step` over every kind (agent glob, exit code and duration filters, the 200 character cut, note glob, `repo`), `InternalSource` start, publish and drop, `sources()` and `publish_internal` with a `tg.bell` rule, and a unit test that fails while the Kinds table in its `README.md` is stale.
- `pty::marks` and `pty::tap`: the scanner over escape sequences however the chunks are cut, bell versus the end of an OSC string, exit codes, line stripping, and the command clock (start at the first Enter after a prompt, end at the next command end).
- Shell integration, in `src-tauri/src/pty/shell/live.rs` (unix only): the real zsh and the real bash are started under a pty with the app's real launch arguments and variables, in a fake home. The tests assert that `OSC 133;D` appears, that a variable set in the user's own rc file is visible, that a repeated command is reported each time, and that a failing command and an `echo` give the signals a rule would receive. A shell that is not installed makes its test skip. The PowerShell script cannot run on these machines, so a test only reads it.
- The feed points: one end to end test in `triggers/feeds.rs` arms a rule for each kind the app feeds from its own code and checks each fires with its payload.

Live tests, which watch the real file system, are `#[ignore]` in `tests/live.rs` of a crate and run with `cargo test -p trigger-fs -- --ignored --test-threads=1`. Each uses its own canonicalised temp directory, waits 400 ms after the watcher starts, and waits for events with a 10 second ceiling rather than a fixed sleep; a negative check first waits for an event on a sentinel file. CI runs them on macOS, Windows and Linux after the other checks.

CI (`.github/workflows/ci.yml`) runs on each of the three systems: `npm test`, `npm run build`, `cargo fmt --check`, `cargo clippy` with `-D warnings` on the three trigger crates, `cargo test --workspace`, `node plans/_tools/check-crates.mjs` (crate isolation, README sections, file sizes) and the live tests.

Manual check for triggers: in Settings switch on "Process triggers (experimental)", put a definition file in the `processes/` folder with a `fs.file-saved` trigger on a folder, tick "Armed", save a file in that folder, and confirm the toast "Trigger fs.file-saved fired for <name>" appears within a few seconds.

Manual check for the internal source: in a process definition add a `tg.command-failed` trigger with no `when`, arm it, open a pane, run `false`, and confirm the same toast appears with the kind `tg.command-failed`. Do the same in a zsh pane and a bash pane.

Unit tests in TypeScript:

- `layoutFor`, `pageOf`, and the move function across page boundaries.
- OSC parsing for `7`, `133`, `7777`.

Manual checklist mapped to the ten requirements, run on Windows 11 with pwsh, Windows PowerShell 5.1 and cmd.exe:

1. Spawn 1, 2, 3, 4, 5, 6, 7, 9, 10 terminals and confirm layouts and the page indicator.
2. Move a terminal across every edge and across a page boundary.
3. `cd` inside a worktree in pane 1, confirm pane 2 (same repo) lists it as open in pane 1 within a second.
4. Run `claude` in a pane, switch focus away, finish the task, confirm the finished event arrives from `process` and the shell source does not double fire.
5. Create, edit, rename, delete a note; delete lands in the Recycle Bin; edit the file in another editor and confirm the reload prompt.
6. Close the app with an agent running; confirm the prompt; relaunch; confirm cwds restore.

---

## 15. Milestones

1. **Core PTY loop:** spawn, output, input, resize, close, restore. One terminal, plain page.
2. **Grid and hotkeys:** layout function, paging, movement, focus, fullscreen.
3. **Git and worktrees:** service, cache, watcher, events, pane header data.
4. **Agent detection:** process watcher, shell integration script, bell, idle fallback.
5. **Notes:** service, tree, editor and viewer wiring, autosave, external change handling.
6. **Polish and packaging:** settings, NSIS installer, README with the Claude Code bell tip.

Frontend from Claude Design lands after milestone 2, when the IPC types and event flow are stable.

---

## 16. Decisions to confirm

- Default shell: pwsh when present, else Windows PowerShell. Confirm you do not want cmd.exe as a first class option beyond "works without integration".
- Agent patterns: the default list above, matched against the full command line. Add anything else you run.
- Hotkey modifier: Ctrl+Alt as requested, with Ctrl+Shift as the alternative in settings because of AltGr on Dutch layouts.
- Notes deletion goes to the Recycle Bin rather than a permanent delete.
- Session restore respawns shells in their saved directories on launch; no attempt to replay history.
