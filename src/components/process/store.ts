import { createContext, useContext, type Dispatch } from "react";
import type { Action, ProcessState } from "../../process/state";

export interface ProcessStore {
  state: ProcessState;
  dispatch: Dispatch<Action>;
  /** The clock the time labels read, refreshed while the tab is mounted. */
  now: number;
  onOpenPane(paneNumber: number): void;
}

export const ProcessContext = createContext<ProcessStore | null>(null);

/** The Process tab's state and dispatch, from ProcessView. */
export function useProcess(): ProcessStore {
  const store = useContext(ProcessContext);
  if (!store) throw new Error("useProcess outside ProcessView");
  return store;
}
