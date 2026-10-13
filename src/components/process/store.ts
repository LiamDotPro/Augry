import { createContext, useContext, type Dispatch } from "react";
import type { Action, ProcessState } from "../../process/state";
import type { TeamApi } from "./useTeam";
import type { TemplatesApi } from "./useTemplates";

/** The Run button's tooltip on a team process while offline (team-offline). */
export const RUN_OFFLINE = "Can't run while offline";
/** The Run button's tooltip until there is a run engine. */
export const RUN_UNAVAILABLE = "Running processes isn’t available yet";
/** The same for Save. */
export const SAVE_OFFLINE = "Can't save while offline";

export interface ProcessStore {
  state: ProcessState;
  dispatch: Dispatch<Action>;
  /** The clock the time labels read, refreshed while the tab is mounted. */
  now: number;
  onOpenPane(paneNumber: number): void;
  /** The selected team (board 11), or null on Personal. */
  team: TeamApi | null;
  /** The server's templates, for the home. */
  templates: TemplatesApi;
  /** Saves a draft: a new version here, or the team's next version. */
  saveProcess(processId: string, message: string): void;
  /** Starts a run. Nothing runs yet: Run buttons ask runBlocked first. */
  runProcess(processId: string): void;
  /** Why a process can't run right now, for the Run button's tooltip; null when it can. */
  runBlocked(processId: string): string | null;
  restoreProcess(processId: string, version: number): void;
  /** A team process while offline: it can't run or save. */
  offline(processId: string): boolean;
}

export const ProcessContext = createContext<ProcessStore | null>(null);

/** The Process tab's state and dispatch, from ProcessView. */
export function useProcess(): ProcessStore {
  const store = useContext(ProcessContext);
  if (!store) throw new Error("useProcess outside ProcessView");
  return store;
}
