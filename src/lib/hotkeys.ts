/**
 * Keyboard shortcuts. The bindings are the ones the design's hotkey popover
 * advertises (Terminal Grid.dc.html, screen 1d), plus the aliases from
 * docs/technical-design.md section 6 that do not collide with them.
 *
 * There are two layouts. Windows and Linux chord everything on a modifier
 * pair (Ctrl+Alt, or Ctrl+Shift where AltGr gets in the way). macOS uses the
 * Command key the way Mac apps do: ⌘N, ⌘W, ⌘1…9, ⌘, and so on. The shell
 * never sees ⌘, so none of these take a key away from the terminal.
 *
 * Matching is done on `KeyboardEvent.code`, never on `key`: with Ctrl+Alt held
 * Windows treats the combination as AltGr on Dutch and US-international
 * layouts, and on macOS Option turns letters into symbols, so `key` is
 * unreliable for letters, digits and brackets.
 */
import type { AppTab, HotkeyModifier } from "../ipc/types";
import type { Direction } from "../ipc/layout";

export type HotkeyAction =
  | { type: "new-pane" }
  | { type: "new-pane-here" }
  | { type: "close-pane" }
  | { type: "restart-pane" }
  | { type: "move-pane"; dir: Direction }
  | { type: "move-focus"; dir: Direction }
  | { type: "focus-index"; index: number }
  | { type: "page"; delta: -1 | 1 }
  | { type: "toggle-tab" }
  | { type: "set-tab"; tab: AppTab }
  | { type: "open-settings" }
  | { type: "cycle-layout" }
  | { type: "notes-toggle"; panel: "tree" | "preview" }
  | { type: "toggle-fullscreen" }
  | { type: "toggle-review" }
  /** Saves the focused terminal's selection as a session note (design turn 7). */
  | { type: "save-note" };

/** The configured modifier pair on Windows and Linux, or the Command layout on macOS. */
export type HotkeyScheme = HotkeyModifier | "mac";

const ARROWS: Record<string, Direction> = {
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
};

export interface HotkeyEventLike {
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  repeat?: boolean;
}

/** True in the macOS build (WKWebView reports a Mac platform). */
export const IS_MAC: boolean =
  typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

/**
 * The layout in force: macOS always uses Command, elsewhere the modifier from
 * Settings applies.
 */
export function hotkeyScheme(modifier: HotkeyModifier, mac: boolean = IS_MAC): HotkeyScheme {
  return mac ? "mac" : modifier;
}

/** Human readable form of the chord modifier, as the popover's header shows it. */
export function modifierLabel(scheme: HotkeyScheme): string {
  if (scheme === "mac") return "⌘";
  return scheme === "ctrl+shift" ? "Ctrl+Shift" : "Ctrl+Alt";
}

/**
 * True while the chord modifier itself is held down. The chrome uses this to
 * reveal the hotkey popover without a click.
 */
export function isModifierHeld(event: HotkeyEventLike, scheme: HotkeyScheme): boolean {
  if (scheme === "mac") return event.metaKey && !event.ctrlKey && !event.altKey;
  return scheme === "ctrl+shift"
    ? event.ctrlKey && event.shiftKey && !event.altKey && !event.metaKey
    : event.ctrlKey && event.altKey && !event.metaKey;
}

/** Resolves a key event to an action, or null when it is not one of ours. */
export function matchHotkey(event: HotkeyEventLike, scheme: HotkeyScheme): HotkeyAction | null {
  return scheme === "mac" ? matchMac(event) : matchChord(event, scheme);
}

/**
 * Windows and Linux. The "secondary" modifier (Shift for Ctrl+Alt, Alt for
 * Ctrl+Shift) turns a pane move into a focus move and a new pane into a new
 * pane in the same cwd.
 */
function matchChord(event: HotkeyEventLike, modifier: HotkeyModifier): HotkeyAction | null {
  if (event.code === "F11" && !event.ctrlKey && !event.altKey && !event.metaKey) {
    return { type: "toggle-fullscreen" };
  }
  // Plain Alt+Arrow moves focus to the neighbouring pane in that direction,
  // whichever chord modifier is configured. Shells do not use the combination.
  if (event.altKey && !event.ctrlKey && !event.shiftKey && !event.metaKey) {
    const arrow = ARROWS[event.code];
    if (arrow) return { type: "move-focus", dir: arrow };
    return null;
  }

  if (!event.ctrlKey || event.metaKey) return null;
  if (modifier === "ctrl+shift" ? !event.shiftKey : !event.altKey) return null;

  const secondary = modifier === "ctrl+shift" ? event.altKey : event.shiftKey;

  const arrow = ARROWS[event.code];
  if (arrow) return secondary ? { type: "move-focus", dir: arrow } : { type: "move-pane", dir: arrow };

  const digit = /^Digit([1-9])$/.exec(event.code);
  if (digit) return { type: "focus-index", index: Number(digit[1]) - 1 };

  switch (event.code) {
    case "KeyN":
      return secondary ? { type: "new-pane-here" } : { type: "new-pane" };
    case "KeyW":
      return { type: "close-pane" };
    case "KeyR":
      return { type: "restart-pane" };
    case "BracketLeft":
    case "PageUp":
      return { type: "page", delta: -1 };
    case "BracketRight":
    case "PageDown":
      return { type: "page", delta: 1 };
    case "Tab":
      return { type: "toggle-tab" };
    case "KeyT":
      return { type: "set-tab", tab: "terminals" };
    case "KeyM":
      return { type: "set-tab", tab: "notes" };
    case "KeyL":
      return { type: "cycle-layout" };
    case "KeyB":
      return { type: "notes-toggle", panel: "tree" };
    case "KeyP":
      return { type: "notes-toggle", panel: "preview" };
    case "KeyS":
      return { type: "save-note" };
    case "Comma":
      return { type: "open-settings" };
    case "Enter":
    case "NumpadEnter":
      return { type: "toggle-review" };
    default:
      return null;
  }
}

/**
 * macOS, after the conventions of Terminal, iTerm and Safari: ⌘N and ⌘T open
 * panes, ⌘W closes one, ⌘1…9 picks one, ⌥⌘ arrows walk between them (iTerm's
 * split navigation), ⌃Tab switches the Terminals and Notes tabs, ⌃⌘F is full
 * screen. Option alone is left to the shell, where it moves by word.
 */
function matchMac(event: HotkeyEventLike): HotkeyAction | null {
  if (event.code === "Tab" && event.ctrlKey && !event.metaKey && !event.altKey) {
    return { type: "toggle-tab" };
  }
  if (!event.metaKey) return null;

  const arrow = ARROWS[event.code];
  if (arrow) {
    if (event.shiftKey) return null;
    if (event.altKey && !event.ctrlKey) return { type: "move-focus", dir: arrow };
    if (event.ctrlKey && !event.altKey) return { type: "move-pane", dir: arrow };
    return null;
  }

  if (event.ctrlKey) {
    return event.code === "KeyF" && !event.altKey && !event.shiftKey ? { type: "toggle-fullscreen" } : null;
  }
  if (event.altKey) return null;

  // ⇧⌘[ and ⇧⌘] are the Mac way to step through tabs; ⌘[ and ⌘] work too.
  if (event.code === "BracketLeft") return { type: "page", delta: -1 };
  if (event.code === "BracketRight") return { type: "page", delta: 1 };
  if (event.shiftKey) return null;

  const digit = /^Digit([1-9])$/.exec(event.code);
  if (digit) return { type: "focus-index", index: Number(digit[1]) - 1 };

  switch (event.code) {
    case "KeyN":
      return { type: "new-pane" };
    case "KeyT":
      return { type: "new-pane-here" };
    case "KeyW":
      return { type: "close-pane" };
    case "KeyR":
      return { type: "restart-pane" };
    case "KeyL":
      return { type: "cycle-layout" };
    case "KeyB":
      return { type: "notes-toggle", panel: "tree" };
    case "KeyP":
      return { type: "notes-toggle", panel: "preview" };
    case "KeyS":
      return { type: "save-note" };
    case "Comma":
      return { type: "open-settings" };
    case "Enter":
    case "NumpadEnter":
      return { type: "toggle-review" };
    default:
      return null;
  }
}

// Labels -------------------------------------------------------------------------

/** Every shortcut the UI names somewhere, not only the global ones. */
export type ShortcutName =
  | "new-pane"
  | "new-pane-here"
  | "cycle-layout"
  | "close-pane"
  | "restart-pane"
  | "move-pane"
  | "move-focus"
  | "focus-index"
  | "page"
  | "toggle-review"
  | "toggle-tab"
  | "notes-tree"
  | "notes-preview"
  | "notes-panels"
  | "settings"
  | "fullscreen"
  /** Submitting a comment or the review tray. */
  | "submit"
  /** Saving in the review editor. */
  | "save"
  /** Saving the terminal selection as a note. */
  | "save-note"
  /** Copying the terminal selection. */
  | "copy";

const MAC_LABELS: Record<ShortcutName, string> = {
  "new-pane": "⌘N",
  "new-pane-here": "⌘T",
  "cycle-layout": "⌘L",
  "close-pane": "⌘W",
  "restart-pane": "⌘R",
  "move-pane": "⌃⌘←↑↓→",
  "move-focus": "⌥⌘←↑↓→",
  "focus-index": "⌘1…9",
  page: "⇧⌘[ ]",
  "toggle-review": "⌘↩",
  "toggle-tab": "⌃Tab",
  "notes-tree": "⌘B",
  "notes-preview": "⌘P",
  "notes-panels": "⌘B / ⌘P",
  settings: "⌘,",
  fullscreen: "⌃⌘F",
  submit: "⌘↩",
  save: "⌘S",
  "save-note": "⌘S",
  copy: "⌘C",
};

function chordLabels(modifier: HotkeyModifier): Record<ShortcutName, string> {
  const mod = modifierLabel(modifier);
  const secondary = modifier === "ctrl+shift" ? "Alt" : "Shift";
  return {
    "new-pane": `${mod}+N`,
    "new-pane-here": `${mod}+${secondary}+N`,
    "cycle-layout": `${mod}+L`,
    "close-pane": `${mod}+W`,
    "restart-pane": `${mod}+R`,
    "move-pane": `${mod}+←↑↓→`,
    "move-focus": "Alt+←↑↓→",
    "focus-index": `${mod}+1…9`,
    page: `${mod}+[ ]`,
    "toggle-review": `${mod}+Enter`,
    "toggle-tab": `${mod}+Tab`,
    "notes-tree": `${mod}+B`,
    "notes-preview": `${mod}+P`,
    "notes-panels": `${mod}+B / P`,
    settings: `${mod}+,`,
    fullscreen: "F11",
    submit: "Ctrl+Enter",
    save: "Ctrl+S",
    "save-note": `${mod}+S`,
    copy: "Ctrl+C",
  };
}

/** How a shortcut is written in this layout: "⌘W" on macOS, "Ctrl+Alt+W" elsewhere. */
export function shortcutLabel(scheme: HotkeyScheme, name: ShortcutName): string {
  return scheme === "mac" ? MAC_LABELS[name] : chordLabels(scheme)[name];
}

/** The key that focuses pane `n` (1-based): "⌘3" or "Ctrl+Alt+3". */
export function focusPaneLabel(scheme: HotkeyScheme, n: number): string {
  return scheme === "mac" ? `⌘${n}` : `${modifierLabel(scheme)}+${n}`;
}

export interface HotkeyHint {
  label: string;
  key: string;
}

/** The list rendered by the hotkey popover, in the design's order. */
export function hotkeyHints(scheme: HotkeyScheme): HotkeyHint[] {
  const key = (name: ShortcutName) => shortcutLabel(scheme, name);
  return [
    { label: "New pane", key: key("new-pane") },
    { label: "New pane in same folder", key: key("new-pane-here") },
    { label: "Stack: grid / side by side / stacked", key: key("cycle-layout") },
    { label: "Close pane", key: key("close-pane") },
    { label: "Restart shell", key: key("restart-pane") },
    { label: "Move pane", key: key("move-pane") },
    { label: "Focus pane in a direction", key: key("move-focus") },
    { label: "Focus pane 1–9", key: key("focus-index") },
    { label: "Previous / next page", key: key("page") },
    { label: "Focus + review", key: key("toggle-review") },
    { label: "Save selection to note", key: key("save-note") },
    { label: "Next tab: Process, Terminals, Notes", key: key("toggle-tab") },
    { label: "Notes: hide / show list, preview", key: key("notes-panels") },
    { label: "Settings", key: key("settings") },
    { label: "Full screen", key: key("fullscreen") },
  ];
}
