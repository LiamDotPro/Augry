import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import type { Config, HotkeyModifier, ThemePreference } from "../ipc/types";
import { cx } from "../lib/cx";
import { useAppearance } from "../lib/appearance";
import { IS_MAC } from "../lib/hotkeys";
import { useAppActions, useAppState } from "../state/AppProvider";
import { useCloud } from "../cloud/CloudProvider";
import { AccountRows, DevicesView } from "./cloud/SettingsAccount";

/** The backend's fallback order differs by platform (src-tauri/src/pty/shell.rs). */
const IS_WINDOWS = typeof navigator !== "undefined" && /Win/.test(navigator.platform || navigator.userAgent);

export const SHELLS = IS_WINDOWS
  ? [
      { value: "", label: "Auto (pwsh, then Windows PowerShell)" },
      { value: "pwsh.exe", label: "pwsh.exe" },
      { value: "powershell.exe", label: "powershell.exe" },
      { value: "cmd.exe", label: "cmd.exe" },
    ]
  : [
      { value: "", label: "Auto ($SHELL, then bash)" },
      { value: "zsh", label: "zsh" },
      { value: "bash", label: "bash" },
      { value: "fish", label: "fish" },
    ];

export const FONTS = ["JetBrains Mono", "Cascadia Mono", "Consolas", "Fira Code", "Menlo"];

const THEMES: { value: ThemePreference; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
  { value: "black", label: "Black" },
  // The see-through window needs the macOS window material.
  ...(IS_MAC ? [{ value: "glass" as const, label: "Glass" }] : []),
];

const MIN_FONT_SIZE = 8;
const MAX_FONT_SIZE = 32;

type Draft = Pick<
  Config,
  | "shell"
  | "agentPatterns"
  | "hotkeyModifier"
  | "fontFamily"
  | "fontSize"
  | "notesRoot"
  | "theme"
  | "compactLayout"
  | "focusColor"
  | "finishedColor"
>;

/**
 * Settings sheet from screen 1f. Edits a draft and only commits through
 * `config_set` on Save, so Cancel really is a cancel. Theme and layout preview
 * live from the draft; on close the app reapplies the saved config.
 *
 * The account rows come first (settings-account); Devices opens the
 * signed-in devices list in place of the body (settings-devices), keeping
 * the draft as it was.
 */
export function SettingsDialog() {
  const state = useAppState();
  const actions = useAppActions();
  const cloud = useCloud();
  const [draft, setDraft] = useState<Draft>(() => toDraft(state.config, state.notes.root));
  // What the folder field started as. The configured path and the service's
  // canonical one differ in form on Windows (C:/x against \\?\C:\x), so
  // comparing with state.notes.root "changed" the folder on every save and
  // closed the open note.
  const [initialNotesRoot] = useState(draft.notesRoot);
  const [pattern, setPattern] = useState("");
  const [view, setView] = useState<"main" | "devices">("main");
  const sheet = useRef<HTMLFormElement>(null);
  useAppearance(draft.theme, draft.compactLayout, {
    focus: draft.focusColor,
    finished: draft.finishedColor,
  });

  // A cloud dialog opened from here (sign in, team settings) sits on top and
  // takes its own Escape; the devices list goes back to Settings first.
  const escape = useRef({ dialogOpen: false, view });
  escape.current = { dialogOpen: cloud.dialog !== null, view };
  useEffect(() => {
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape" || escape.current.dialogOpen) return;
      event.stopPropagation();
      if (escape.current.view === "devices") setView("main");
      else actions.setSettingsOpen(false);
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [actions]);

  useEffect(() => {
    if (view === "main") sheet.current?.querySelector<HTMLElement>("select, input, button")?.focus();
  }, [view]);

  // Signed out from the devices list (or by the server): back to Settings.
  useEffect(() => {
    if (!cloud.user) setView("main");
  }, [cloud.user]);

  const patch = <K extends keyof Draft>(key: K, value: Draft[K]) =>
    setDraft((current) => ({ ...current, [key]: value }));

  const commitPattern = () => {
    const value = pattern.trim();
    if (!value || draft.agentPatterns.includes(value)) {
      setPattern("");
      return;
    }
    patch("agentPatterns", [...draft.agentPatterns, value]);
    setPattern("");
  };

  const onPatternKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter" || event.key === ",") {
      event.preventDefault();
      commitPattern();
    } else if (event.key === "Backspace" && pattern === "" && draft.agentPatterns.length > 0) {
      patch("agentPatterns", draft.agentPatterns.slice(0, -1));
    }
  };

  const onSubmit = async (event: FormEvent) => {
    event.preventDefault();
    const trimmed = pattern.trim();
    const agentPatterns =
      trimmed && !draft.agentPatterns.includes(trimmed)
        ? [...draft.agentPatterns, trimmed]
        : draft.agentPatterns;
    await actions.saveConfig({ ...draft, agentPatterns });
    // The notes root is also a service level setting: switching it creates the
    // folder if needed and reloads the tree.
    if (draft.notesRoot && draft.notesRoot !== initialNotesRoot) {
      await actions.setNotesRoot(draft.notesRoot);
    }
    actions.setSettingsOpen(false);
  };

  return (
    <div className="scrim" onMouseDown={() => actions.setSettingsOpen(false)}>
      <form
        className="sheet"
        ref={sheet}
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        onMouseDown={(event) => event.stopPropagation()}
        onSubmit={(event) => void onSubmit(event)}
      >
        {view === "devices" ? (
          <DevicesView onBack={() => setView("main")} onClose={() => actions.setSettingsOpen(false)} />
        ) : (
          <>
            <div className="sheet__head">
              <h2 className="sheet__title">Settings</h2>
              <button
                type="button"
                className="sheet__close"
                aria-label="Close settings"
                onClick={() => actions.setSettingsOpen(false)}
              >
                ×
              </button>
            </div>

            <div className="sheet__body">
              <AccountRows onManageDevices={() => setView("devices")} />

              <div className="row">
                <label className="row__label" htmlFor="settings-shell">
                  Shell
                </label>
                <div className="field">
                  <select
                    id="settings-shell"
                    value={draft.shell ?? ""}
                    onChange={(event) => patch("shell", event.target.value || null)}
                  >
                    {SHELLS.map((shell) => (
                      <option key={shell.value} value={shell.value}>
                        {shell.label}
                      </option>
                    ))}
                    {/* A shell set in config.json, or on another platform, still shows. */}
                    {draft.shell && !SHELLS.some((shell) => shell.value === draft.shell) && (
                      <option value={draft.shell}>{draft.shell}</option>
                    )}
                  </select>
                  <span className="field__chevron" aria-hidden="true">
                    ▾
                  </span>
                </div>
              </div>

              <div className="row row--top">
                <div className="row__label">
                  Agent name patterns
                  <div className="row__hint">Process names that mark a pane as an agent</div>
                </div>
                <div className="tokens">
                  {draft.agentPatterns.map((value) => (
                    <span className="token" key={value}>
                      {value}
                      <button
                        type="button"
                        className="token__remove"
                        aria-label={`Remove ${value}`}
                        onClick={() =>
                          patch(
                            "agentPatterns",
                            draft.agentPatterns.filter((item) => item !== value),
                          )
                        }
                      >
                        ×
                      </button>
                    </span>
                  ))}
                  <input
                    className="tokens__input"
                    value={pattern}
                    placeholder="add pattern…"
                    aria-label="Add an agent name pattern"
                    onChange={(event) => setPattern(event.target.value)}
                    onKeyDown={onPatternKeyDown}
                    onBlur={commitPattern}
                  />
                </div>
              </div>

              {IS_MAC ? (
                <div className="row">
                  <div className="row__label">Hotkeys</div>
                  <div className="row__note">
                    Command (⌘), the macOS layout: ⌘N, ⌘W, ⌘1…9 and so on. Hold ⌘ to see them all.
                  </div>
                </div>
              ) : (
                <div className="row">
                  <div className="row__label" id="settings-modifier">
                    Hotkey modifier
                  </div>
                  <div className="segmented" role="radiogroup" aria-labelledby="settings-modifier">
                    {(["ctrl+alt", "ctrl+shift"] as HotkeyModifier[]).map((value) => (
                      <button
                        key={value}
                        type="button"
                        role="radio"
                        aria-checked={draft.hotkeyModifier === value}
                        className={cx(
                          "segmented__item",
                          draft.hotkeyModifier === value && "segmented__item--active",
                        )}
                        onClick={() => patch("hotkeyModifier", value)}
                      >
                        {value === "ctrl+alt" ? "Ctrl+Alt" : "Ctrl+Shift"}
                      </button>
                    ))}
                  </div>
                </div>
              )}

              <div className="row">
                <div className="row__label" id="settings-theme">
                  Theme
                  <div className="row__hint">
                    System follows your OS; Black is total darkness
                    {IS_MAC && "; Glass shows your desktop through the window (Dark in full screen)"}
                  </div>
                </div>
                <div className="segmented" role="radiogroup" aria-labelledby="settings-theme">
                  {THEMES.map(({ value, label }) => (
                    <button
                      key={value}
                      type="button"
                      role="radio"
                      aria-checked={draft.theme === value}
                      className={cx("segmented__item", draft.theme === value && "segmented__item--active")}
                      onClick={() => patch("theme", value)}
                    >
                      {label}
                    </button>
                  ))}
                </div>
              </div>

              <div className="row">
                <div className="row__label" id="settings-compact">
                  Compact layout
                  <div className="row__hint">Square panes, no borders, minimal gaps</div>
                </div>
                <div className="segmented" role="radiogroup" aria-labelledby="settings-compact">
                  {[false, true].map((value) => (
                    <button
                      key={String(value)}
                      type="button"
                      role="radio"
                      aria-checked={draft.compactLayout === value}
                      className={cx(
                        "segmented__item",
                        draft.compactLayout === value && "segmented__item--active",
                      )}
                      onClick={() => patch("compactLayout", value)}
                    >
                      {value ? "On" : "Off"}
                    </button>
                  ))}
                </div>
              </div>

              <div className="row">
                <div className="row__label">
                  Colours
                  <div className="row__hint">Reset follows the theme</div>
                </div>
                <div className="color-picks">
                  <ColorPick
                    label="Focus ring"
                    token="--focus-rgb"
                    value={draft.focusColor}
                    onChange={(value) => patch("focusColor", value)}
                  />
                  <ColorPick
                    label="Agent finished"
                    token="--finished-rgb"
                    value={draft.finishedColor}
                    onChange={(value) => patch("finishedColor", value)}
                  />
                </div>
              </div>

              <div className="row">
                <label className="row__label" htmlFor="settings-font">
                  Font
                </label>
                <div style={{ display: "grid", gridTemplateColumns: "1fr 84px", gap: 8 }}>
                  <div className="field">
                    <select
                      id="settings-font"
                      value={draft.fontFamily ?? FONTS[0]}
                      onChange={(event) => patch("fontFamily", event.target.value)}
                    >
                      {FONTS.map((font) => (
                        <option key={font} value={font}>
                          {font}
                        </option>
                      ))}
                    </select>
                    <span className="field__chevron" aria-hidden="true">
                      ▾
                    </span>
                  </div>
                  <div className="field stepper">
                    <input
                      type="number"
                      inputMode="numeric"
                      min={MIN_FONT_SIZE}
                      max={MAX_FONT_SIZE}
                      aria-label="Font size"
                      value={draft.fontSize}
                      onChange={(event) =>
                        patch("fontSize", clampFontSize(Number(event.target.value) || draft.fontSize))
                      }
                    />
                    <span className="stepper__arrows">
                      <button
                        type="button"
                        aria-label="Increase font size"
                        onClick={() => patch("fontSize", clampFontSize(draft.fontSize + 1))}
                      >
                        ▲
                      </button>
                      <button
                        type="button"
                        aria-label="Decrease font size"
                        onClick={() => patch("fontSize", clampFontSize(draft.fontSize - 1))}
                      >
                        ▼
                      </button>
                    </span>
                  </div>
                </div>
              </div>

              <div className="row">
                <div className="row__label">Notes folder</div>
                <div style={{ display: "grid", gridTemplateColumns: "1fr auto", gap: 8 }}>
                  <div className="field">
                    <span className="ellipsis" title={draft.notesRoot ?? ""}>
                      {draft.notesRoot ?? "Not set"}
                    </span>
                  </div>
                  <button
                    type="button"
                    className="btn btn--outline"
                    onClick={() => {
                      void actions.chooseNotesRoot().then((root) => {
                        if (root) patch("notesRoot", root);
                      });
                    }}
                  >
                    Browse…
                  </button>
                </div>
              </div>
            </div>

            {cloud.user && (
              <div className="settings-note">
                Signing out keeps everything on this computer. Team processes are hidden until you sign in again.
              </div>
            )}

            <div className="sheet__foot">
              <button
                type="button"
                className="btn btn--outline"
                onClick={() => actions.setSettingsOpen(false)}
              >
                Cancel
              </button>
              <button type="submit" className="btn btn--primary">
                Save
              </button>
            </div>
          </>
        )}
      </form>
    </div>
  );
}

function toDraft(config: Config, notesRoot: string | null): Draft {
  return {
    shell: config.shell,
    agentPatterns: config.agentPatterns,
    hotkeyModifier: config.hotkeyModifier,
    fontFamily: config.fontFamily ?? FONTS[0],
    fontSize: config.fontSize,
    notesRoot: config.notesRoot ?? notesRoot,
    theme: config.theme,
    compactLayout: config.compactLayout,
    focusColor: config.focusColor,
    finishedColor: config.finishedColor,
  };
}

/**
 * A colour swatch over the system colour picker. The swatch paints the live
 * token, so it shows the theme's colour until one is chosen and follows the
 * theme preview.
 */
function ColorPick({
  label,
  token,
  value,
  onChange,
}: {
  label: string;
  token: string;
  value: string | null;
  onChange: (value: string | null) => void;
}) {
  return (
    <div className="color-pick">
      <label className="color-pick__swatch" style={{ background: `rgb(var(${token}))` }}>
        <input
          type="color"
          className="color-pick__input"
          aria-label={label}
          value={value ?? channelsToHex(getComputedStyle(document.documentElement).getPropertyValue(token))}
          onChange={(event) => onChange(event.target.value)}
        />
      </label>
      <span className="color-pick__label">{label}</span>
      {value && (
        <button type="button" className="color-pick__reset" onClick={() => onChange(null)}>
          Reset
        </button>
      )}
    </div>
  );
}

/** "141 220 255" as #8ddcff, the format a colour input needs. */
function channelsToHex(channels: string): string {
  const parts = channels.trim().split(/\s+/).map(Number);
  if (parts.length !== 3 || parts.some((part) => !Number.isFinite(part))) return "#000000";
  return `#${parts.map((part) => Math.round(part).toString(16).padStart(2, "0")).join("")}`;
}

function clampFontSize(value: number): number {
  return Math.min(MAX_FONT_SIZE, Math.max(MIN_FONT_SIZE, Math.round(value)));
}
