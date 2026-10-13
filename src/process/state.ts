/**
 * Everything the Process tab holds, as one reducer. ProcessView keeps it in
 * React state for as long as the app runs, so switching to Terminals or Notes
 * and back never loses the view, the open process or run, the selection, the
 * Lanes / Runbook choice or a draft. Your processes are files on this
 * computer (disk.ts, src-tauri/src/processes): ProcessView loads them into
 * `records` and writes back whatever changes. Team processes come from the
 * cloud API (team.ts). There is no run engine yet, so nothing here starts a
 * run; the run views stay for when there is.
 */

import { CONNECTIONS } from "./connections";
import type { DiskProblem } from "./disk";
import { blankProcess } from "./examples";
import { cloneProcess, fitName, hashOf } from "./json";
import { uniqueId } from "./model";
import { approveRun, decideNoRun, sendBackRun, skipRun, stopRun } from "./runs";
import type { ProcessPage, TeamProcess } from "../cloud/types";
import { applyPage, dropProcess, putProcess, syncTeamRecords, teamVersion, withVersion, EMPTY_TEAM_CACHE, type TeamCache } from "./team";
import type { Connection, Process, ProcessRecord, ProcessVersion, Run } from "./types";
import { latest, restoreVersion, saveVersion, withDraft, working } from "./versions";

export type Route =
  | { view: "home" }
  | { view: "run"; runId: string }
  | { view: "editor"; processId: string }
  | { view: "versions"; processId: string }
  | { view: "connections" };

export type Selection = { kind: "step"; id: string } | { kind: "start" } | { kind: "stage"; id: string } | null;

export interface EditorUi {
  selection: Selection;
  layout: "lanes" | "runbook";
  json: boolean;
}

export interface VersionsUi {
  /** The older side of the compare. */
  compare: number | null;
  mode: "json" | "lanes";
  /** Lanes compare: which side is drawn. */
  side: "from" | "to";
  message: string;
}

/** A team's processes: the server's cache and the records the views draw from it. */
export interface TeamSpace {
  cache: TeamCache;
  records: ProcessRecord[];
}

/** The team dialogs over the tab (board 11): share-to-team, share-blocked, team-conflict. */
export type TeamDialog =
  | { kind: "share"; processId: string }
  | { kind: "share-blocked"; processId: string; rule: string; path: string; masked: string | null }
  | {
      kind: "conflict";
      processId: string;
      teamId: string;
      /** The newer version the 409 carried. */
      theirs: TeamProcess;
      /** The version the draft started from. */
      base: { version: number; byId?: string; at: number };
    };

export interface ProcessState {
  route: Route;
  /** What the home shows: "personal", or a team's processes by team id. */
  space: string;
  /** Team processes by team id, as far as they have loaded. */
  teams: Record<string, TeamSpace>;
  teamDialog: TeamDialog | null;
  /** A one-line note about a team call that didn't work. */
  teamNote: string | null;
  /** Your processes, once read from disk. */
  records: ProcessRecord[];
  /** Reading your processes folder: until it's read the home shows neither cards nor the empty state. */
  disk: { status: "loading" | "ready" | "failed"; problems: DiskProblem[]; error?: string };
  /** Newest first. */
  runs: Run[];
  connections: Connection[];
  editor: Record<string, EditorUi>;
  versions: Record<string, VersionsUi>;
  /** The connection open in the side panel. */
  connection: string | null;
  importOpen: boolean;
}

export type Action =
  | { type: "navigate"; route: Route }
  | { type: "loaded"; records: ProcessRecord[]; problems: DiskProblem[] }
  | { type: "load-failed"; error: string }
  /** A template from the server, already converted, saved as a process of yours. */
  | { type: "use-template"; process: Process; name: string; now: number }
  | { type: "approve"; runId: string; stepId: string; now: number }
  | { type: "send-back"; runId: string; stepId: string; now: number }
  | { type: "skip"; runId: string; stepId: string; now: number }
  | { type: "decide-no"; runId: string; stepId: string; now: number }
  | { type: "stop"; runId: string; now: number }
  | { type: "edit"; processId: string; process: Process }
  | { type: "select"; processId: string; selection: Selection }
  | { type: "editor-ui"; processId: string; patch: Partial<EditorUi> }
  | { type: "save"; processId: string; message: string; now: number }
  | { type: "discard"; processId: string }
  | { type: "restore"; processId: string; version: number; now: number }
  | { type: "versions-ui"; processId: string; patch: Partial<VersionsUi> }
  | { type: "new-process"; now: number }
  | { type: "import"; process: Process; now: number }
  | { type: "duplicate"; processId: string; now: number }
  | { type: "delete"; processId: string }
  | { type: "import-open"; open: boolean }
  | { type: "connection-open"; id: string | null }
  | { type: "space"; space: string }
  /** A team's cache as last saved on this computer, before the first load. */
  | { type: "team-cache"; teamId: string; cache: TeamCache }
  | { type: "team-page"; teamId: string; page: ProcessPage }
  /** The server accepted a write (save, restore, create, template). `open` goes to its editor. */
  | { type: "team-saved"; teamId: string; process: TeamProcess; open?: "editor" | "start" }
  /** A save hit a newer version (409): keep the draft on top of it and ask. */
  | { type: "team-conflict"; teamId: string; processId: string; current: TeamProcess }
  | { type: "team-versions"; teamId: string; processId: string; versions: ProcessVersion[] }
  | { type: "team-removed"; teamId: string; processId: string }
  | { type: "team-dialog"; dialog: TeamDialog | null }
  | { type: "team-note"; note: string | null };

export function initialState(): ProcessState {
  return {
    route: { view: "home" },
    space: "personal",
    teams: {},
    teamDialog: null,
    teamNote: null,
    records: [],
    disk: { status: "loading", problems: [] },
    runs: [],
    connections: CONNECTIONS,
    editor: {},
    versions: {},
    connection: null,
    importOpen: false,
  };
}

export const DEFAULT_EDITOR: EditorUi = { selection: null, layout: "lanes", json: false };

export function editorUi(state: ProcessState, processId: string): EditorUi {
  return state.editor[processId] ?? DEFAULT_EDITOR;
}

export function versionsUi(state: ProcessState, processId: string): VersionsUi {
  const record = state.records.find((r) => r.id === processId);
  const fallback = record && record.versions.length > 1 ? record.versions[record.versions.length - 2]!.version : null;
  return state.versions[processId] ?? { compare: fallback, mode: "json", side: "to", message: "" };
}

export function recordOf(state: ProcessState, processId: string): ProcessRecord | undefined {
  const own = state.records.find((r) => r.id === processId);
  if (own) return own;
  for (const team of Object.values(state.teams)) {
    const found = team.records.find((r) => r.id === processId);
    if (found) return found;
  }
  return undefined;
}

/** The team a process belongs to, or null for a personal one. */
export function teamOf(state: ProcessState, processId: string): string | null {
  for (const [teamId, team] of Object.entries(state.teams)) {
    if (team.records.some((r) => r.id === processId)) return teamId;
  }
  return null;
}

/** The processes the home shows: yours, or the selected team's. */
export function spaceRecords(state: ProcessState): ProcessRecord[] {
  return state.space === "personal" ? state.records : (state.teams[state.space]?.records ?? []);
}

/** Runs of the processes the home shows. Runs are always local, whoever's process it is. */
export function spaceRuns(state: ProcessState): Run[] {
  const space = state.space;
  return state.runs.filter((r) => (teamOf(state, r.processId) ?? "personal") === space);
}

function replaceRecord(state: ProcessState, record: ProcessRecord): ProcessState {
  const teamId = teamOf(state, record.id);
  if (teamId) {
    const team = state.teams[teamId]!;
    return { ...state, teams: { ...state.teams, [teamId]: { ...team, records: team.records.map((r) => (r.id === record.id ? record : r)) } } };
  }
  return { ...state, records: state.records.map((r) => (r.id === record.id ? record : r)) };
}

function teamSpace(state: ProcessState, teamId: string): TeamSpace {
  return state.teams[teamId] ?? { cache: EMPTY_TEAM_CACHE, records: [] };
}

function withTeam(state: ProcessState, teamId: string, team: TeamSpace): ProcessState {
  return { ...state, teams: { ...state.teams, [teamId]: team } };
}

/** The process a route is about, if any. */
function routeProcess(state: ProcessState): string | null {
  const route = state.route;
  if (route.view === "editor" || route.view === "versions") return route.processId;
  if (route.view === "run") return state.runs.find((r) => r.id === route.runId)?.processId ?? null;
  return null;
}

function replaceRun(state: ProcessState, run: Run): ProcessState {
  return { ...state, runs: state.runs.map((r) => (r.id === run.id ? run : r)) };
}

function addRecord(state: ProcessState, process: Process, message: string, now: number): { state: ProcessState; id: string } {
  const id = uniqueId(process.id || process.name, state.records.map((r) => r.id));
  const content = { ...cloneProcess(process), id, version: 1 };
  const record: ProcessRecord = {
    id,
    draft: null,
    versions: [{ version: 1, message, by: "You", at: now, hash: hashOf(content), process: content }],
  };
  return { state: { ...state, records: [...state.records, record] }, id };
}


export function reducer(state: ProcessState, action: Action): ProcessState {
  switch (action.type) {
    case "navigate":
      return { ...state, route: action.route };

    case "loaded":
      return { ...state, records: action.records, disk: { status: "ready", problems: action.problems } };

    case "load-failed":
      return { ...state, disk: { status: "failed", problems: [], error: action.error } };

    case "use-template": {
      const added = addRecord(state, action.process, `Created from the ${action.name} template`, action.now);
      return { ...added.state, route: { view: "editor", processId: added.id } };
    }

    case "approve":
    case "send-back":
    case "skip":
    case "decide-no":
    case "stop": {
      const run = state.runs.find((r) => r.id === action.runId);
      if (!run) return state;
      const next =
        action.type === "stop"
          ? stopRun(run, action.now)
          : { approve: approveRun, "send-back": sendBackRun, skip: skipRun, "decide-no": decideNoRun }[action.type](
              run,
              action.stepId,
              action.now,
            );
      return next === run ? state : replaceRun(state, next);
    }

    case "edit": {
      const record = recordOf(state, action.processId);
      if (!record) return state;
      return replaceRecord(state, withDraft(record, action.process));
    }

    case "select": {
      const ui = editorUi(state, action.processId);
      return { ...state, editor: { ...state.editor, [action.processId]: { ...ui, selection: action.selection } } };
    }

    case "editor-ui": {
      const ui = editorUi(state, action.processId);
      return { ...state, editor: { ...state.editor, [action.processId]: { ...ui, ...action.patch } } };
    }

    case "save": {
      const record = recordOf(state, action.processId);
      // Team processes save through the API (team-saved).
      if (!record?.draft || teamOf(state, action.processId)) return state;
      const saved = saveVersion(record, action.message, action.now);
      const ui = versionsUi(state, action.processId);
      return {
        ...replaceRecord(state, saved),
        versions: { ...state.versions, [action.processId]: { ...ui, compare: latest(record).version, message: "" } },
      };
    }

    case "discard": {
      const record = recordOf(state, action.processId);
      if (!record?.draft) return state;
      return replaceRecord(state, { ...record, draft: null });
    }

    case "restore": {
      const record = recordOf(state, action.processId);
      if (!record || teamOf(state, action.processId)) return state;
      const restored = restoreVersion(record, action.version, action.now);
      const ui = versionsUi(state, action.processId);
      return { ...replaceRecord(state, restored), versions: { ...state.versions, [action.processId]: { ...ui, compare: action.version } } };
    }

    case "versions-ui": {
      const ui = versionsUi(state, action.processId);
      return { ...state, versions: { ...state.versions, [action.processId]: { ...ui, ...action.patch } } };
    }

    case "new-process": {
      const name = "New process";
      const id = uniqueId(name, state.records.map((r) => r.id));
      const added = addRecord(state, blankProcess(id, name), "Created", action.now);
      return {
        ...added.state,
        route: { view: "editor", processId: added.id },
        editor: { ...added.state.editor, [added.id]: { ...DEFAULT_EDITOR, selection: { kind: "start" } } },
      };
    }

    case "import": {
      const added = addRecord(state, { ...action.process, from: "import" }, "Imported from JSON", action.now);
      return { ...added.state, importOpen: false, route: { view: "editor", processId: added.id } };
    }

    case "duplicate": {
      const record = recordOf(state, action.processId);
      if (!record) return state;
      const source = working(record);
      const copy = { ...cloneProcess(source), name: fitName(`${source.name} copy`), from: `process:${source.id}@${latest(record).version}` };
      return addRecord(state, copy, `Copied from ${source.name}`, action.now).state;
    }

    case "delete": {
      // Its runs go too, so a later process that gets the same id starts clean.
      const { [action.processId]: _editor, ...editor } = state.editor;
      const { [action.processId]: _versions, ...versions } = state.versions;
      return {
        ...state,
        records: state.records.filter((r) => r.id !== action.processId),
        runs: state.runs.filter((r) => r.processId !== action.processId),
        editor,
        versions,
        route: { view: "home" },
      };
    }

    case "import-open":
      return { ...state, importOpen: action.open };

    case "connection-open":
      return { ...state, connection: action.id };

    case "space": {
      if (action.space === state.space) return state;
      const next: ProcessState = { ...state, space: action.space, connection: null, teamNote: null, teamDialog: null };
      // A process from the other space closes back to the home.
      const shown = routeProcess(state);
      if (shown === null || (teamOf(state, shown) ?? "personal") === action.space) return next;
      return { ...next, route: { view: "home" } };
    }

    case "team-cache": {
      if (state.teams[action.teamId]) return state;
      return withTeam(state, action.teamId, { cache: action.cache, records: syncTeamRecords([], action.cache) });
    }

    case "team-page": {
      const team = teamSpace(state, action.teamId);
      const cache = applyPage(team.cache, action.page);
      const records = syncTeamRecords(team.records, cache);
      const gone = team.records.filter((r) => !records.some((x) => x.id === r.id)).map((r) => r.id);
      return gone.reduce(forget, withTeam(state, action.teamId, { cache, records }));
    }

    case "team-saved": {
      const team = teamSpace(state, action.teamId);
      const version = teamVersion(action.process);
      if (!version) return state;
      const existing = team.records.find((r) => r.id === action.process.id);
      const record: ProcessRecord = existing ? { ...withVersion(existing, version), draft: null } : { id: action.process.id, versions: [version], draft: null };
      const cache = putProcess(team.cache, action.process);
      const records = syncTeamRecords(existing ? team.records.map((r) => (r.id === record.id ? record : r)) : [...team.records, record], cache);
      let next = withTeam(state, action.teamId, { cache, records });
      if (existing) {
        const ui = versionsUi(next, record.id);
        next = { ...next, versions: { ...next.versions, [record.id]: { ...ui, compare: latest(existing).version, message: "" } } };
      }
      if (action.open) {
        next = { ...next, route: { view: "editor", processId: record.id } };
        if (action.open === "start") next = { ...next, editor: { ...next.editor, [record.id]: { ...DEFAULT_EDITOR, selection: { kind: "start" } } } };
      }
      const dialog = next.teamDialog;
      return dialog?.kind === "conflict" && dialog.processId === record.id ? { ...next, teamDialog: null } : next;
    }

    case "team-conflict": {
      const team = teamSpace(state, action.teamId);
      const record = team.records.find((r) => r.id === action.processId);
      const version = teamVersion(action.current);
      if (!record || !version) return state;
      const base = latest(record);
      const updated = withVersion(record, version);
      const next = withTeam(state, action.teamId, {
        cache: putProcess(team.cache, action.current),
        records: team.records.map((r) => (r.id === record.id ? updated : r)),
      });
      return {
        ...next,
        teamDialog: { kind: "conflict", processId: record.id, teamId: action.teamId, theirs: action.current, base: { version: base.version, byId: base.byId, at: base.at } },
      };
    }

    case "team-versions": {
      const record = recordOf(state, action.processId);
      if (!record || teamOf(state, action.processId) !== action.teamId) return state;
      // Newer versions arrive through the feed; a draft keeps the version it started from.
      const top = latest(record).version;
      const merged = action.versions.filter((v) => v.version <= top).reduce(withVersion, record);
      return replaceRecord(state, { ...merged, draft: record.draft });
    }

    case "team-removed": {
      const team = teamSpace(state, action.teamId);
      const next = withTeam(state, action.teamId, {
        cache: dropProcess(team.cache, action.processId),
        records: team.records.filter((r) => r.id !== action.processId),
      });
      return forget(next, action.processId);
    }

    case "team-dialog":
      return { ...state, teamDialog: action.dialog };

    case "team-note":
      return { ...state, teamNote: action.note };
  }
}

/** Drops what the tab kept about a process that is gone: its runs, editor and versions place. */
function forget(state: ProcessState, processId: string): ProcessState {
  const { [processId]: _editor, ...editor } = state.editor;
  const { [processId]: _versions, ...versions } = state.versions;
  const dialog = state.teamDialog;
  return {
    ...state,
    runs: state.runs.filter((r) => r.processId !== processId),
    editor,
    versions,
    route: routeProcess(state) === processId ? { view: "home" } : state.route,
    teamDialog: dialog && dialog.processId === processId ? null : dialog,
  };
}
