/**
 * Everything the Process tab holds, as one reducer. ProcessView keeps it in
 * React state for as long as the app runs, so switching to Terminals or Notes
 * and back never loses the view, the open process or run, the selection, the
 * Lanes / Runbook choice or a draft. The data is mocked (mock.ts) until the
 * backend reads process files and drives runs.
 */

import { seedConnections, testConnection, connectedStatus } from "./connections";
import { blankProcess, TEMPLATES } from "./examples";
import { cloneProcess, hashOf } from "./json";
import { seedRecords, seedRuns, newestFile } from "./mock";
import { uniqueId } from "./model";
import { advanceRun, approveRun, decideNoRun, sendBackRun, skipRun, startRun, stopRun } from "./runs";
import type { Connection, Process, ProcessRecord, Run } from "./types";
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

export interface Notice {
  id: number;
  runId: string;
  title: string;
  file: string;
  folder: string;
}

export interface ProcessState {
  route: Route;
  records: ProcessRecord[];
  /** Newest first. */
  runs: Run[];
  connections: Connection[];
  editor: Record<string, EditorUi>;
  versions: Record<string, VersionsUi>;
  /** The connection open in the side panel. */
  connection: string | null;
  /** The Copy and run sheet, for a template. */
  sheet: { templateId: string } | null;
  importOpen: boolean;
  notice: Notice | null;
}

export type Action =
  | { type: "navigate"; route: Route }
  | { type: "run"; processId: string; trigger?: string; detail?: string; now: number }
  | { type: "copy-template"; templateId: string; run: boolean; trigger?: string; detail?: string; now: number }
  | { type: "tick"; now: number }
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
  | { type: "file-arrived"; processId: string; now: number }
  | { type: "dismiss-notice" }
  | { type: "sheet"; templateId: string | null }
  | { type: "import-open"; open: boolean }
  | { type: "connection-open"; id: string | null }
  | { type: "connection-set"; id: string; key: string; value: string }
  | { type: "connection-test"; id: string }
  | { type: "connection-save"; id: string }
  | { type: "connection-remove"; id: string };

export function initialState(now: number): ProcessState {
  const records = seedRecords(now);
  return {
    route: { view: "home" },
    records,
    runs: seedRuns(records, now),
    connections: seedConnections(),
    editor: {},
    versions: {},
    connection: null,
    sheet: null,
    importOpen: false,
    notice: null,
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
  return state.records.find((r) => r.id === processId);
}

/** The trigger a run gets from a process's starting point when started by hand. */
export function defaultTrigger(process: Process, now: number): { trigger: string; detail?: string } {
  switch (process.start.on) {
    case "ticket":
      return { trigger: "issue #142", detail: "#142" };
    case "folder":
      return { trigger: "folder", detail: newestFile(now) };
    case "webhook":
      return { trigger: "webhook", detail: "test request" };
    case "branch":
      return { trigger: "push", detail: "main" };
    case "schedule":
      return { trigger: "schedule" };
    case "notes":
      return { trigger: "note" };
    case "manual":
      return { trigger: "manual" };
  }
}

function replaceRecord(state: ProcessState, record: ProcessRecord): ProcessState {
  return { ...state, records: state.records.map((r) => (r.id === record.id ? record : r)) };
}

function replaceRun(state: ProcessState, run: Run): ProcessState {
  return { ...state, runs: state.runs.map((r) => (r.id === run.id ? run : r)) };
}

/** Starts a run of a process; a draft is saved as a version first, as the spec says. */
function beginRun(state: ProcessState, processId: string, now: number, trigger?: string, detail?: string): { state: ProcessState; run: Run | null } {
  let record = recordOf(state, processId);
  if (!record) return { state, run: null };
  if (record.draft) {
    record = saveVersion(record, "Saved before a run", now);
    state = replaceRecord(state, record);
  }
  const current = latest(record);
  const number = Math.max(0, ...state.runs.filter((r) => r.processId === processId).map((r) => r.number)) + 1;
  const fallback = defaultTrigger(current.process, now);
  const run = startRun(current.process, {
    processId,
    number,
    version: current.version,
    trigger: trigger ?? fallback.trigger,
    detail: detail ?? (trigger ? undefined : fallback.detail),
    now,
  });
  return { state: { ...state, runs: [run, ...state.runs] }, run };
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

function withConnection(state: ProcessState, id: string, change: (c: Connection) => Connection): ProcessState {
  return { ...state, connections: state.connections.map((c) => (c.id === id ? change(c) : c)) };
}

export function reducer(state: ProcessState, action: Action): ProcessState {
  switch (action.type) {
    case "navigate":
      return { ...state, route: action.route, sheet: null };

    case "run": {
      const started = beginRun(state, action.processId, action.now, action.trigger, action.detail);
      if (!started.run) return state;
      return { ...started.state, route: { view: "run", runId: started.run.id } };
    }

    case "copy-template": {
      const template = TEMPLATES.find((t) => t.id === action.templateId);
      if (!template) return state;
      const process = { ...cloneProcess(template.process), from: `template:${template.id}@${template.revision}` };
      const added = addRecord(state, process, `Created from template ${template.name}@${template.revision}`, action.now);
      const next = { ...added.state, sheet: null };
      if (!action.run) return { ...next, route: { view: "editor", processId: added.id } };
      const started = beginRun(next, added.id, action.now, action.trigger, action.detail);
      return started.run ? { ...started.state, route: { view: "run", runId: started.run.id } } : started.state;
    }

    case "tick": {
      let changed = false;
      const runs = state.runs.map((r) => {
        const next = advanceRun(r, action.now);
        if (next !== r) changed = true;
        return next;
      });
      return changed ? { ...state, runs } : state;
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
      if (!record?.draft) return state;
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
      if (!record) return state;
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
      const copy = { ...cloneProcess(source), name: `${source.name} copy`, from: `process:${source.id}@${latest(record).version}` };
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

    case "file-arrived": {
      const record = recordOf(state, action.processId);
      if (!record) return state;
      const process = latest(record).process;
      const file = newestFile(action.now);
      const started = beginRun(state, action.processId, action.now, "folder", file);
      if (!started.run) return state;
      return {
        ...started.state,
        notice: {
          id: action.now,
          runId: started.run.id,
          title: `${process.name} started`,
          file,
          folder: String(process.start.path ?? "the folder"),
        },
      };
    }

    case "dismiss-notice":
      return { ...state, notice: null };

    case "sheet":
      return { ...state, sheet: action.templateId ? { templateId: action.templateId } : null };

    case "import-open":
      return { ...state, importOpen: action.open };

    case "connection-open":
      return { ...state, connection: action.id };

    case "connection-set":
      return withConnection(state, action.id, (c) => ({ ...c, settings: { ...c.settings, [action.key]: action.value }, test: undefined }));

    case "connection-test":
      return withConnection(state, action.id, (c) => ({ ...c, test: testConnection(c) }));

    case "connection-save":
      return withConnection(state, action.id, (c) => {
        const test = testConnection(c);
        if (!test.ok) return { ...c, test };
        return { ...c, on: true, test, status: connectedStatus(c), used: c.used || "Not used yet" };
      });

    case "connection-remove":
      return withConnection(state, action.id, (c) => ({ ...c, on: false, status: "Not set up", used: "", test: undefined }));
  }
}
