/**
 * The single application reducer. Everything the UI renders is derived from
 * this state; async work lives in AppProvider and only ever dispatches here.
 */
import {
  LAYOUT_MODES,
  PAGE_SIZE,
  insertAfter,
  layoutFor,
  moveTerminal,
  nextLayoutMode,
  pageCount,
  pageOf,
  pageSlice,
} from "../ipc/layout";
import type { Direction, LayoutMode } from "../ipc/layout";
import type {
  AgentEvent,
  AppTab,
  Config,
  GitInfo,
  NoteLink,
  NoteNode,
  Session,
  TerminalId,
  TerminalInfo,
} from "../ipc/types";
import { EMPTY_CLOUD_SESSION, type CloudSession } from "../cloud/types";
import type { TerminalState } from "./model";

export const DEFAULT_CONFIG: Config = {
  version: 1,
  shell: null,
  shellArgs: [],
  notesRoot: null,
  agentPatterns: ["claude", "codex", "gemini", "aider", "gpt", "copilot"],
  idleTimeoutMs: 4000,
  hotkeyModifier: "ctrl+alt",
  fontFamily: null,
  fontSize: 14,
  scrollback: 10000,
  restoreSessionOnLaunch: true,
  theme: "system",
  compactLayout: false,
  focusColor: null,
  finishedColor: null,
};

export type SaveState = "clean" | "dirty" | "saving" | "saved" | "error";

export interface NotesState {
  root: string | null;
  tree: NoteNode[];
  expanded: string[];
  openPath: string | null;
  content: string;
  /** Content as it exists on disk, used to decide whether the buffer is dirty. */
  baseline: string;
  mtimeMs: number | null;
  saveState: SaveState;
  /** Set when the open file changed on disk while the buffer was dirty. */
  conflict: { currentMtimeMs: number } | null;
  loading: boolean;
  /** The tree and the preview fold away; the editor always stays. */
  treeCollapsed: boolean;
  previewCollapsed: boolean;
}

/** A line comment in the review panel. Only staged files take comments. */
export interface ReviewComment {
  id: string;
  /** Comments belong to a repository, so every pane on it sees them. */
  repoRoot: string;
  path: string;
  /** Line in the staged (index) version of the file. */
  line: number;
  /** That line's text when the comment was written, to follow it if the file moves. */
  lineText: string;
  text: string;
  /** Pending until sent to an agent; sent ones drop out of the count. */
  status: "pending" | "sent";
  createdAt: number;
}

export interface SentBatch {
  repoRoot: string;
  paneId: TerminalId;
  count: number;
  at: number;
}

export interface ReviewState {
  comments: ReviewComment[];
  /** The last batch sent, for the header's "sent to pane N" marker. */
  sent: SentBatch | null;
}

/** The pane's "Saved to session notes" message, with its Undo. */
export interface NoteToast {
  paneId: TerminalId;
  relPath: string;
  at: number;
}

export interface AppState {
  ready: boolean;
  /** Launch is over: config and notes are loaded and the saved panes restored. */
  booted: boolean;
  /** How many saved panes launch is bringing back, for the splash's status line. */
  restoringPanes: number;
  config: Config;
  /** Agent CLIs on PATH; each idle pane offers a button to start one. */
  installedAgents: string[];
  activeTab: AppTab;
  /** First-time setup is over (finished or skipped); until then it runs after the splash. */
  setupDone: boolean;
  /**
   * The cloud account as last seen, the teams and this device's choices.
   * Optional: everything works signed out. Kept in the session; the token is
   * in the OS keychain. src/cloud/CloudProvider.tsx drives it.
   */
  cloud: CloudSession;
  order: TerminalId[];
  terminals: Record<TerminalId, TerminalState>;
  focusedId: TerminalId | null;
  page: number;
  /** How the panes on a page stack: both ways, side by side, or on top of each other. */
  layoutMode: LayoutMode;
  /** Focus + review: the focused pane next to a review panel for its repository. */
  focusMode: boolean;
  review: ReviewState;
  notes: NotesState;
  noteToast: NoteToast | null;
  /**
   * Notes copied out of a terminal, by path, outliving the pane they came
   * from; any other note in a folder was written by hand.
   */
  copiedNotes: Record<string, NoteLink>;
  settingsOpen: boolean;
  hotkeysOpen: boolean;
  busy: boolean;
  error: string | null;
  /** Wall clock, advanced by a 1 s tick while the window is visible. */
  now: number;
}

export const initialState: AppState = {
  ready: false,
  booted: false,
  restoringPanes: 0,
  config: DEFAULT_CONFIG,
  installedAgents: [],
  activeTab: "terminals",
  setupDone: true,
  cloud: EMPTY_CLOUD_SESSION,
  order: [],
  terminals: {},
  focusedId: null,
  page: 0,
  layoutMode: "grid",
  focusMode: false,
  review: { comments: [], sent: null },
  notes: {
    root: null,
    tree: [],
    expanded: [],
    openPath: null,
    content: "",
    baseline: "",
    mtimeMs: null,
    saveState: "clean",
    conflict: null,
    loading: false,
    treeCollapsed: false,
    previewCollapsed: false,
  },
  noteToast: null,
  copiedNotes: {},
  settingsOpen: false,
  hotkeysOpen: false,
  busy: false,
  error: null,
  now: Date.now(),
};

export type Action =
  | { type: "ready"; config: Config; session: Session | null }
  | { type: "booted" }
  | { type: "config/set"; config: Config }
  | { type: "agents/installed"; agents: string[] }
  | { type: "tab/set"; tab: AppTab }
  /** Steps to the next tab in the bar's order, wrapping round. */
  | { type: "tab/toggle" }
  | { type: "setup/done" }
  | { type: "cloud/set"; patch: Partial<CloudSession> }
  | { type: "page/set"; page: number }
  | { type: "page/step"; delta: -1 | 1 }
  | { type: "layout/set"; mode: LayoutMode }
  | { type: "layout/cycle" }
  | { type: "ui/settings"; open: boolean }
  | { type: "ui/hotkeys"; open: boolean }
  | { type: "ui/busy"; busy: boolean }
  | { type: "ui/error"; message: string | null }
  | { type: "tick"; now: number; activity: Record<TerminalId, number> }
  /** `after` places the new pane right behind that one instead of at the end. */
  | { type: "terminal/added"; info: TerminalInfo; focus: boolean; after?: TerminalId | null }
  | {
      type: "terminal/restored";
      infos: TerminalInfo[];
      focusedId: TerminalId | null;
      page: number;
      /** Each restored session's note links, by position. */
      links?: NoteLink[][];
    }
  | { type: "terminal/removed"; id: TerminalId }
  | { type: "terminal/restarted"; info: TerminalInfo }
  | { type: "terminal/exited"; id: TerminalId; code: number | null }
  | { type: "terminal/cwd"; id: TerminalId; cwd: string }
  | { type: "terminal/git"; id: TerminalId; info: GitInfo }
  | { type: "terminal/agent"; event: AgentEvent }
  | { type: "terminal/stats"; id: TerminalId; cpuPercent: number; memBytes: number }
  | { type: "terminal/command"; id: TerminalId; command: string }
  | { type: "terminal/prompt"; id: TerminalId; atPrompt: boolean }
  | { type: "terminal/task"; id: TerminalId; task: string | null; updatedAt: number }
  | { type: "terminal/focus"; id: TerminalId | null }
  | { type: "terminal/focus-index"; index: number }
  | { type: "terminal/move"; dir: Direction }
  | { type: "terminal/move-focus"; dir: Direction }
  | { type: "review/toggle" }
  /** Focus mode on `id`: from a pill in the bar, or the focus + review hotkey. */
  | { type: "review/open"; id: TerminalId }
  | { type: "review/close" }
  | { type: "review/comment-add"; comment: ReviewComment }
  | { type: "review/comment-edit"; id: string; text: string }
  | { type: "review/comment-delete"; id: string }
  | { type: "review/sent"; ids: string[]; repoRoot: string; paneId: TerminalId; at: number }
  /** Drops the comments on files that were committed and so left the review. */
  | { type: "review/forget"; repoRoot: string; paths: string[] }
  | { type: "notes/link-add"; id: TerminalId; link: NoteLink }
  /** A linked note went away (deleted, or the save was undone). */
  | { type: "notes/link-remove"; relPath: string }
  /** A note or a folder of notes was renamed; links follow it. */
  | { type: "notes/link-moved"; from: string; to: string; title?: string }
  | { type: "notes/toast"; toast: NoteToast | null }
  | { type: "notes/root"; root: string }
  | { type: "notes/collapse"; panel: "tree" | "preview"; collapsed: boolean }
  | { type: "notes/toggle-panel"; panel: "tree" | "preview" }
  | { type: "notes/tree"; tree: NoteNode[] }
  | { type: "notes/toggle-folder"; relPath: string }
  | { type: "notes/opening"; relPath: string }
  | { type: "notes/opened"; relPath: string; content: string; mtimeMs: number }
  | { type: "notes/closed" }
  | { type: "notes/edit"; content: string }
  | { type: "notes/saving" }
  | { type: "notes/saved"; mtimeMs: number; content: string }
  | { type: "notes/save-failed" }
  | { type: "notes/conflict"; currentMtimeMs: number }
  | { type: "notes/conflict-dismissed" };

export function reducer(state: AppState, action: Action): AppState {
  switch (action.type) {
    case "ready": {
      const session = action.session;
      return {
        ...state,
        ready: true,
        restoringPanes: action.config.restoreSessionOnLaunch ? (session?.terminals.length ?? 0) : 0,
        config: action.config,
        activeTab: isAppTab(session?.activeTab) ? session.activeTab : state.activeTab,
        // A session from before setup existed means the app was already in use.
        setupDone: session ? (session.setupDone ?? true) : false,
        cloud: { ...EMPTY_CLOUD_SESSION, ...session?.cloud },
        layoutMode: isLayoutMode(session?.layoutMode) ? session.layoutMode : state.layoutMode,
        notes: {
          ...state.notes,
          expanded: session?.notes.expandedFolders ?? [],
          openPath: session?.notes.openRelPath ?? null,
          treeCollapsed: session?.notes.treeCollapsed ?? state.notes.treeCollapsed,
          previewCollapsed: session?.notes.previewCollapsed ?? state.notes.previewCollapsed,
        },
        copiedNotes: session?.notes.copied ?? state.copiedNotes,
      };
    }

    case "booted":
      return { ...state, booted: true };

    case "config/set":
      return { ...state, config: action.config };

    case "agents/installed":
      return { ...state, installedAgents: action.agents };

    case "tab/set":
      return { ...state, activeTab: action.tab, hotkeysOpen: false };

    case "tab/toggle":
      return {
        ...state,
        activeTab: APP_TABS[(APP_TABS.indexOf(state.activeTab) + 1) % APP_TABS.length],
        hotkeysOpen: false,
      };

    case "setup/done":
      return { ...state, setupDone: true };

    case "cloud/set":
      return { ...state, cloud: { ...state.cloud, ...action.patch } };

    case "page/set":
      return { ...state, page: clampPage(action.page, state.order.length) };

    case "page/step":
      return { ...state, page: clampPage(state.page + action.delta, state.order.length) };

    case "layout/set":
      return state.layoutMode === action.mode ? state : { ...state, layoutMode: action.mode };

    case "layout/cycle":
      return { ...state, layoutMode: nextLayoutMode(state.layoutMode) };

    case "ui/settings":
      return { ...state, settingsOpen: action.open, hotkeysOpen: false };

    case "ui/hotkeys":
      return { ...state, hotkeysOpen: action.open };

    case "ui/busy":
      return { ...state, busy: action.busy };

    case "ui/error":
      return { ...state, error: action.message };

    case "tick": {
      let terminals = state.terminals;
      for (const [id, at] of Object.entries(action.activity)) {
        const term = terminals[id];
        if (!term || term.lastOutputAt >= at) continue;
        if (terminals === state.terminals) terminals = { ...terminals };
        terminals[id] = { ...term, lastOutputAt: at };
      }
      return { ...state, now: action.now, terminals };
    }

    case "terminal/added": {
      const order = insertAfter(state.order, action.info.id, action.after ?? null);
      return {
        ...state,
        order,
        terminals: { ...state.terminals, [action.info.id]: newTerminal(action.info, state.now) },
        focusedId: action.focus ? action.info.id : state.focusedId,
        page: action.focus
          ? pageOf(order.indexOf(action.info.id))
          : clampPage(state.page, order.length),
      };
    }

    case "terminal/restored": {
      const terminals: Record<TerminalId, TerminalState> = { ...state.terminals };
      action.infos.forEach((info, index) => {
        terminals[info.id] = { ...newTerminal(info, state.now), notes: action.links?.[index] ?? [] };
      });
      const order = [...state.order, ...action.infos.map((info) => info.id)];
      const focusedId =
        action.focusedId && terminals[action.focusedId] ? action.focusedId : (order[0] ?? null);
      return { ...state, order, terminals, focusedId, page: clampPage(action.page, order.length) };
    }

    case "terminal/removed": {
      const index = state.order.indexOf(action.id);
      if (index === -1) return state;
      const order = state.order.filter((id) => id !== action.id);
      const terminals = { ...state.terminals };
      delete terminals[action.id];
      const focusedId =
        state.focusedId === action.id
          ? (order[Math.min(index, order.length - 1)] ?? null)
          : state.focusedId;
      return {
        ...state,
        order,
        terminals,
        focusedId,
        page: clampPage(state.page, order.length),
        focusMode: state.focusMode && focusedId !== null,
      };
    }

    case "terminal/restarted": {
      const previous = state.terminals[action.info.id];
      if (!previous) return state;
      return {
        ...state,
        terminals: {
          ...state.terminals,
          [action.info.id]: { ...newTerminal(action.info, state.now), git: previous.git, notes: previous.notes },
        },
      };
    }

    case "terminal/exited":
      return patchTerminal(state, action.id, (term) => ({
        ...term,
        info: { ...term.info, exited: true, exitCode: action.code },
        agent: null,
        task: null,
        finished: null,
        stats: null,
      }));

    case "terminal/cwd":
      return patchTerminal(state, action.id, (term) => ({
        ...term,
        info: { ...term.info, cwd: action.cwd },
      }));

    case "terminal/git":
      return patchTerminal(state, action.id, (term) => ({ ...term, git: action.info }));

    case "terminal/agent": {
      const { id, event, name, durationMs } = action.event;
      return patchTerminal(state, id, (term) => {
        if (event === "started") {
          return {
            ...term,
            agent: { name, startedAt: state.now },
            // A task from the previous run would describe the wrong work.
            task: null,
            finished: null,
            lastOutputAt: state.now,
          };
        }
        // Ignore a duplicate finish from a slower detection source (design section 5).
        if (term.finished && state.now - term.finished.at < 5000) return term;
        return {
          ...term,
          agent: null,
          finished: {
            name: name || term.agent?.name || "Agent",
            at: state.now,
            durationMs: durationMs ?? null,
          },
        };
      });
    }

    case "terminal/stats":
      return patchTerminal(state, action.id, (term) => ({
        ...term,
        stats: { cpuPercent: action.cpuPercent, memBytes: action.memBytes },
      }));

    case "terminal/command":
      return patchTerminal(state, action.id, (term) => ({ ...term, lastCommand: action.command }));

    case "terminal/prompt": {
      const term = state.terminals[action.id];
      if (!term || term.atPrompt === action.atPrompt) return state;
      return patchTerminal(state, action.id, () => ({ ...term, atPrompt: action.atPrompt }));
    }

    case "terminal/task":
      return patchTerminal(state, action.id, (term) => ({
        ...term,
        task: action.task === null ? null : { text: action.task, updatedAt: action.updatedAt },
      }));

    case "terminal/focus": {
      if (action.id === null) return { ...state, focusedId: null };
      const term = state.terminals[action.id];
      if (!term) return state;
      // Looking at a pane acknowledges its finished marker.
      const terminals = term.finished
        ? { ...state.terminals, [action.id]: { ...term, finished: null } }
        : state.terminals;
      return {
        ...state,
        terminals,
        focusedId: action.id,
        page: pageOf(state.order.indexOf(action.id)),
      };
    }

    case "terminal/focus-index": {
      const id = pageSlice(state.order, state.page)[action.index];
      return id ? reducer(state, { type: "terminal/focus", id }) : state;
    }

    case "terminal/move": {
      if (!state.focusedId) return state;
      const order = moveTerminal(state.order, state.focusedId, action.dir, state.layoutMode);
      if (order === state.order) return state;
      return { ...state, order, page: pageOf(order.indexOf(state.focusedId)) };
    }

    case "terminal/move-focus": {
      const id = neighbourId(state, action.dir);
      return id ? reducer(state, { type: "terminal/focus", id }) : state;
    }

    case "review/toggle":
      if (state.focusMode) return { ...state, focusMode: false };
      if (!state.focusedId || !state.terminals[state.focusedId]) return state;
      return { ...state, focusMode: true, activeTab: "terminals", hotkeysOpen: false };

    case "review/open": {
      const focused = reducer(state, { type: "terminal/focus", id: action.id });
      if (focused.focusedId !== action.id) return state;
      return { ...focused, focusMode: true, activeTab: "terminals", hotkeysOpen: false };
    }

    case "review/close":
      return state.focusMode ? { ...state, focusMode: false } : state;

    case "review/comment-add":
      return { ...state, review: { ...state.review, comments: [...state.review.comments, action.comment] } };

    case "review/comment-edit":
      return patchComments(state, (comments) =>
        comments.map((comment) =>
          comment.id === action.id
            ? // Rewording a sent comment makes it something to send again.
              { ...comment, text: action.text, status: "pending" }
            : comment,
        ),
      );

    case "review/comment-delete":
      return patchComments(state, (comments) => comments.filter((comment) => comment.id !== action.id));

    case "review/sent": {
      const ids = new Set(action.ids);
      const comments = state.review.comments.map((comment) =>
        ids.has(comment.id) ? { ...comment, status: "sent" as const } : comment,
      );
      return {
        ...state,
        review: {
          comments,
          sent: { repoRoot: action.repoRoot, paneId: action.paneId, count: ids.size, at: action.at },
        },
      };
    }

    case "review/forget": {
      const paths = new Set(action.paths);
      return patchComments(state, (comments) =>
        comments.filter((comment) => comment.repoRoot !== action.repoRoot || !paths.has(comment.path)),
      );
    }

    case "notes/link-add": {
      const linked = patchTerminal(state, action.id, (term) => ({
        ...term,
        notes: [...term.notes.filter((link) => link.relPath !== action.link.relPath), action.link],
      }));
      return { ...linked, copiedNotes: { ...linked.copiedNotes, [action.link.relPath]: action.link } };
    }

    case "notes/link-remove": {
      const copiedNotes = { ...state.copiedNotes };
      delete copiedNotes[action.relPath];
      return {
        ...patchLinks(state, (link) => (link.relPath === action.relPath ? null : link)),
        copiedNotes,
        noteToast: state.noteToast?.relPath === action.relPath ? null : state.noteToast,
      };
    }

    case "notes/link-moved": {
      const prefix = `${action.from}/`;
      const move = (link: NoteLink): NoteLink => {
        if (link.relPath === action.from) {
          return { ...link, relPath: action.to, title: action.title ?? link.title };
        }
        if (link.relPath.startsWith(prefix)) {
          return { ...link, relPath: `${action.to}/${link.relPath.slice(prefix.length)}` };
        }
        return link;
      };
      const copiedNotes: Record<string, NoteLink> = {};
      for (const link of Object.values(state.copiedNotes)) {
        const moved = move(link);
        copiedNotes[moved.relPath] = moved;
      }
      return { ...patchLinks(state, move), copiedNotes };
    }

    case "notes/toast":
      return { ...state, noteToast: action.toast };

    case "notes/root":
      return { ...state, notes: { ...state.notes, root: action.root } };

    case "notes/collapse": {
      const key = action.panel === "tree" ? "treeCollapsed" : "previewCollapsed";
      if (state.notes[key] === action.collapsed) return state;
      return { ...state, notes: { ...state.notes, [key]: action.collapsed } };
    }

    case "notes/toggle-panel": {
      const key = action.panel === "tree" ? "treeCollapsed" : "previewCollapsed";
      return { ...state, notes: { ...state.notes, [key]: !state.notes[key] } };
    }

    case "notes/tree":
      return { ...state, notes: { ...state.notes, tree: action.tree } };

    case "notes/toggle-folder": {
      const expanded = state.notes.expanded.includes(action.relPath)
        ? state.notes.expanded.filter((path) => path !== action.relPath)
        : [...state.notes.expanded, action.relPath];
      return { ...state, notes: { ...state.notes, expanded } };
    }

    case "notes/opening":
      return {
        ...state,
        notes: { ...state.notes, openPath: action.relPath, loading: true, conflict: null },
      };

    case "notes/opened":
      return {
        ...state,
        notes: {
          ...state.notes,
          openPath: action.relPath,
          content: action.content,
          baseline: action.content,
          mtimeMs: action.mtimeMs,
          saveState: "clean",
          conflict: null,
          loading: false,
        },
      };

    case "notes/closed":
      return {
        ...state,
        notes: {
          ...state.notes,
          openPath: null,
          content: "",
          baseline: "",
          mtimeMs: null,
          saveState: "clean",
          conflict: null,
          loading: false,
        },
      };

    case "notes/edit":
      return {
        ...state,
        notes: {
          ...state.notes,
          content: action.content,
          saveState: action.content === state.notes.baseline ? "clean" : "dirty",
        },
      };

    case "notes/saving":
      return { ...state, notes: { ...state.notes, saveState: "saving" } };

    case "notes/saved":
      // `content` is what actually reached disk. Anything typed while the write
      // was in flight leaves the buffer dirty again.
      return {
        ...state,
        notes: {
          ...state.notes,
          baseline: action.content,
          mtimeMs: action.mtimeMs,
          saveState: state.notes.content === action.content ? "saved" : "dirty",
          conflict: null,
        },
      };

    case "notes/save-failed":
      // Parked until the next edit, so a failing write is not retried in a loop.
      return { ...state, notes: { ...state.notes, saveState: "error" } };

    case "notes/conflict":
      return {
        ...state,
        notes: {
          ...state.notes,
          saveState: "dirty",
          conflict: { currentMtimeMs: action.currentMtimeMs },
        },
      };

    case "notes/conflict-dismissed":
      return { ...state, notes: { ...state.notes, conflict: null } };

    default:
      return state;
  }
}

// Helpers -----------------------------------------------------------------

function newTerminal(info: TerminalInfo, now: number): TerminalState {
  return {
    info,
    git: null,
    agent: null,
    task: null,
    finished: null,
    lastCommand: null,
    atPrompt: null,
    stats: null,
    lastOutputAt: now,
    notes: [],
  };
}

function patchTerminal(
  state: AppState,
  id: TerminalId,
  patch: (term: TerminalState) => TerminalState,
): AppState {
  const term = state.terminals[id];
  if (!term) return state;
  const next = patch(term);
  if (next === term) return state;
  return { ...state, terminals: { ...state.terminals, [id]: next } };
}

/** Maps every note link of every terminal; null drops the link. */
function patchLinks(state: AppState, patch: (link: NoteLink) => NoteLink | null): AppState {
  let terminals = state.terminals;
  for (const [id, term] of Object.entries(state.terminals)) {
    let changed = false;
    const notes: NoteLink[] = [];
    for (const link of term.notes) {
      const next = patch(link);
      if (next !== link) changed = true;
      if (next) notes.push(next);
    }
    if (!changed) continue;
    if (terminals === state.terminals) terminals = { ...terminals };
    terminals[id] = { ...term, notes };
  }
  return terminals === state.terminals ? state : { ...state, terminals };
}

function patchComments(
  state: AppState,
  patch: (comments: ReviewComment[]) => ReviewComment[],
): AppState {
  return { ...state, review: { ...state.review, comments: patch(state.review.comments) } };
}

function clampPage(page: number, total: number): number {
  return Math.min(Math.max(0, page), pageCount(total) - 1);
}

function isLayoutMode(value: unknown): value is LayoutMode {
  return typeof value === "string" && (LAYOUT_MODES as readonly string[]).includes(value);
}

/** The terminal one cell away from the focused one, inside the current page. */
function neighbourId(state: AppState, dir: Direction): TerminalId | null {
  if (!state.focusedId) return null;
  const index = state.order.indexOf(state.focusedId);
  if (index === -1) return null;
  const page = pageOf(index);
  const onPage = pageSlice(state.order, page);
  const { cols } = layoutFor(onPage.length, state.layoutMode);
  const local = index - page * PAGE_SIZE;
  const row = Math.floor(local / cols);
  const col = local % cols;

  // Stacked in one column, up and down walk the list and nothing lies sideways.
  if (state.layoutMode === "rows") {
    if (dir === "up") return local > 0 ? (onPage[local - 1] ?? null) : null;
    if (dir === "down") return onPage[local + 1] ?? null;
    return null;
  }

  let target: number | null = null;
  if (dir === "up" && row > 0) target = local - cols;
  if (dir === "down" && local + cols < onPage.length) target = local + cols;
  if (dir === "left" && col > 0) target = local - 1;
  if (dir === "right" && col < cols - 1 && local + 1 < onPage.length) target = local + 1;

  return target === null ? null : (onPage[target] ?? null);
}

/** The top bar's tabs, left to right. */
export const APP_TABS: readonly AppTab[] = ["process", "terminals", "notes"];

function isAppTab(value: unknown): value is AppTab {
  return APP_TABS.includes(value as AppTab);
}

// Selectors ---------------------------------------------------------------

export function visibleIds(state: AppState): TerminalId[] {
  return pageSlice(state.order, state.page);
}

export function totalPages(state: AppState): number {
  return pageCount(state.order.length);
}

export function toSession(state: AppState): Session {
  return {
    version: 1,
    activeTab: state.activeTab,
    setupDone: state.setupDone,
    cloud: state.cloud,
    focusedId: state.focusedId,
    page: state.page,
    layoutMode: state.layoutMode,
    terminals: state.order.map((id) => ({
      id,
      cwd: state.terminals[id]?.info.cwd ?? "",
      labelOverride: null,
      notes: state.terminals[id]?.notes ?? [],
    })),
    notes: {
      openRelPath: state.notes.openPath,
      expandedFolders: state.notes.expanded,
      treeCollapsed: state.notes.treeCollapsed,
      previewCollapsed: state.notes.previewCollapsed,
      copied: state.copiedNotes,
    },
  };
}
