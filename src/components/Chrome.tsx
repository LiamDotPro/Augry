import { LAYOUT_MODES, pageSlice, type LayoutMode } from "../ipc/layout";
import { cx } from "../lib/cx";
import { focusPaneLabel, IS_MAC, shortcutLabel } from "../lib/hotkeys";
import type { AppTab } from "../ipc/types";
import type { WindowFrame } from "../lib/windowFrame";
import { useAppActions, useAppState, useHotkeyScheme } from "../state/AppProvider";
import { totalPages } from "../state/reducer";
import { AccountChip } from "./cloud/AccountChip";

const LAYOUT_LABELS: Record<LayoutMode, string> = {
  grid: "Grid: stack both ways",
  columns: "Side by side: stack horizontally",
  rows: "Stacked: stack vertically",
};

/**
 * The title bar version of the app icon (Components.dc.html use="topbar"):
 * three small overlapping panes, violet at the back, cyan in the middle and
 * clear glass in front, stepping down to the left. Flat colours, no glow.
 */
function BrandMark() {
  return (
    <svg
      className="chrome__mark"
      width="14"
      height="13"
      viewBox="0 0 14 13"
      aria-hidden="true"
      focusable="false"
    >
      <rect x="7" y="0" width="7" height="9" rx="2" fill="#8b6cff" />
      <rect x="3.5" y="2" width="7" height="9" rx="2" fill="#3fc8ff" />
      <rect x="0" y="4" width="7" height="9" rx="2" fill="#eaf8ff" />
    </svg>
  );
}

const TABS: { tab: AppTab; label: string }[] = [
  { tab: "process", label: "Process" },
  { tab: "terminals", label: "Terminals" },
  { tab: "notes", label: "Notes" },
];

/** Tiny pictograms of the three stacking modes. */
function LayoutGlyph({ mode }: { mode: LayoutMode }) {
  const rects =
    mode === "grid"
      ? [[1, 1, 5, 5], [8, 1, 5, 5], [1, 8, 5, 5], [8, 8, 5, 5]]
      : mode === "columns"
        ? [[1, 1, 5.5, 12], [7.5, 1, 5.5, 12]]
        : [[1, 1, 12, 5.5], [1, 7.5, 12, 5.5]];
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true" focusable="false">
      {rects.map(([x, y, w, h], index) => (
        <rect key={index} x={x} y={y} width={w} height={h} rx="1.2" fill="currentColor" />
      ))}
    </svg>
  );
}

/** Four corner brackets: pointing out to enter full screen, in to leave it. */
function FullscreenGlyph({ exit = false }: { exit?: boolean }) {
  const d = exit
    ? "M4.5 1v3.5H1M9.5 1v3.5H13M4.5 13V9.5H1M9.5 13V9.5H13"
    : "M1 4.5V1h3.5M13 4.5V1H9.5M1 9.5V13h3.5M13 9.5V13H9.5";
  return (
    <svg width="12" height="12" viewBox="0 0 14 14" aria-hidden="true" focusable="false">
      <path d={d} fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

/**
 * A window with a title bar, so it reads as "window" next to the full screen
 * brackets; restoring shows a second window stacked behind it.
 */
function MaximizeGlyph({ restore = false }: { restore?: boolean }) {
  return (
    <svg width="12" height="12" viewBox="0 0 14 14" aria-hidden="true" focusable="false">
      <g fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
        {restore ? (
          <>
            <path d="M4.75 3.75V3A1.25 1.25 0 0 1 6 1.75h5A1.25 1.25 0 0 1 12.25 3v5A1.25 1.25 0 0 1 11 9.25h-.75" />
            <rect x="1.75" y="4.75" width="7.5" height="7.5" rx="1.25" />
            <path d="M1.75 7.5h7.5" />
          </>
        ) : (
          <>
            <rect x="1.75" y="1.75" width="10.5" height="10.5" rx="1.5" />
            <path d="M1.75 5h10.5" />
          </>
        )}
      </g>
    </svg>
  );
}

/**
 * The window bar: brand, tab switcher, pager, pane/hotkey/settings actions,
 * the account chip (signed in only) and the window controls. It is also the
 * drag region, so the app can run without native decorations the way the
 * design draws it.
 *
 * On macOS the window keeps its native frame with the title bar overlaid
 * (tauri.macos.conf.json), so the traffic lights sit in this bar the way they
 * do in WebStorm: the bar makes room for them and drops its own minimize,
 * maximize and close. Full screen keeps them in the bar too, WebStorm style
 * (src-tauri/src/traffic_lights.rs), with the green one leaving full screen.
 */
export function Chrome({ frame }: { frame: WindowFrame }) {
  const state = useAppState();
  const actions = useAppActions();
  const scheme = useHotkeyScheme();
  const pages = totalPages(state);
  // Settings is a page in place of the tab: no tab is selected under it.
  const settings = state.settingsOpen;
  const isTerminals = state.activeTab === "terminals" && !settings;
  const inFocus = isTerminals && state.focusMode;
  const showPager = isTerminals && pages > 1 && !inFocus;
  const fullscreen = frame === "fullscreen";
  // Nothing to drag while fullscreen, and a double-click there would maximize
  // the window underneath without anything visibly changing.
  const dragRegion = fullscreen ? undefined : "";
  const trafficLights = IS_MAC;

  return (
    <header className={cx("chrome", trafficLights && "chrome--traffic-lights")} data-tauri-drag-region={dragRegion}>
      <div className="chrome__brand" data-tauri-drag-region={dragRegion}>
        <BrandMark />
        {inFocus ? <FocusPills /> : <span className="chrome__title">Augry</span>}
      </div>

      <div className="chrome__center" data-tauri-drag-region={dragRegion}>
        <div className="tabs" role="tablist" aria-label="Sections">
          {TABS.map(({ tab, label }) => (
            <button
              key={tab}
              type="button"
              role="tab"
              aria-selected={!settings && state.activeTab === tab}
              className={cx("tabs__item", !settings && state.activeTab === tab && "tabs__item--active")}
              onClick={() => actions.setTab(tab)}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      <div className="chrome__right">
        {showPager && (
          <div className="pager">
            <button
              type="button"
              className="pager__step"
              aria-label="Previous page"
              disabled={state.page === 0}
              onClick={() => actions.stepPage(-1)}
            >
              ‹
            </button>
            <span className="pager__label">
              {state.page + 1} / {pages}
            </span>
            <button
              type="button"
              className="pager__step"
              aria-label="Next page"
              disabled={state.page >= pages - 1}
              onClick={() => actions.stepPage(1)}
            >
              ›
            </button>
          </div>
        )}

        {isTerminals && state.order.length > 0 && !inFocus && (
          <div
            className="layout-switch"
            role="radiogroup"
            aria-label="Pane stacking"
            title={`Pane stacking (${shortcutLabel(scheme, "cycle-layout")} cycles)`}
          >
            {LAYOUT_MODES.map((mode) => (
              <button
                key={mode}
                type="button"
                role="radio"
                aria-checked={state.layoutMode === mode}
                aria-label={LAYOUT_LABELS[mode]}
                title={LAYOUT_LABELS[mode]}
                className={cx("layout-switch__item", state.layoutMode === mode && "layout-switch__item--active")}
                onClick={() => actions.setLayoutMode(mode)}
              >
                <LayoutGlyph mode={mode} />
              </button>
            ))}
          </div>
        )}

        {isTerminals && (
          <button
            type="button"
            className="chrome__new"
            title="New pane"
            onClick={() => void actions.newTerminal()}
          >
            <span className="chrome__new-glyph" aria-hidden="true">
              +
            </span>
            Pane
          </button>
        )}

        <button
          type="button"
          className={cx("btn", "btn--quiet", "chrome__help", state.hotkeysOpen && "chrome__help--active")}
          data-hotkeys-toggle
          title="Hotkeys"
          aria-expanded={state.hotkeysOpen}
          onClick={() => actions.setHotkeysOpen(!state.hotkeysOpen)}
        >
          ?
        </button>

        <button
          type="button"
          className={cx("btn", "btn--quiet", "chrome__settings", settings && "chrome__settings--active")}
          title={`Settings (${shortcutLabel(scheme, "settings")})`}
          aria-pressed={settings}
          onClick={() => actions.setSettingsOpen(true)}
        >
          Settings
        </button>

        <AccountChip />

        {!trafficLights && <span className="chrome__divider" aria-hidden="true" />}

        {!trafficLights && (
          <div className="winbtns">
            <button
              type="button"
              className="winbtn"
              aria-label="Minimize"
              onClick={() => void actions.minimize()}
            >
              —
            </button>
            {fullscreen ? (
              <button
                type="button"
                className="winbtn winbtn--fullscreen"
                title="Exit full screen (F11)"
                onClick={() => void actions.toggleFullscreen()}
              >
                <FullscreenGlyph exit />
                Full screen
              </button>
            ) : (
              <>
                <button
                  type="button"
                  className="winbtn"
                  aria-label="Full screen"
                  title="Full screen (F11)"
                  onClick={() => void actions.toggleFullscreen()}
                >
                  <FullscreenGlyph />
                </button>
                <button
                  type="button"
                  className="winbtn"
                  aria-label={frame === "maximized" ? "Restore" : "Maximize"}
                  title={frame === "maximized" ? "Restore" : "Maximize"}
                  onClick={() => void actions.toggleMaximize()}
                >
                  <MaximizeGlyph restore={frame === "maximized"} />
                </button>
              </>
            )}
            <button
              type="button"
              className="winbtn winbtn--close"
              aria-label="Close"
              onClick={() => void actions.closeWindow()}
            >
              ×
            </button>
          </div>
        )}
      </div>
    </header>
  );
}

/**
 * Focus mode's left of the bar (design screen 2a): back to the grid, and the
 * page's other panes as numbered pills; a green dot marks one that finished.
 */
function FocusPills() {
  const state = useAppState();
  const actions = useAppActions();
  const scheme = useHotkeyScheme();
  const ids = pageSlice(state.order, state.page);

  return (
    <>
      <button
        type="button"
        className="chrome__back"
        title={`Back to grid (${shortcutLabel(scheme, "toggle-review")})`}
        onClick={actions.closeReview}
      >
        <span className="chrome__back-glyph" aria-hidden="true">
          ‹
        </span>
        Grid
      </button>
      <div className="chrome__pills" role="tablist" aria-label="Panes">
        {ids.map((id, index) => {
          const current = id === state.focusedId;
          const finished = Boolean(state.terminals[id]?.finished);
          return (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={current}
              className={cx("chrome__pill", current && "chrome__pill--current")}
              title={`Review pane ${index + 1}${finished ? " · finished" : ""} (${focusPaneLabel(scheme, index + 1)})`}
              onClick={() => actions.openReview(id)}
            >
              {index + 1}
              {finished && <span className="chrome__pill-dot" aria-hidden="true" />}
            </button>
          );
        })}
      </div>
    </>
  );
}
