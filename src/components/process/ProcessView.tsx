import { useEffect, useMemo, useReducer, useState } from "react";
import { initialState, reducer } from "../../process/state";
import { ConnectionsView } from "./ConnectionsView";
import { FolderNotice } from "./FolderNotice";
import { ImportSheet } from "./ImportSheet";
import { ProcessEditor } from "./ProcessEditor";
import { ProcessHome } from "./ProcessHome";
import { RunSheet } from "./RunSheet";
import { RunView } from "./RunView";
import { ProcessContext } from "./store";
import { ScrollMemory } from "./useKeptScroll";
import { VersionsView } from "./VersionsView";

/** How often a simulated run moves on a step. */
const TICK_MS = 5000;
/** How often the time labels ("8m", "12 min ago") refresh. */
const CLOCK_MS = 15000;

export interface ProcessViewProps {
  /** Jump to a pane in the Terminals tab (Open on a run row). 1-based pane number. */
  onOpenPane(paneNumber: number): void;
}

/**
 * The Process tab (Process Tab.dc.html). It opens on the Processes home; a run
 * and the editor are separate views, and Connections and Versions open from
 * them. The shell keeps it mounted while other tabs show;
 * it needs no account. All of the tab's place lives in the reducer
 * here (plus scroll positions in a ref), so switching tabs never resets it.
 *
 * There is no backend for processes yet: the data is mocked in
 * src/process/mock.ts and runs are simulated in memory.
 */
export function ProcessView({ onOpenPane }: ProcessViewProps) {
  const [state, dispatch] = useReducer(reducer, undefined, () => initialState(Date.now()));
  const [now, setNow] = useState(() => Date.now());
  const [scrolls] = useState(() => new Map<string, { left: number; top: number }>());

  const live = state.runs.some((r) => r.status === "running" || r.status === "waiting");
  useEffect(() => {
    if (!live) return;
    const timer = window.setInterval(() => {
      if (document.hidden) return;
      dispatch({ type: "tick", now: Date.now() });
      setNow(Date.now());
    }, TICK_MS);
    return () => window.clearInterval(timer);
  }, [live]);

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), CLOCK_MS);
    return () => window.clearInterval(timer);
  }, []);

  const store = useMemo(() => ({ state, dispatch, now, onOpenPane }), [state, now, onOpenPane]);
  const route = state.route;

  return (
    <ProcessContext.Provider value={store}>
      <ScrollMemory.Provider value={scrolls}>
        <div className="process">
          {route.view === "home" && <ProcessHome />}
          {route.view === "run" && <RunView runId={route.runId} />}
          {route.view === "editor" && <ProcessEditor processId={route.processId} />}
          {route.view === "versions" && <VersionsView processId={route.processId} />}
          {route.view === "connections" && <ConnectionsView />}
          {state.sheet && <RunSheet templateId={state.sheet.templateId} />}
          {state.importOpen && <ImportSheet />}
          {state.notice && <FolderNotice />}
        </div>
      </ScrollMemory.Provider>
    </ProcessContext.Provider>
  );
}
