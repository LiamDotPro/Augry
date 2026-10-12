---
id: T36
epic: 001-trigger-core
title: Add the trigger commands, refresh calls, IPC types and docs
status: todo
depends_on: [T35]
size: M
model: haiku
files:
  - src-tauri/src/triggers/mod.rs
  - src-tauri/src/lib.rs
  - src-tauri/src/pty/mod.rs
  - src-tauri/src/state/mod.rs
  - src/ipc/types.ts
  - docs/technical-design.md
---

## Goal

The four commands `triggers_catalogue`, `triggers_check`, `triggers_status` and `triggers_arm` exist and are registered. The rules are refreshed when a terminal is created or closed, when a working directory changes and when the config is saved. The commands, the `trigger://fired` event and their TypeScript types are in `src/ipc/types.ts`, and section 10 of the technical design lists them.

## Read first

- `plans/001-trigger-core/epic.md` Decision 13 (when `refresh()` runs), Decision 15 and the "Commands" table in the Shape
- `src-tauri/src/triggers/mod.rs` the service from T35
- `src-tauri/src/pty/mod.rs:498-523` and `:558-562` and `:592-621` `create_terminal`, `close_terminal`, `report_cwd`
- `src-tauri/src/state/mod.rs:300-320` `config_set`
- `src/ipc/types.ts` and `docs/technical-design.md:545-566` section 10

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance.

2. In `src-tauri/src/triggers/mod.rs`, replace this import line:

```rust
use tauri::{AppHandle, Emitter, Manager};
```

with:

```rust
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
```

and add this line directly above `use crate::state::TriggersConfig;`:

```rust
use crate::error::{AppError, Result};
```

Because `Result` now means the app's one-argument alias, change the signature of `TriggerService::check` to `pub fn check(&self, rule: &Rule) -> std::result::Result<(), RuleError> {` (it was `Result<(), RuleError>`).

3. In the same file, add this method to `impl TriggerService`, directly above the doc comment `/// Starts the thread that turns each fired trigger`:

```rust
    /// True when a definition file with this id exists.
    pub fn has_definition(&self, id: &str) -> bool {
        store::load(&self.dir).iter().any(|def| def.id == id)
    }

```

4. In the same file, add this block directly above the line `#[cfg(test)]` that opens `mod tests`, with one blank line before and after it. `triggers_status` refreshes first so a definition file added since the last refresh shows up. `triggers_arm` refuses to arm an id that has no file, saves the armed list in the config and refreshes.

```rust
// Commands ----------------------------------------------------------------

/// The answer of `triggers_check`: `{ ok: true }` or `{ ok: false, reason }`.
#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[tauri::command]
pub fn triggers_catalogue(state: State<'_, crate::AppState>) -> Vec<SourceInfo> {
    state.triggers.catalogue()
}

#[tauri::command]
pub fn triggers_check(state: State<'_, crate::AppState>, rule: Rule) -> CheckResult {
    match state.triggers.check(&rule) {
        Ok(()) => CheckResult {
            ok: true,
            reason: None,
        },
        Err(error) => CheckResult {
            ok: false,
            reason: Some(error.to_string()),
        },
    }
}

/// Recomputes first, so a definition file added since the last refresh shows up.
#[tauri::command]
pub async fn triggers_status(app: AppHandle) -> Result<TriggersStatus> {
    crate::blocking(move || {
        let state = app.state::<crate::AppState>();
        refresh_now(&state)
    })
    .await
}

#[tauri::command]
pub async fn triggers_arm(app: AppHandle, process_id: String, armed: bool) -> Result<()> {
    crate::blocking(move || {
        let state = app.state::<crate::AppState>();
        if armed && !state.triggers.has_definition(&process_id) {
            return Err(AppError::NotFound {
                message: "there is no process definition with that id".into(),
            });
        }
        state.state.update_config(|config| {
            config.triggers.armed.retain(|id| id != &process_id);
            if armed {
                config.triggers.armed.push(process_id.clone());
            }
        })?;
        refresh_now(&state);
        Ok(())
    })
    .await?
}
```

5. In the same file, inside `mod tests`, directly above its final closing `}`, add these two tests (keep a blank line before the first):

```rust

    #[test]
    fn has_definition_looks_for_the_file() {
        let (service, dir) = service();
        assert!(service.has_definition("tidy"));
        assert!(!service.has_definition("nope"));
        cleanup(&dir);
    }

    #[test]
    fn check_results_have_the_shape_the_frontend_reads() {
        let ok = CheckResult {
            ok: true,
            reason: None,
        };
        assert_eq!(
            serde_json::to_value(ok).unwrap(),
            serde_json::json!({ "ok": true })
        );
        let bad = CheckResult {
            ok: false,
            reason: Some("unknown kind x.y".into()),
        };
        assert_eq!(
            serde_json::to_value(bad).unwrap(),
            serde_json::json!({ "ok": false, "reason": "unknown kind x.y" })
        );
    }
```

6. In `src-tauri/src/lib.rs`, in `tauri::generate_handler![...]`, find the comment line `            // window` and add these lines directly above it:

```rust
            // triggers
            triggers::triggers_catalogue,
            triggers::triggers_check,
            triggers::triggers_status,
            triggers::triggers_arm,
```

7. In `src-tauri/src/pty/mod.rs` make three changes.

First, in `create_terminal`, find these two lines:

```rust
        state.agent.wake();
        Ok(info)
    })
    .await?
}

#[tauri::command]
pub fn write_terminal(
```

and add one line after `state.agent.wake();` (the first `state.agent.wake();` in the file is in `create_terminal`; `restart_terminal` has a similar pair, leave that one alone):

```rust
        crate::triggers::refresh_app(&app);
```

Second, replace the whole function `close_terminal`:

```rust
#[tauri::command]
pub fn close_terminal(state: State<'_, crate::AppState>, id: TerminalId) -> Result<()> {
    state.pty.close(&id)
}
```

with:

```rust
#[tauri::command]
pub fn close_terminal(
    app: AppHandle,
    state: State<'_, crate::AppState>,
    id: TerminalId,
) -> Result<()> {
    state.pty.close(&id)?;
    crate::triggers::refresh_app(&app);
    Ok(())
}
```

Third, in `report_cwd`, find the lines:

```rust
            cwd: cwd.clone(),
        },
    );

    let app_handle = app.clone();
```

and add one line after the `);`:

```rust
    crate::triggers::refresh_app(&app);
```

(so the `);` line is followed by that call, then the blank line, then `let app_handle = app.clone();`). The frontend sends the same arguments as before; Tauri fills in `app`.

8. In `src-tauri/src/state/mod.rs`, replace the `config_set` command:

```rust
pub fn config_set(state: State<'_, crate::AppState>, patch: ConfigPatch) -> Result<Config> {
```

with:

```rust
pub fn config_set(
    app: AppHandle,
    state: State<'_, crate::AppState>,
    patch: ConfigPatch,
) -> Result<Config> {
```

and in its body add `crate::triggers::refresh_app(&app);` on the line directly above `Ok(config)` (the last line of the function).

9. In `src/ipc/types.ts` make three changes.

First, in the `EVENTS` object add this line after `notesChanged: "notes://changed",`:

```ts
  triggerFired: "trigger://fired",
```

Second, add this block directly above the line `// Commands (frontend to Rust) ---` (keep a blank line before it):

```ts
// Triggers ----------------------------------------------------------------

export type TriggerPlatform = "macos" | "windows" | "linux";

export type TriggerPermission =
  | "accessibility"
  | "screenRecording"
  | "location"
  | "calendar"
  | "reminders"
  | "microphone"
  | "camera"
  | "notifications"
  | { folder: string };

export interface TriggerFieldInfo {
  name: string;
  doc: string;
  required: boolean;
}

export interface TriggerKindInfo {
  id: string;
  title: string;
  summary: string;
  platforms: TriggerPlatform[];
  permission: TriggerPermission | null;
  when: TriggerFieldInfo[];
  payload: TriggerFieldInfo[];
}

export type TriggerAvailability =
  | { state: "available" }
  | { state: "unsupported"; reason: string }
  | { state: "needsPermission"; permission: TriggerPermission; how: string };

export interface TriggerSourceInfo {
  id: string;
  kinds: TriggerKindInfo[];
  availability: TriggerAvailability;
}

export interface TriggerRule {
  id: string;
  kind: string;
  when: Record<string, unknown>;
}

export type TriggerCheck = { ok: true } | { ok: false; reason: string };

/** "skipped" is a rule whose variable has no value on this machine. */
export type TriggerRuleState = "active" | "rejected" | "sourceFailed" | "skipped";

export interface TriggerRuleStatus {
  id: string;
  kind: string;
  state: TriggerRuleState;
  /** Present for every state except "active". */
  reason?: string;
}

export interface TriggerProcessStatus {
  id: string;
  name: string;
  armed: boolean;
  /** Why the definition file cannot be used; such a process has no rules. */
  problem?: string;
  rules: TriggerRuleStatus[];
}

export interface TriggersStatus {
  enabled: boolean;
  processes: TriggerProcessStatus[];
}

/** `trigger://fired`. `at` is epoch ms; `payload` holds the source's fields (paths and so on). */
export interface TriggerFiredEvent {
  processId: string;
  processName: string;
  ruleId: string;
  kind: string;
  at: number;
  payload: Record<string, unknown>;
  coalesced: number;
}
```

Third, in `interface Commands`, add this block directly above the comment `  // window`:

```ts
  // triggers
  triggers_catalogue: { in: Record<string, never>; out: TriggerSourceInfo[] };
  triggers_check: { in: { rule: TriggerRule }; out: TriggerCheck };
  triggers_status: { in: Record<string, never>; out: TriggersStatus };
  triggers_arm: { in: { processId: string; armed: boolean }; out: void };

```

10. In `docs/technical-design.md`, section 10 (it starts at the line `## 10. IPC contract summary`).

First, in the events table add this row directly after the `notes://changed` row:

```markdown
| `trigger://fired` | `{ processId, processName, ruleId, kind, at, payload, coalesced }` a process trigger fired. `at` is epoch ms, `payload` holds the source's fields (for `trigger-fs`: path, name, relative, ext), `coalesced` counts events the 2 s cool down dropped since the last one for this rule |
```

Second, in the paragraph that starts `Frontend → Rust commands are listed in sections`, change the end of the first sentence from `8, 9.` to `8, 9 and the process trigger commands below.`

Third, add this text directly after that paragraph (before `Field naming:`), with a blank line before and after:

```markdown
Process trigger commands (`src-tauri/src/triggers/`):

| Command | In | Out |
|---|---|---|
| `triggers_catalogue` | `{}` | `SourceInfo[]`: every source with its kinds and availability |
| `triggers_check` | `{ rule: { id, kind, when } }` | `{ ok: true }` or `{ ok: false, reason }`; the rule is checked as given, variables are not expanded |
| `triggers_status` | `{}` | `{ enabled, processes: { id, name, armed, problem?, rules: { id, kind, state, reason? }[] }[] }`; recomputes first. `state` is `active`, `rejected`, `sourceFailed` or `skipped` |
| `triggers_arm` | `{ processId, armed }` | `void`; `NotFound` when arming an id that has no definition file |

Turning triggers on or off is a `config_set` patch: `{ triggers: { enabled } }`.
```

11. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::tests` shows 6 tests, all ok
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p trigger-core -p trigger-hub -p trigger-fs --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `npm run build` exits 0
- [ ] `npm test` passes
- [ ] `grep -c "trigger://fired" docs/technical-design.md src/ipc/types.ts` prints a count of at least 1 for each file
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Using the commands from the frontend. T38 adds the actions.
- Adding `triggers` to the TypeScript `Config` type. T37 does that.
- Refreshing from `restart_terminal` (its directory does not change) or from the git sweep.
- Other sections of the technical design (2, 3, 9, 14). Epic slice E does them.
- Validating a rule's variables in `triggers_check`.

## If stuck

- If an anchor text in step 7 or 8 is not found, the file changed since this card was written. Set status blocked and quote the lines you see.
- If `cargo test` fails with `__cmd__triggers_...` not found, the command names in step 6 do not match the function names in step 4.
- If clippy reports a new warning, quote it and set status blocked; do not add `allow` attributes beyond those in the code above.
- If `npm run build` fails in a file other than `src/ipc/types.ts`, set status blocked and quote the first error.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
