import { useEffect, useRef, useState } from "react";
import { Chrome } from "./components/Chrome";
import { EmptyState } from "./components/EmptyState";
import { HotkeyPopover } from "./components/HotkeyPopover";
import { NotesView } from "./components/notes/NotesView";
import { PaneGrid } from "./components/PaneGrid";
import { SettingsDialog } from "./components/SettingsDialog";
import { Splash } from "./components/Splash";
import { useAppearance } from "./lib/appearance";
import { cx } from "./lib/cx";
import { hotkeyScheme, IS_MAC, isModifierHeld, matchHotkey } from "./lib/hotkeys";
import { useWindowFrame } from "./lib/windowFrame";
import { AppProvider, useAppActions, useAppState } from "./state/AppProvider";

/** How long the chord modifier has to be held before the cheat sheet appears. */
const HINT_DELAY_MS = 700;

const MODIFIER_CODES = new Set([
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "ShiftLeft",
  "ShiftRight",
  "MetaLeft",
  "MetaRight",
]);

export default function App() {
  return (
    <AppProvider>
      <Shell />
    </AppProvider>
  );
}

function Shell() {
  const state = useAppState();
  const actions = useAppActions();
  useGlobalHotkeys();
  useModifierHint();
  // Settings previews its draft appearance while open; closing it (Save or
  // Cancel) hands back to the saved config.
  useAppearance(
    state.config.theme,
    state.config.compactLayout,
    { focus: state.config.focusColor, finished: state.config.finishedColor },
    !state.settingsOpen,
  );
  const frame = useWindowFrame();
  // The launch splash plays over the app, which stays hidden ("hold") until the
  // handoff lets the chrome in ("reveal").
  const [splash, setSplash] = useState<"hold" | "reveal" | null>("hold");

  const isTerminals = state.activeTab === "terminals";

  // macOS draws a native edge round the window; elsewhere the app draws one.
  return (
    <div className={cx("app", frame === "windowed" && !IS_MAC && "app--windowed")} data-splash={splash ?? undefined}>
      <Chrome frame={frame} />

      <div className="app__body">
        <div className={cx("view", isTerminals && "view--active")} aria-hidden={!isTerminals}>
          {state.order.length === 0 ? <EmptyState /> : <PaneGrid />}
        </div>
        <div className={cx("view", !isTerminals && "view--active")} aria-hidden={isTerminals}>
          <NotesView />
        </div>
      </div>

      {splash && (
        <Splash
          booted={state.booted}
          restoring={state.restoringPanes}
          onReveal={() => setSplash("reveal")}
          onDone={() => setSplash(null)}
        />
      )}

      {state.hotkeysOpen && <HotkeyPopover />}
      {state.settingsOpen && <SettingsDialog />}

      {state.error && (
        <div className="toast" role="status">
          <span>{state.error}</span>
          <button
            type="button"
            className="toast__dismiss"
            aria-label="Dismiss"
            onClick={actions.dismissError}
          >
            ×
          </button>
        </div>
      )}

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
    </div>
  );
}

/**
 * One window level handler in the capture phase, so a chord never reaches an
 * xterm instance or a text field first (design section 6).
 */
function useGlobalHotkeys(): void {
  const state = useAppState();
  const actions = useAppActions();

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // The settings sheet owns the keyboard while it is open.
      if (state.settingsOpen) return;

      const scheme = hotkeyScheme(state.config.hotkeyModifier);
      const action = matchHotkey(event, scheme);
      if (!action) return;
      // On macOS ⌘↩ also submits a comment or the review tray; inside one of
      // our text boxes it belongs to the box. The terminal's own hidden input
      // is not one of those.
      if (action.type === "toggle-review" && scheme === "mac" && isTextField(event.target)) return;
      // ⌘S saves in the review editor and the note editor; only a terminal's
      // selection is ours to save.
      if (action.type === "save-note" && isTextField(event.target)) return;
      // In the review panel plain Alt+↑ / Alt+↓ step through changed files.
      if (
        action.type === "move-focus" &&
        (action.dir === "up" || action.dir === "down") &&
        !event.ctrlKey &&
        !event.metaKey &&
        isInReview(event.target)
      ) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();

      switch (action.type) {
        case "new-pane":
          void actions.newTerminal();
          break;
        case "new-pane-here":
          void actions.newTerminalHere();
          break;
        case "close-pane":
          if (state.focusedId) void actions.closeTerminal(state.focusedId);
          break;
        case "restart-pane":
          if (state.focusedId) void actions.restartTerminal(state.focusedId);
          break;
        case "move-pane":
          actions.movePane(action.dir);
          break;
        case "move-focus":
          actions.moveFocus(action.dir);
          break;
        case "focus-index":
          actions.focusIndex(action.index);
          break;
        case "page":
          actions.stepPage(action.delta);
          break;
        case "toggle-tab":
          actions.toggleTab();
          break;
        case "set-tab":
          actions.setTab(action.tab);
          break;
        case "open-settings":
          actions.setSettingsOpen(true);
          break;
        case "cycle-layout":
          actions.cycleLayoutMode();
          break;
        case "notes-toggle":
          // Only meaningful on the notes tab; switch there so the key always does something visible.
          if (state.activeTab !== "notes") actions.setTab("notes");
          actions.toggleNotesPanel(action.panel);
          break;
        case "toggle-fullscreen":
          void actions.toggleFullscreen();
          break;
        case "toggle-review":
          actions.toggleReview();
          break;
        case "save-note":
          if (state.activeTab === "terminals" && state.focusedId) void actions.saveSelection(state.focusedId);
          break;
      }
    };

    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [actions, state.activeTab, state.config.hotkeyModifier, state.focusedId, state.settingsOpen]);
}

/** Holding the chord modifier reveals the hotkey popover, as screen 1d notes. */
function useModifierHint(): void {
  const state = useAppState();
  const actions = useAppActions();
  const timer = useRef<number | null>(null);
  const openedByHold = useRef(false);

  useEffect(() => {
    const clear = () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
      timer.current = null;
    };

    const onKeyDown = (event: KeyboardEvent) => {
      const scheme = hotkeyScheme(state.config.hotkeyModifier);
      if (!MODIFIER_CODES.has(event.code) || !isModifierHeld(event, scheme)) {
        clear();
        return;
      }
      if (timer.current !== null || state.hotkeysOpen) return;
      timer.current = window.setTimeout(() => {
        timer.current = null;
        openedByHold.current = true;
        actions.setHotkeysOpen(true);
      }, HINT_DELAY_MS);
    };

    const onKeyUp = (event: KeyboardEvent) => {
      if (!MODIFIER_CODES.has(event.code)) return;
      clear();
      if (openedByHold.current) {
        openedByHold.current = false;
        actions.setHotkeysOpen(false);
      }
    };

    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    window.addEventListener("blur", clear);
    return () => {
      clear();
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
      window.removeEventListener("blur", clear);
    };
  }, [actions, state.config.hotkeyModifier, state.hotkeysOpen]);
}

function isInReview(target: EventTarget | null): boolean {
  return Boolean((target as HTMLElement | null)?.closest?.(".review"));
}

function isTextField(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  return Boolean(element?.closest?.("textarea, input") && !element.closest(".xterm"));
}
