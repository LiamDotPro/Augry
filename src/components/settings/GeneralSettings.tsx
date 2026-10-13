import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import type { Config, HotkeyModifier, ThemePreference } from "../../ipc/types";
import { cx } from "../../lib/cx";
import { IS_MAC } from "../../lib/hotkeys";
import { useAppActions, useAppState } from "../../state/AppProvider";
import { SettingsHeading } from "./SettingsPage";

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
/** A colour picker fires on every drag step; save once it settles. */
const COLOR_SAVE_MS = 250;

/**
 * Settings → General (App Shell board 03, `settings`). Every control saves
 * through `config_set` as it changes; there is no draft and no Save.
 */
export function GeneralSettings() {
  const state = useAppState();
  const actions = useAppActions();
  const config = state.config;
  const [pattern, setPattern] = useState("");

  const save = (patch: Partial<Config>) => void actions.saveConfig(patch);

  const addPattern = () => {
    const value = pattern.trim();
    setPattern("");
    if (value && !config.agentPatterns.includes(value)) save({ agentPatterns: [...config.agentPatterns, value] });
  };

  const onPatternKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter" || event.key === ",") {
      event.preventDefault();
      addPattern();
    } else if (event.key === "Backspace" && pattern === "" && config.agentPatterns.length > 0) {
      save({ agentPatterns: config.agentPatterns.slice(0, -1) });
    }
  };

  const chooseFolder = async () => {
    const root = await actions.chooseNotesRoot();
    if (!root) return;
    await actions.saveConfig({ notesRoot: root });
    // Also a service level setting: switching creates the folder and reloads the tree.
    await actions.setNotesRoot(root);
  };

  return (
    <>
      <SettingsHeading title="General">Applies to this computer.</SettingsHeading>

      <div className="settings-rows">
        <Row label="Shell" hint="Used for new panes" htmlFor="settings-shell">
          <div className="field settings-control settings-control--md">
            <select
              id="settings-shell"
              value={config.shell ?? ""}
              onChange={(event) => save({ shell: event.target.value || null })}
            >
              {SHELLS.map((shell) => (
                <option key={shell.value} value={shell.value}>
                  {shell.label}
                </option>
              ))}
              {/* A shell set in config.json, or on another platform, still shows. */}
              {config.shell && !SHELLS.some((shell) => shell.value === config.shell) && (
                <option value={config.shell}>{config.shell}</option>
              )}
            </select>
            <span className="field__chevron" aria-hidden="true">
              ▾
            </span>
          </div>
        </Row>

        <Row label="Agent name patterns" hint="Process names that mark a pane as an agent" top>
          <div className="tokens settings-control settings-control--lg">
            {config.agentPatterns.map((value) => (
              <span className="token" key={value}>
                {value}
                <button
                  type="button"
                  className="token__remove"
                  aria-label={`Remove ${value}`}
                  onClick={() => save({ agentPatterns: config.agentPatterns.filter((item) => item !== value) })}
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
              onBlur={addPattern}
            />
          </div>
        </Row>

        {IS_MAC ? (
          <Row label="Hotkeys" hint="Used by every shortcut">
            <div className="row__note">
              Command (⌘), the macOS layout: ⌘N, ⌘W, ⌘1…9 and so on. Hold ⌘ to see them all.
            </div>
          </Row>
        ) : (
          <Row label="Hotkey modifier" hint="Used by every shortcut" labelId="settings-modifier">
            <div className="segmented settings-segmented" role="radiogroup" aria-labelledby="settings-modifier">
              {(["ctrl+alt", "ctrl+shift"] as HotkeyModifier[]).map((value) => (
                <button
                  key={value}
                  type="button"
                  role="radio"
                  aria-checked={config.hotkeyModifier === value}
                  className={cx("segmented__item", config.hotkeyModifier === value && "segmented__item--active")}
                  onClick={() => save({ hotkeyModifier: value })}
                >
                  {value === "ctrl+alt" ? "Ctrl+Alt" : "Ctrl+Shift"}
                </button>
              ))}
            </div>
          </Row>
        )}

        <Row
          label="Theme"
          hint={`System follows your OS; Black is total darkness${IS_MAC ? "; Glass shows your desktop through the window (Dark in full screen)" : ""}`}
          labelId="settings-theme"
        >
          <div className="segmented settings-segmented" role="radiogroup" aria-labelledby="settings-theme">
            {THEMES.map(({ value, label }) => (
              <button
                key={value}
                type="button"
                role="radio"
                aria-checked={config.theme === value}
                className={cx("segmented__item", config.theme === value && "segmented__item--active")}
                onClick={() => save({ theme: value })}
              >
                {label}
              </button>
            ))}
          </div>
        </Row>

        <Row label="Compact layout" hint="Square panes, no borders, minimal gaps" labelId="settings-compact">
          <div className="segmented settings-segmented" role="radiogroup" aria-labelledby="settings-compact">
            {[false, true].map((value) => (
              <button
                key={String(value)}
                type="button"
                role="radio"
                aria-checked={config.compactLayout === value}
                className={cx("segmented__item", config.compactLayout === value && "segmented__item--active")}
                onClick={() => save({ compactLayout: value })}
              >
                {value ? "On" : "Off"}
              </button>
            ))}
          </div>
        </Row>

        <Row label="Colours" hint="Reset follows the theme">
          <div className="color-picks">
            <ColorPick
              label="Focus ring"
              token="--focus-rgb"
              value={config.focusColor}
              onChange={(value) => save({ focusColor: value })}
            />
            <ColorPick
              label="Agent finished"
              token="--finished-rgb"
              value={config.finishedColor}
              onChange={(value) => save({ finishedColor: value })}
            />
          </div>
        </Row>

        <Row label="Terminal font" htmlFor="settings-font">
          <div className="settings-control settings-control--md settings-font">
            <div className="field">
              <select
                id="settings-font"
                value={config.fontFamily ?? FONTS[0]}
                onChange={(event) => save({ fontFamily: event.target.value })}
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
            <FontSize value={config.fontSize} onChange={(fontSize) => save({ fontSize })} />
          </div>
        </Row>

        <Row label="Notes folder" hint="Where Notes reads and writes .md files">
          <div className="settings-control settings-control--lg settings-folder">
            <div className="field">
              <span className="ellipsis" title={config.notesRoot ?? state.notes.root ?? ""}>
                {config.notesRoot ?? state.notes.root ?? "Not set"}
              </span>
            </div>
            <button type="button" className="btn btn--outline settings-button" onClick={() => void chooseFolder()}>
              Browse…
            </button>
          </div>
        </Row>
      </div>
    </>
  );
}

function Row({
  label,
  hint,
  htmlFor,
  labelId,
  top = false,
  children,
}: {
  label: string;
  hint?: string;
  htmlFor?: string;
  labelId?: string;
  top?: boolean;
  children: ReactNode;
}) {
  const text = (
    <>
      {label}
      {hint && <div className="settings-row__hint">{hint}</div>}
    </>
  );
  return (
    <div className={cx("settings-row", top && "settings-row--top")}>
      {htmlFor ? (
        <label className="settings-row__label" htmlFor={htmlFor}>
          {text}
        </label>
      ) : (
        <div className="settings-row__label" id={labelId}>
          {text}
        </div>
      )}
      <div className="settings-row__control">{children}</div>
    </div>
  );
}

/** The size stepper. Typing keeps a local value, so a half-typed number isn't saved. */
function FontSize({ value, onChange }: { value: number; onChange(size: number): void }) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const commit = (next: number) => {
    const size = clampFontSize(next);
    setText(String(size));
    if (size !== value) onChange(size);
  };
  return (
    <div className="field stepper">
      <input
        type="number"
        inputMode="numeric"
        min={MIN_FONT_SIZE}
        max={MAX_FONT_SIZE}
        aria-label="Font size"
        value={text}
        onChange={(event) => setText(event.target.value)}
        onBlur={() => commit(Number(text) || value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") commit(Number(text) || value);
        }}
      />
      <span className="stepper__arrows">
        <button type="button" aria-label="Increase font size" onClick={() => commit(value + 1)}>
          ▲
        </button>
        <button type="button" aria-label="Decrease font size" onClick={() => commit(value - 1)}>
          ▼
        </button>
      </span>
    </div>
  );
}

/**
 * A colour swatch over the system colour picker. The swatch paints the live
 * token, so it shows the theme's colour until one is chosen. Dragging saves
 * once the colour settles.
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
  const timer = useRef(0);
  const [picking, setPicking] = useState<string | null>(null);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const shown = picking ?? value;

  return (
    <div className="color-pick">
      <label className="color-pick__swatch" style={{ background: shown ?? `rgb(var(${token}))` }}>
        <input
          type="color"
          className="color-pick__input"
          aria-label={label}
          value={shown ?? channelsToHex(getComputedStyle(document.documentElement).getPropertyValue(token))}
          onChange={(event) => {
            const next = event.target.value;
            setPicking(next);
            window.clearTimeout(timer.current);
            timer.current = window.setTimeout(() => {
              setPicking(null);
              onChange(next);
            }, COLOR_SAVE_MS);
          }}
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
