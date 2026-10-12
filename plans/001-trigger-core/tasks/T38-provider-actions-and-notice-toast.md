---
id: T38
epic: 001-trigger-core
title: Add the trigger actions, the fired event and the notice toast
status: todo
depends_on: [T37]
size: S
model: haiku
files:
  - src/state/AppProvider.tsx
  - src/App.tsx
  - src/styles/overlays.css
---

## Goal

`useAppActions()` has `loadTriggersStatus`, `setTriggersEnabled`, `armProcess` and `dismissNotice`. The provider listens for `trigger://fired` and shows "Trigger fs.file-saved fired for Keep the docs tidy" in a neutral toast that goes away after six seconds. Components still do not call `call()`.

## Read first

- `src/state/AppProvider.tsx:60-75` imports, `:100-125` the start of `AppActions`, `:300-345` the event subscriptions, `:410-422` the note toast timer to copy, `:625-635` the first actions in the returned object
- `src/App.tsx:80-98` the error toast
- `src/styles/overlays.css:338-375` the error toast styles
- `src/lib/triggers.ts` `firedNotice` and `NOTICE_MS` (T37)

## Steps

1. In `src/state/AppProvider.tsx`, in the `import type { ... } from "../ipc/types";` list add `TriggerFiredEvent,` after `TerminalInfo,`. Below the line `import { cdCommand, clearLineKey } from "../lib/shellCommand";` add:

```ts
import { firedNotice, NOTICE_MS } from "../lib/triggers";
```

2. In `interface AppActions`, directly after `dismissError(): void;` add:

```ts
  dismissNotice(): void;

  /** Reads the triggers status, which also picks up definition files added since. */
  loadTriggersStatus(): Promise<void>;
  /** The "Process triggers (experimental)" switch. Applies at once; no Save needed. */
  setTriggersEnabled(enabled: boolean): Promise<void>;
  /** Arms or disarms a local process definition. Applies at once. */
  armProcess(processId: string, armed: boolean): Promise<void>;
```

3. In the effect that subscribes to backend events, in the `pending` array, directly after the `subscribe<NotesChangedEvent>(...)` entry (it ends with `}),`) add:

```ts
      subscribe<TriggerFiredEvent>(EVENTS.triggerFired, (event) => {
        dispatch({ type: "ui/notice", notice: { message: firedNotice(event), at: Date.now() } });
      }),
```

4. Directly above the comment line `  // Notes autosave ----` add this effect, which is the note toast timer applied to the notice:

```ts
  // The trigger notice goes away by itself.
  useEffect(() => {
    const notice = state.notice;
    if (!notice) return;
    const timer = window.setTimeout(() => {
      if (stateRef.current.notice === notice) dispatch({ type: "ui/notice", notice: null });
    }, NOTICE_MS);
    return () => window.clearTimeout(timer);
  }, [state.notice]);

```

5. In the object returned from the `useMemo` that builds the actions, directly after the line `dismissError: () => dispatch({ type: "ui/error", message: null }),` add:

```ts
      dismissNotice: () => dispatch({ type: "ui/notice", notice: null }),

      loadTriggersStatus: async () => {
        try {
          dispatch({ type: "triggers/status", status: await call("triggers_status", {}) });
        } catch (error) {
          fail(error);
        }
      },

      setTriggersEnabled: async (enabled) => {
        try {
          const config = await call("config_set", { patch: { triggers: { enabled } } });
          dispatch({ type: "config/set", config });
          dispatch({ type: "triggers/status", status: await call("triggers_status", {}) });
        } catch (error) {
          fail(error);
        }
      },

      armProcess: async (processId, armed) => {
        try {
          await call("triggers_arm", { processId, armed });
          dispatch({ type: "config/set", config: await call("config_get", {}) });
          dispatch({ type: "triggers/status", status: await call("triggers_status", {}) });
        } catch (error) {
          fail(error);
        }
      },
```

6. In `src/App.tsx`, directly after the closing `)}` of the `{state.error && ( ... )}` block and before the closing `</div>` of the app, add:

```tsx

      {state.notice && (
        <div className={cx("toast", "toast--notice", state.error && "toast--raised")} role="status">
          <span>{state.notice.message}</span>
          <button
            type="button"
            className="toast__dismiss"
            aria-label="Dismiss"
            onClick={actions.dismissNotice}
          >
            ×
          </button>
        </div>
      )}
```

7. In `src/styles/overlays.css`, directly after the rule `.toast__dismiss:hover { ... }` (the last rule in the file) add:

```css

/* Notice toast: the same toast in neutral colours, for things that are not errors. */
.toast--notice {
  background: var(--glass-sheet);
  border-color: var(--edge-strong);
  color: var(--text-90);
}

/* Sits above the error toast when both are showing. */
.toast--raised {
  bottom: 76px;
}
```

## Acceptance

- [ ] `npm run build` exits 0
- [ ] `npm test` passes
- [ ] `grep -n "call(" src/components/SettingsDialog.tsx src/App.tsx` prints nothing (components still never call IPC)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- The Settings rows. T39 does them.
- A sound, a system notification or a click action on the notice.
- Showing the event payload (paths) in the toast. It is not shown anywhere.
- Colours outside `tokens.css`. The toast uses existing tokens only.
- Changing the error toast.

## If stuck

- If the anchor lines in steps 3 to 6 are not found exactly, the file changed since this card was written. Set status blocked and quote what is there.
- If `tsc` says `TriggerFiredEvent` is not exported, T36 did not add it to `src/ipc/types.ts`. Set status blocked.
- If `tsc` says `triggers_status` is not a key of `Commands`, T36 is not built. Set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
