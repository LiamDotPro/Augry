---
id: T39
epic: 001-trigger-core
title: Add the Processes rows to the Settings dialog
status: todo
depends_on: [T38]
size: S
model: haiku
files:
  - src/components/SettingsDialog.tsx
  - src/styles/overlays.css
---

## Goal

The Settings dialog has a "Process triggers (experimental)" switch and, when it is on, one row per local process definition with an "Armed" checkbox and a one line status. The switch and the checkboxes apply at once through the actions from T38. No new screen and no design export (Decision 16).

## Read first

- `src/components/SettingsDialog.tsx:1-10` imports, `:60-75` the component start, `:140-150` the first effects, `:360-395` the Notes folder row and the end of `sheet__body`
- `src/styles/overlays.css:130-170` the `.row` rules and `:225-260` `.segmented`
- `src/lib/triggers.ts` `processStatusLine` (T37)
- `src/ipc/types.ts` `TriggerProcessStatus`

## Steps

1. In `src/components/SettingsDialog.tsx`, below the line `import { IS_MAC } from "../lib/hotkeys";` add:

```tsx
import { processStatusLine } from "../lib/triggers";
```

2. In the component `SettingsDialog`, directly after the line `const sheet = useRef<HTMLFormElement>(null);` add:

```tsx
  const triggersOn = state.config.triggers.enabled;
  const triggers = state.triggers.status;
```

3. In the same component, directly after the existing effect that focuses the first control (the one whose body is `sheet.current?.querySelector<HTMLElement>("select, input, button")?.focus();`) add:

```tsx

  // The list of definitions is read each time the sheet opens, so a file
  // added since shows up.
  useEffect(() => {
    void actions.loadTriggersStatus();
  }, [actions]);
```

4. In the JSX, directly after the closing `</div>` of the "Notes folder" row (the row whose label is `Notes folder`) and before the `</div>` that closes `sheet__body`, add this block. It reuses `.row`, `.row__label`, `.row__hint`, `.row__note` and `.segmented`. The switch and the checkboxes are not part of the draft: they call the actions at once, so Cancel does not undo them.

```tsx

          <div className="row">
            <div className="row__label" id="settings-triggers">
              Process triggers (experimental)
              <div className="row__hint">
                Applies as soon as you change it. Definitions are JSON files in the processes folder
                of the app's config folder.
                {IS_MAC &&
                  " macOS may ask for access to Desktop, Documents or Downloads the first time a trigger watches them."}
              </div>
            </div>
            <div className="segmented" role="radiogroup" aria-labelledby="settings-triggers">
              {[false, true].map((value) => (
                <button
                  key={String(value)}
                  type="button"
                  role="radio"
                  aria-checked={triggersOn === value}
                  className={cx("segmented__item", triggersOn === value && "segmented__item--active")}
                  onClick={() => void actions.setTriggersEnabled(value)}
                >
                  {value ? "On" : "Off"}
                </button>
              ))}
            </div>
          </div>

          {triggersOn &&
            triggers?.processes.map((process) => (
              <div className="row row--top" key={process.id}>
                <div className="row__label">{process.name}</div>
                <div className="process">
                  <label className="process__armed">
                    <input
                      type="checkbox"
                      checked={process.armed}
                      disabled={Boolean(process.problem) && !process.armed}
                      onChange={(event) => void actions.armProcess(process.id, event.target.checked)}
                    />
                    Armed
                  </label>
                  <div className="row__hint">{processStatusLine(process)}</div>
                </div>
              </div>
            ))}

          {triggersOn && triggers && triggers.processes.length === 0 && (
            <div className="row">
              <div className="row__label">Definitions</div>
              <div className="row__note">No process definitions found.</div>
            </div>
          )}
```

5. In `src/styles/overlays.css`, directly above the comment line `/* Segmented choice ---` add:

```css
/* Process triggers list ------------------------------------------------ */

.process {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
  padding-top: 6px;
}

.process__armed {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
  color: var(--text-85);
}

.process__armed input {
  accent-color: var(--accent);
}

.process .row__hint {
  margin-top: 0;
  overflow-wrap: anywhere;
}
```

## Acceptance

- [ ] `npm run build` exits 0
- [ ] `npm test` passes
- [ ] `grep -c "call(" src/components/SettingsDialog.tsx` prints `0`
- [ ] `sed -n '/Process triggers list/,/Segmented choice/p' src/styles/overlays.css | grep -c '#'` prints `0` (no hard coded colours in the new rules)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- A trigger picker, an editor for definition files, or showing rule details.
- Putting the switch or the checkboxes into the draft that Save commits.
- A new design export or changes to `design/`.
- Polling the status. It is read when the sheet opens and after each change.

## If stuck

- If the "Notes folder" row is no longer the last row of `sheet__body`, add the block after the last row anyway and say so in the Log.
- If `tsc` says `state.triggers` or `state.config.triggers` does not exist, T37 is not built. Set status blocked.
- If `tsc` says `actions.loadTriggersStatus` does not exist, T38 is not built. Set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
