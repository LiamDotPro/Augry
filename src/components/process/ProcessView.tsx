import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { call } from "../../ipc/client";
import { errorMessage } from "../../lib/appError";
import { diskChanges, recordsFromDisk } from "../../process/disk";
import { initialState, reducer, teamOf } from "../../process/state";
import type { Process, ProcessRecord } from "../../process/types";
import { ConnectionsView } from "./ConnectionsView";
import { ImportSheet } from "./ImportSheet";
import { ProcessEditor } from "./ProcessEditor";
import { ProcessHome } from "./ProcessHome";
import { RunView } from "./RunView";
import { ProcessContext, RUN_UNAVAILABLE, type ProcessStore } from "./store";
import { TeamDialogs } from "./TeamDialogs";
import { ScrollMemory } from "./useKeptScroll";
import { useTeam } from "./useTeam";
import { useTemplates } from "./useTemplates";
import { VersionsView } from "./VersionsView";

/** How often the time labels ("8m", "12 min ago") refresh. */
const CLOCK_MS = 15000;
/** Drafts are written once typing pauses; saves are written straight away. */
const DRAFT_WRITE_MS = 400;

export interface ProcessViewProps {
  /** Jump to a pane in the Terminals tab (Open on a run row). 1-based pane number. */
  onOpenPane(paneNumber: number): void;
}

/**
 * The Process tab (Process Tab.dc.html). It opens on the Processes home; the
 * editor, Versions and Connections open from it. The shell keeps it mounted
 * while other tabs show and it needs no account. All of the tab's place
 * lives in the reducer here (plus scroll positions in a ref), so switching
 * tabs never resets it.
 *
 * Your processes are files on this computer: they are read once when the tab
 * mounts and every change to them is written back (useProcessFiles). With a
 * team selected in the account chip (board 11) the home shows the team's
 * processes from the cloud API (useTeam). Templates come from the server
 * (useTemplates). There is no run engine yet, so Run says so and nothing runs.
 */
export function ProcessView({ onOpenPane }: ProcessViewProps) {
  const [state, dispatch] = useReducer(reducer, undefined, initialState);
  const [now, setNow] = useState(() => Date.now());
  const [scrolls] = useState(() => new Map<string, { left: number; top: number }>());
  const team = useTeam(state, dispatch);
  const templates = useTemplates();
  const stateRef = useRef(state);
  stateRef.current = state;
  useProcessFiles(state.records, state.disk.status === "ready", dispatch);

  useEffect(() => {
    void call("processes_load", {})
      .then((stored) => {
        const loaded = recordsFromDisk(stored, Date.now());
        dispatch({ type: "loaded", records: loaded.records, problems: loaded.problems });
      })
      .catch((error) => dispatch({ type: "load-failed", error: errorMessage(error) }));
  }, []);

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), CLOCK_MS);
    return () => window.clearInterval(timer);
  }, []);

  // Team processes save and restore through the team; yours stay on this computer.
  const offline = useCallback((processId: string) => !!team && !team.online && teamOf(stateRef.current, processId) !== null, [team]);
  const saveProcess = useCallback(
    (processId: string, message: string) => {
      if (teamOf(stateRef.current, processId)) void team?.save(processId);
      else dispatch({ type: "save", processId, message, now: Date.now() });
    },
    [team],
  );
  // There is no run engine yet: every Run button is disabled with this reason.
  const runBlocked = useCallback(() => RUN_UNAVAILABLE, []);
  const runProcess = useCallback(() => {}, []);
  const restoreProcess = useCallback(
    (processId: string, version: number) => {
      if (teamOf(stateRef.current, processId)) void team?.restore(processId, version);
      else dispatch({ type: "restore", processId, version, now: Date.now() });
    },
    [team],
  );

  const store = useMemo<ProcessStore>(
    () => ({ state, dispatch, now, onOpenPane, team, templates, saveProcess, runProcess, runBlocked, restoreProcess, offline }),
    [state, now, onOpenPane, team, templates, saveProcess, runProcess, runBlocked, restoreProcess, offline],
  );
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
          {state.importOpen && <ImportSheet />}
          {state.teamDialog && <TeamDialogs />}
        </div>
      </ScrollMemory.Provider>
    </ProcessContext.Provider>
  );
}

/**
 * Writes your processes back to disk as they change: new versions and
 * deletes straight away, drafts once typing pauses. Writes run one at a time
 * in order, so a delete never overtakes the save before it. A failed write
 * says so on the home.
 */
function useProcessFiles(records: ProcessRecord[], ready: boolean, dispatch: ProcessStore["dispatch"]) {
  const written = useRef<ProcessRecord[] | null>(null);
  const queue = useRef<Promise<void>>(Promise.resolve());
  const drafts = useRef(new Map<string, number>());

  useEffect(() => {
    if (!ready) return;
    // The first records are the ones just read from disk.
    if (written.current === null) {
      written.current = records;
      return;
    }
    const changes = diskChanges(written.current, records);
    written.current = records;

    const write = (what: string, work: () => Promise<void>) => {
      queue.current = queue.current
        .then(work)
        .catch((error) => dispatch({ type: "team-note", note: `Couldn’t save ${what} to disk: ${errorMessage(error)}` }));
    };
    for (const { id, version } of changes.saves) write(`“${version.process.name}”`, () => call("process_save_version", { id, version }));
    for (const { id, draft } of changes.drafts) {
      window.clearTimeout(drafts.current.get(id));
      const writeDraft = () => write("your changes", () => call("process_save_draft", { id, draft: draft as Process | null }));
      // Clearing a draft (after Save or Discard) can't wait: it must not land after the next save.
      if (draft === null) writeDraft();
      else drafts.current.set(id, window.setTimeout(writeDraft, DRAFT_WRITE_MS));
    }
    for (const id of changes.deletes) {
      window.clearTimeout(drafts.current.get(id));
      write("the deletion", () => call("process_delete", { id }));
    }
  }, [records, ready, dispatch]);
}
