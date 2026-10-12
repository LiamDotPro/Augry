/**
 * Wires the reducer to the Rust core: bootstrap, event subscriptions, the
 * action surface the components call, session persistence and notes autosave.
 * Components never call `call()` themselves.
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  type ReactNode,
} from "react";
import { ask, open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { call, decodeB64, on } from "../ipc/client";
import { EVENTS } from "../ipc/types";
import type {
  AgentEvent,
  ChangedFile,
  Config,
  CwdEvent,
  ExitEvent,
  FileVersions,
  GitEvent,
  NodeKind,
  NotesChangedEvent,
  OutputEvent,
  StatsEvent,
  TaskEvent,
  ReviewStatus,
  TerminalId,
  TerminalInfo,
  TriggerFiredEvent,
} from "../ipc/types";
import type { LayoutMode } from "../ipc/layout";
import { TASK_REPORT_PROMPT } from "../lib/agentTask";
import { hotkeyScheme, type HotkeyScheme } from "../lib/hotkeys";
import { reviewMessage, reviewMessageOneLine } from "../lib/review";
import { errorMessage, isAppError } from "../lib/appError";
import { basename } from "../lib/format";
import {
  folderFor,
  noteContent,
  noteFromSelection,
  slugFrom,
  splitHeading,
  uniqueNotePath,
} from "../lib/sessionNotes";
import { cdCommand, clearLineKey } from "../lib/shellCommand";
import { firedNotice, NOTICE_MS } from "../lib/triggers";
import { matchesAgentPattern } from "../terminals/osc";
import { terminalRegistry } from "../terminals/registry";
import { closeUnownedTerminals } from "./orphans";
import type { WorktreeChip } from "./model";
import { initialState, reducer, toSession, type Action, type AppState, type ReviewComment } from "./reducer";

const AUTOSAVE_DELAY_MS = 600;
const SESSION_DEBOUNCE_MS = 1000;
const TICK_MS = 1000;
const SUBMIT_DELAY_MS = 150;
/** How long the pane's "Saved to session notes" message, and its Undo, stay. */
const NOTE_TOAST_MS = 6000;

export interface AppActions {
  setTab(tab: "terminals" | "notes"): void;
  toggleTab(): void;
  setPage(page: number): void;
  stepPage(delta: -1 | 1): void;
  setLayoutMode(mode: LayoutMode): void;
  cycleLayoutMode(): void;
  setHotkeysOpen(open: boolean): void;
  setSettingsOpen(open: boolean): void;
  dismissError(): void;
  dismissNotice(): void;

  /** Reads the triggers status, which also picks up definition files added since. */
  loadTriggersStatus(): Promise<void>;
  /** The "Process triggers (experimental)" switch. Applies at once; no Save needed. */
  setTriggersEnabled(enabled: boolean): Promise<void>;
  /** Arms or disarms a local process definition. Applies at once. */
  armProcess(processId: string, armed: boolean): Promise<void>;

  /** Folder picker; the pane goes after the focused one. */
  newTerminal(): Promise<void>;
  /** Same folder as the focused pane, placed right after it. */
  newTerminalHere(): Promise<void>;
  /**
   * Opens a pane next to `from`: in the same folder, or after a folder
   * picker that starts in that folder.
   */
  newTerminalFrom(from: TerminalId, how: "same-folder" | "pick-folder"): Promise<void>;
  /** Opens a pane in `cwd`, placed right after `from`. */
  newTerminalIn(from: TerminalId, cwd: string): Promise<void>;
  /** Types a cd into the pane's shell, moving it into `path`. */
  changeDirectory(id: TerminalId, path: string): Promise<void>;
  /**
   * Adds a worktree for `branch` to the pane's repository and opens a pane in
   * it. Throws (an AppError) when git refuses, so the form can say why.
   */
  createWorktree(id: TerminalId, branch: string): Promise<void>;
  /** Asks, then deletes a linked worktree's folder; the branch stays. */
  removeWorktree(id: TerminalId, worktree: WorktreeChip): Promise<void>;
  closeTerminal(id: TerminalId): Promise<void>;
  restartTerminal(id: TerminalId): Promise<void>;
  focusTerminal(id: TerminalId): void;
  focusIndex(index: number): void;
  /** Types the task report prompt into the pane's agent and submits it. */
  askAgentForTask(id: TerminalId): Promise<void>;
  /** Runs an installed agent CLI (e.g. "claude") in the pane's shell. */
  launchAgent(id: TerminalId, agent: string): Promise<void>;
  movePane(dir: "up" | "down" | "left" | "right"): void;
  moveFocus(dir: "up" | "down" | "left" | "right"): void;

  /** Focus + review on the focused pane, or back to the grid. */
  toggleReview(): void;
  /** Focus + review on `id`. */
  openReview(id: TerminalId): void;
  closeReview(): void;
  /** The pane repository's staged and unstaged files. Throws when it is not a repo. */
  reviewStatus(id: TerminalId): Promise<ReviewStatus>;
  reviewFile(id: TerminalId, file: ChangedFile, staged: boolean): Promise<FileVersions>;
  stageFiles(id: TerminalId, paths: string[]): Promise<void>;
  unstageFiles(id: TerminalId, paths: string[]): Promise<void>;
  /** Saves the review editor's buffer; throws a Conflict when the file moved on disk. */
  writeReviewFile(
    id: TerminalId,
    path: string,
    content: string,
    expectedMtimeMs: number | null,
  ): Promise<{ mtimeMs: number }>;
  addComment(comment: Pick<ReviewComment, "repoRoot" | "path" | "line" | "lineText" | "text">): void;
  editComment(id: string, text: string): void;
  deleteComment(id: string): void;
  /** Drops comments on files that were committed. */
  forgetComments(repoRoot: string, paths: string[]): void;
  /** Types the pending comments (and a note) into the pane's agent and submits them. */
  sendReview(id: TerminalId, commentIds: string[], note: string): Promise<boolean>;

  /** Every file of the pane's repository, for the edit tree. */
  repoFiles(id: TerminalId): Promise<string[]>;

  /** Saves the pane's selection as a note linked to its session (design turn 7). */
  saveSelection(id: TerminalId): Promise<void>;
  /** Copies the pane's selection to the clipboard. */
  copySelection(id: TerminalId): Promise<void>;
  /** Takes the last saved note back: the file goes to the bin, the link goes. */
  undoSavedNote(): Promise<void>;
  dismissNoteToast(): void;
  /**
   * A blank note in the pane repository's notes folder, opened in the Notes
   * tab. Written by hand, so it belongs to the folder, not to the session.
   */
  newFolderNote(id: TerminalId): Promise<void>;
  /** Renames a note of the folder: its heading, its file name and any link to it. */
  renameFolderNote(relPath: string, title: string): Promise<void>;
  /** Switches to the Notes tab with this note open. */
  openInNotes(relPath: string): Promise<void>;
  readNote(relPath: string): Promise<string>;

  saveConfig(patch: Partial<Config>): Promise<void>;
  chooseNotesRoot(): Promise<string | null>;
  setNotesRoot(root: string): Promise<void>;
  toggleFullscreen(): Promise<void>;
  minimize(): Promise<void>;
  toggleMaximize(): Promise<void>;
  closeWindow(): Promise<void>;

  openNote(relPath: string): Promise<void>;
  editNote(content: string): void;
  saveNote(): Promise<void>;
  reloadNote(): Promise<void>;
  keepMine(): Promise<void>;
  toggleFolder(relPath: string): void;
  /** Folds the notes tree or the preview away, or brings it back. */
  collapseNotesPanel(panel: "tree" | "preview", collapsed: boolean): void;
  toggleNotesPanel(panel: "tree" | "preview"): void;
  createNode(parentRelPath: string | null, kind: NodeKind): Promise<void>;
  renameNode(relPath: string, name: string): Promise<void>;
  deleteNode(relPath: string, kind: NodeKind): Promise<void>;
}

const StateContext = createContext<AppState | null>(null);
const ActionsContext = createContext<AppActions | null>(null);

export function useAppState(): AppState {
  const state = useContext(StateContext);
  if (!state) throw new Error("useAppState must be used inside <AppProvider>");
  return state;
}

export function useAppActions(): AppActions {
  const actions = useContext(ActionsContext);
  if (!actions) throw new Error("useAppActions must be used inside <AppProvider>");
  return actions;
}

/** The shortcut layout in force: Command on macOS, the configured chord elsewhere. */
export function useHotkeyScheme(): HotkeyScheme {
  return hotkeyScheme(useAppState().config.hotkeyModifier);
}

export function AppProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, initialState);

  // The event handlers and timers below need the newest state without being
  // re-created on every keystroke, so they read it through a ref.
  const stateRef = useRef(state);
  useEffect(() => {
    stateRef.current = state;
  }, [state]);

  // The app launches fullscreen from a normal (unmaximized) window.
  const maximizedBeforeFullscreen = useRef(false);

  const fail = useCallback((error: unknown) => {
    dispatch({ type: "ui/error", message: errorMessage(error) });
  }, []);

  // Bootstrap ------------------------------------------------------------

  // StrictMode runs this effect twice in dev; the ref survives its simulated
  // remount, so the session is restored once.
  const booted = useRef(false);
  useEffect(() => {
    if (booted.current) return;
    booted.current = true;

    void (async () => {
      try {
        const [config, session, notesRoot] = await Promise.all([
          call("config_get", {}),
          call("session_get", {}),
          call("notes_get_root", {}),
          // No pane exists yet, so every shell the core already runs was left
          // behind by an earlier page load (a webview reload).
          closeUnownedTerminals(new Set()),
        ]);
        dispatch({ type: "ready", config, session });
        dispatch({ type: "notes/root", root: notesRoot.root });
        // Only drives optional launcher buttons, so a failure stays quiet.
        void call("installed_agents", {})
          .then((agents) => dispatch({ type: "agents/installed", agents }))
          .catch(() => {});

        const tree = await call("notes_tree", {});
        dispatch({ type: "notes/tree", tree });

        if (config.restoreSessionOnLaunch && session && session.terminals.length > 0) {
          const infos: TerminalInfo[] = [];
          for (const saved of session.terminals) {
            infos.push(await call("create_terminal", { cwd: saved.cwd, cols: 80, rows: 24 }));
          }
          // Restored shells get fresh ids, so the saved focus is matched by
          // position in the saved order rather than by id.
          const focusedIndex = session.terminals.findIndex(
            (saved) => saved.id === session.focusedId,
          );
          dispatch({
            type: "terminal/restored",
            infos,
            focusedId: focusedIndex === -1 ? null : (infos[focusedIndex]?.id ?? null),
            page: session.page,
            links: session.terminals.map((saved) => saved.notes ?? []),
          });
          for (const info of infos) void refreshGit(info.id);
        }

        if (session?.notes.openRelPath) void openNoteInternal(session.notes.openRelPath);
        dispatch({ type: "booted" });
      } catch (error) {
        // A missing backend is fatal for the terminal surface, but the shell
        // should still render so the user sees why.
        dispatch({ type: "ready", config: stateRef.current.config, session: null });
        dispatch({ type: "booted" });
        fail(error);
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Backend events -------------------------------------------------------

  useEffect(() => {
    // Subscribing fails as a group when the core is not there at all (the page
    // opened outside the desktop shell); report that once rather than leaving
    // seven unhandled rejections behind.
    const subscribe = <T,>(...args: Parameters<typeof on<T>>) =>
      on<T>(...args).catch((error) => {
        fail(error);
        return () => {};
      });

    const pending = [
      subscribe<OutputEvent>(EVENTS.output, (event) => {
        terminalRegistry.write(event.id, decodeB64(event.dataB64));
      }),
      subscribe<ExitEvent>(EVENTS.exit, (event) => {
        dispatch({ type: "terminal/exited", id: event.id, code: event.code });
      }),
      subscribe<CwdEvent>(EVENTS.cwd, (event) => {
        dispatch({ type: "terminal/cwd", id: event.id, cwd: event.cwd });
      }),
      subscribe<GitEvent>(EVENTS.git, (event) => {
        dispatch({ type: "terminal/git", id: event.id, info: event.info });
      }),
      subscribe<AgentEvent>(EVENTS.agent, (event) => {
        dispatch({ type: "terminal/agent", event });
      }),
      subscribe<StatsEvent>(EVENTS.stats, (event) => {
        dispatch({
          type: "terminal/stats",
          id: event.id,
          cpuPercent: event.cpuPercent,
          memBytes: event.memBytes,
        });
      }),
      subscribe<TaskEvent>(EVENTS.task, (event) => {
        dispatch({
          type: "terminal/task",
          id: event.id,
          task: event.task,
          updatedAt: event.updatedAtMs,
        });
      }),
      subscribe<NotesChangedEvent>(EVENTS.notesChanged, (event) => {
        void handleNotesChanged(event);
      }),
      subscribe<TriggerFiredEvent>(EVENTS.triggerFired, (event) => {
        dispatch({ type: "ui/notice", notice: { message: firedNotice(event), at: Date.now() } });
      }),
    ];

    return () => {
      for (const unlisten of pending) void unlisten.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Terminal registry bridge --------------------------------------------

  useEffect(() => {
    terminalRegistry.configure(
      {
        onData: (id, data) => {
          void call("write_terminal", { id, data }).catch(fail);
          // Enter at the prompt starts a command; the next prompt mark ends it.
          if (data.includes("\r") && stateRef.current.terminals[id]?.atPrompt) {
            dispatch({ type: "terminal/prompt", id, atPrompt: false });
          }
        },
        onPrompt: (id) => {
          if (stateRef.current.terminals[id]?.atPrompt !== true) {
            dispatch({ type: "terminal/prompt", id, atPrompt: true });
          }
        },
        onResize: (id, cols, rows) => {
          void call("resize_terminal", { id, cols, rows }).catch(() => {
            // A resize that races a close is not worth surfacing.
          });
        },
        onCwd: (id, cwd) => {
          void call("report_cwd", { id, cwd }).catch(() => {});
        },
        onCommand: (id, command) => {
          dispatch({ type: "terminal/command", id, command });
        },
        onCommandEnd: (id) => {
          // Secondary agent signal (design section 5.2): the process watcher is
          // authoritative, the reducer drops a duplicate within 5 s.
          const term = stateRef.current.terminals[id];
          const patterns = stateRef.current.config.agentPatterns;
          const name = term?.lastCommand ? matchesAgentPattern(term.lastCommand, patterns) : null;
          if (name && term?.agent) {
            dispatch({
              type: "terminal/agent",
              event: { id, event: "finished", name, source: "shell" },
            });
          }
          void call("refresh_git_info", { id }).catch(() => {});
        },
        onBell: (id) => {
          // Tertiary signal (design section 5.3): only when the pane is not focused.
          const term = stateRef.current.terminals[id];
          if (!term?.agent || stateRef.current.focusedId === id) return;
          dispatch({
            type: "terminal/agent",
            event: { id, event: "finished", name: term.agent.name, source: "bell" },
          });
        },
        onFocus: (id) => {
          if (stateRef.current.focusedId !== id) dispatch({ type: "terminal/focus", id });
        },
      },
      state.config,
    );
  }, [state.config, fail]);

  // Clock: drives elapsed times and the idle threshold ------------------

  useEffect(() => {
    const timer = window.setInterval(() => {
      dispatch({ type: "tick", now: Date.now(), activity: terminalRegistry.activity() });
    }, TICK_MS);
    return () => window.clearInterval(timer);
  }, []);

  // Session persistence --------------------------------------------------

  const sessionKey = useMemo(
    () => JSON.stringify(toSession(state)),
    // Recomputing the session on every render is cheap next to the debounce.
    [state],
  );

  useEffect(() => {
    if (!state.ready) return;
    const timer = window.setTimeout(() => {
      void call("session_set", { session: JSON.parse(sessionKey) }).catch(() => {});
    }, SESSION_DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [sessionKey, state.ready]);

  // The saved note message goes away by itself.
  useEffect(() => {
    const toast = state.noteToast;
    if (!toast) return;
    const timer = window.setTimeout(() => {
      if (stateRef.current.noteToast === toast) dispatch({ type: "notes/toast", toast: null });
    }, NOTE_TOAST_MS);
    return () => window.clearTimeout(timer);
  }, [state.noteToast]);

  // The trigger notice goes away by itself.
  useEffect(() => {
    const notice = state.notice;
    if (!notice) return;
    const timer = window.setTimeout(() => {
      if (stateRef.current.notice === notice) dispatch({ type: "ui/notice", notice: null });
    }, NOTICE_MS);
    return () => window.clearTimeout(timer);
  }, [state.notice]);

  // Notes autosave -------------------------------------------------------

  const writeNote = useCallback(async () => {
    const notes = stateRef.current.notes;
    if (!notes.openPath || notes.content === notes.baseline || notes.conflict) return;
    const written = notes.content;
    dispatch({ type: "notes/saving" });
    try {
      const result = await call("notes_write", {
        relPath: notes.openPath,
        content: written,
        expectedMtimeMs: notes.mtimeMs ?? undefined,
      });
      dispatch({ type: "notes/saved", mtimeMs: result.mtimeMs, content: written });
    } catch (error) {
      if (isAppError(error, "Conflict")) {
        const current = (error as { currentMtimeMs?: number }).currentMtimeMs ?? Date.now();
        dispatch({ type: "notes/conflict", currentMtimeMs: current });
      } else {
        dispatch({ type: "notes/save-failed" });
        fail(error);
      }
    }
  }, [fail]);

  useEffect(() => {
    // Only a genuinely dirty buffer autosaves: "error" waits for the next edit.
    if (state.notes.saveState !== "dirty" || state.notes.conflict) return;
    const timer = window.setTimeout(() => void writeNote(), AUTOSAVE_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [state.notes.saveState, state.notes.content, state.notes.conflict, writeNote]);

  // Flush pending edits when the app loses focus or the tab changes.
  useEffect(() => {
    const flush = () => void writeNote();
    window.addEventListener("blur", flush);
    window.addEventListener("beforeunload", flush);
    return () => {
      window.removeEventListener("blur", flush);
      window.removeEventListener("beforeunload", flush);
      flush();
    };
  }, [state.activeTab, writeNote]);

  // Notes helpers --------------------------------------------------------

  const refreshTree = useCallback(async () => {
    try {
      dispatch({ type: "notes/tree", tree: await call("notes_tree", {}) });
    } catch (error) {
      fail(error);
    }
  }, [fail]);

  const openNoteInternal = useCallback(
    async (relPath: string) => {
      dispatch({ type: "notes/opening", relPath });
      try {
        const file = await call("notes_read", { relPath });
        dispatch({ type: "notes/opened", relPath, content: file.content, mtimeMs: file.mtimeMs });
      } catch (error) {
        dispatch({ type: "notes/closed" });
        fail(error);
      }
    },
    [fail],
  );

  const handleNotesChanged = useCallback(
    async (event: NotesChangedEvent) => {
      await refreshTree();
      const notes = stateRef.current.notes;
      if (event.relPath !== notes.openPath) return;
      if (event.kind === "remove") {
        dispatch({ type: "notes/closed" });
        return;
      }
      // A clean buffer just follows the file; a dirty one asks (design section 8.4).
      if (notes.saveState === "clean" || notes.saveState === "saved") {
        await openNoteInternal(notes.openPath);
      } else {
        dispatch({ type: "notes/conflict", currentMtimeMs: Date.now() });
      }
    },
    [openNoteInternal, refreshTree],
  );

  const refreshGit = useCallback(async (id: TerminalId) => {
    try {
      dispatch({ type: "terminal/git", id, info: await call("get_git_info", { id }) });
    } catch {
      // Not every cwd is a repository; the pane simply shows no git labels.
    }
  }, []);

  // Actions --------------------------------------------------------------

  const actions = useMemo<AppActions>(() => {
    const spawn = async (cwd: string | undefined, focus: boolean, after: TerminalId | null) => {
      dispatch({ type: "ui/busy", busy: true });
      try {
        const info = await call("create_terminal", { cwd, cols: 80, rows: 24 });
        dispatch({ type: "terminal/added", info, focus, after });
        void refreshGit(info.id);
        void call("set_focused_terminal", { id: info.id }).catch(() => {});
      } catch (error) {
        fail(error);
      } finally {
        dispatch({ type: "ui/busy", busy: false });
      }
    };

    const pickFolder = async (title: string, defaultPath?: string) => {
      const picked = await openDialog({ directory: true, multiple: false, title, defaultPath });
      return typeof picked === "string" ? picked : null;
    };

    /** The pane a new one should land behind: the origin if given, else the focused pane. */
    const anchorFor = (from: TerminalId | null) => {
      const current = stateRef.current;
      const id = from ?? current.focusedId;
      return id && current.terminals[id] ? id : null;
    };

    const openFrom = async (from: TerminalId | null, how: "same-folder" | "pick-folder") => {
      const anchor = anchorFor(from);
      const anchorCwd = anchor ? stateRef.current.terminals[anchor]?.info.cwd : undefined;
      try {
        if (how === "same-folder") {
          await spawn(anchorCwd, true, anchor);
          return;
        }
        const folder = await pickFolder("Open folder in a new pane", anchorCwd);
        if (folder) await spawn(folder, true, anchor);
      } catch (error) {
        fail(error);
      }
    };

    /**
     * Creates `<repo>/<slug>.md` for the pane's repository. `copiedFrom` names
     * who wrote text copied out of the terminal, and links the note to the
     * session; a note written by hand has none.
     */
    const createRepoNote = async (id: TerminalId, title: string, body: string, copiedFrom: string | null) => {
      const term = stateRef.current.terminals[id];
      if (!term) return null;
      const repo = term.git?.repoName ?? basename(term.info.cwd);
      const taken = new Set(flattenPaths(stateRef.current.notes.tree));
      const wanted = uniqueNotePath(folderFor(repo), slugFrom(title), taken);
      const node = await call("notes_create", { relPath: wanted, kind: "file" });
      await call("notes_write", { relPath: node.relPath, content: noteContent(title, body) });
      await refreshTree();
      const at = Date.now();
      if (copiedFrom) {
        dispatch({
          type: "notes/link-add",
          id,
          link: { relPath: node.relPath, title, source: copiedFrom, branch: term.git?.branch ?? null, at },
        });
      }
      return { relPath: node.relPath, at };
    };

    /**
     * A focus hotkey: moves the ring like the reducer always did, and also
     * hands the keyboard to the terminal it lands on, as a click does. Focus
     * mode keeps the keyboard in its review panel.
     */
    const focusVia = (action: Action) => {
      const before = stateRef.current.focusedId;
      const after = reducer(stateRef.current, action);
      dispatch(action);
      const id = after.focusedId;
      if (!id) return;
      if (!after.focusMode) {
        // After the render, once the pane's page is the visible one.
        window.setTimeout(() => terminalRegistry.focus(id), 0);
      }
      if (id !== before) {
        void call("set_focused_terminal", { id }).catch(() => {});
        void refreshGit(id);
      }
    };

    const openInNotes = async (relPath: string) => {
      dispatch({ type: "tab/set", tab: "notes" });
      const expanded = stateRef.current.notes.expanded;
      const parts = relPath.split("/").slice(0, -1);
      for (let index = 1; index <= parts.length; index++) {
        const folder = parts.slice(0, index).join("/");
        if (!expanded.includes(folder)) dispatch({ type: "notes/toggle-folder", relPath: folder });
      }
      await writeNote();
      await openNoteInternal(relPath);
    };

    return {
      setTab: (tab) => dispatch({ type: "tab/set", tab }),
      setLayoutMode: (mode) => dispatch({ type: "layout/set", mode }),
      cycleLayoutMode: () => dispatch({ type: "layout/cycle" }),
      toggleTab: () => dispatch({ type: "tab/toggle" }),
      setPage: (page) => dispatch({ type: "page/set", page }),
      stepPage: (delta) => dispatch({ type: "page/step", delta }),
      setHotkeysOpen: (open) => dispatch({ type: "ui/hotkeys", open }),
      setSettingsOpen: (open) => dispatch({ type: "ui/settings", open }),
      dismissError: () => dispatch({ type: "ui/error", message: null }),
      dismissNotice: () => dispatch({ type: "ui/notice", notice: null }),

      loadTriggersStatus: async () => {
        try {
          dispatch({ type: "triggers/status", status: await call("triggers_status", {}) });
        } catch (error) {
          fail(error);
        }
      },

      setTriggersEnabled: async (enabled) => {
        try {
          const config = await call("config_set", { patch: { triggers: { enabled } } });
          dispatch({ type: "config/set", config });
          dispatch({ type: "triggers/status", status: await call("triggers_status", {}) });
        } catch (error) {
          fail(error);
        }
      },

      armProcess: async (processId, armed) => {
        try {
          await call("triggers_arm", { processId, armed });
          dispatch({ type: "config/set", config: await call("config_get", {}) });
          dispatch({ type: "triggers/status", status: await call("triggers_status", {}) });
        } catch (error) {
          fail(error);
        }
      },

      newTerminal: () => openFrom(null, "pick-folder"),
      newTerminalHere: () => openFrom(null, "same-folder"),
      newTerminalFrom: (from, how) => openFrom(from, how),
      newTerminalIn: (from, cwd) => spawn(cwd, true, anchorFor(from)),

      changeDirectory: async (id, path) => {
        const term = stateRef.current.terminals[id];
        if (!term || term.info.exited) return;
        const shell = term.info.shell;
        try {
          const clear = clearLineKey(shell);
          if (clear) {
            await call("write_terminal", { id, data: clear });
            await new Promise((resolve) => window.setTimeout(resolve, SUBMIT_DELAY_MS));
          }
          await call("write_terminal", { id, data: `${cdCommand(shell, path)}\r` });
          // cmd has no shell integration to report the new folder through OSC 7.
          if (shell === "cmd") await call("set_terminal_cwd", { id, cwd: path });
        } catch (error) {
          fail(error);
        }
        terminalRegistry.focus(id);
      },

      createWorktree: async (id, branch) => {
        const path = await call("worktree_create", { id, branch });
        await spawn(path, true, id);
      },

      removeWorktree: async (id, worktree) => {
        const confirmed = await ask(
          worktree.prunable
            ? `The folder ${worktree.key} is already gone. This clears git's record of it.`
            : `Deletes the folder ${worktree.key}. Its branch and commits stay in the repository.`,
          { title: "Remove worktree?", kind: "warning", okLabel: "Remove", cancelLabel: "Keep it" },
        );
        if (!confirmed) return;
        try {
          await call("worktree_remove", { id, path: worktree.key });
        } catch (error) {
          fail(error);
        }
      },

      closeTerminal: async (id) => {
        const running = stateRef.current.terminals[id]?.agent;
        if (running) {
          const confirmed = await ask(`${running.name} is still running in this pane.`, {
            title: "Close pane?",
            kind: "warning",
            okLabel: "Close pane",
            cancelLabel: "Keep it open",
          });
          if (!confirmed) return;
        }
        try {
          await call("close_terminal", { id });
        } catch (error) {
          fail(error);
        } finally {
          terminalRegistry.dispose(id);
          dispatch({ type: "terminal/removed", id });
        }
      },

      restartTerminal: async (id) => {
        try {
          const { cols, rows } = terminalRegistry.size(id);
          terminalRegistry.reset(id);
          const info = await call("restart_terminal", { id });
          dispatch({ type: "terminal/restarted", info });
          await call("resize_terminal", { id, cols, rows });
          void refreshGit(id);
        } catch (error) {
          fail(error);
        }
      },

      focusTerminal: (id) => {
        dispatch({ type: "terminal/focus", id });
        terminalRegistry.focus(id);
        void call("set_focused_terminal", { id }).catch(() => {});
        void refreshGit(id);
      },

      askAgentForTask: async (id) => {
        try {
          await call("write_terminal", { id, data: TASK_REPORT_PROMPT });
          // Sent together, agent TUIs read the Enter as part of a paste and
          // leave the prompt sitting unsubmitted.
          await new Promise((resolve) => window.setTimeout(resolve, SUBMIT_DELAY_MS));
          await call("write_terminal", { id, data: "\r" });
        } catch (error) {
          fail(error);
        }
      },

      launchAgent: async (id, agent) => {
        try {
          await call("write_terminal", { id, data: `${agent}\r` });
          terminalRegistry.focus(id);
        } catch (error) {
          fail(error);
        }
      },

      toggleReview: () => {
        const leaving = stateRef.current.focusMode;
        dispatch({ type: "review/toggle" });
        // Back on the grid the keyboard goes where it was before: the terminal.
        const id = stateRef.current.focusedId;
        if (leaving && id) window.setTimeout(() => terminalRegistry.focus(id), 0);
      },
      openReview: (id) => {
        dispatch({ type: "review/open", id });
        void call("set_focused_terminal", { id }).catch(() => {});
        void refreshGit(id);
      },
      closeReview: () => {
        dispatch({ type: "review/close" });
        const id = stateRef.current.focusedId;
        if (id) window.setTimeout(() => terminalRegistry.focus(id), 0);
      },

      reviewStatus: (id) => call("review_status", { id }),
      reviewFile: (id, file, staged) =>
        call("review_file", { id, path: file.path, oldPath: file.oldPath, staged }),
      stageFiles: async (id, paths) => {
        try {
          await call("review_stage", { id, paths });
        } catch (error) {
          fail(error);
        }
      },
      unstageFiles: async (id, paths) => {
        try {
          await call("review_unstage", { id, paths });
        } catch (error) {
          fail(error);
        }
      },
      writeReviewFile: (id, path, content, expectedMtimeMs) =>
        call("review_write_file", { id, path, content, expectedMtimeMs: expectedMtimeMs ?? undefined }),

      addComment: (comment) =>
        dispatch({
          type: "review/comment-add",
          comment: {
            ...comment,
            id: crypto.randomUUID(),
            status: "pending",
            createdAt: Date.now(),
          },
        }),
      editComment: (id, text) => dispatch({ type: "review/comment-edit", id, text }),
      deleteComment: (id) => dispatch({ type: "review/comment-delete", id }),
      forgetComments: (repoRoot, paths) => dispatch({ type: "review/forget", repoRoot, paths }),

      sendReview: async (id, commentIds, note) => {
        const wanted = new Set(commentIds);
        const comments = stateRef.current.review.comments.filter((comment) => wanted.has(comment.id));
        const term = stateRef.current.terminals[id];
        if (comments.length === 0 || !term || term.info.exited) return false;
        try {
          // A bracketed paste keeps the message's newlines inside the prompt;
          // without one, a single line is the only safe form.
          if (!terminalRegistry.paste(id, reviewMessage(comments, note))) {
            await call("write_terminal", { id, data: reviewMessageOneLine(comments, note) });
          }
          await new Promise((resolve) => window.setTimeout(resolve, SUBMIT_DELAY_MS));
          await call("write_terminal", { id, data: "\r" });
          dispatch({
            type: "review/sent",
            ids: comments.map((comment) => comment.id),
            repoRoot: comments[0]!.repoRoot,
            paneId: id,
            at: Date.now(),
          });
          return true;
        } catch (error) {
          fail(error);
          return false;
        }
      },

      repoFiles: (id) => call("review_files", { id }),

      saveSelection: async (id) => {
        const term = stateRef.current.terminals[id];
        const note = noteFromSelection(terminalRegistry.selection(id));
        if (!term || !note) return;
        try {
          const source = term.agent?.name ?? term.finished?.name ?? "Terminal";
          const saved = await createRepoNote(id, note.title, note.body, source);
          if (!saved) return;
          terminalRegistry.clearSelection(id);
          dispatch({ type: "notes/toast", toast: { paneId: id, relPath: saved.relPath, at: saved.at } });
        } catch (error) {
          fail(error);
        }
      },

      copySelection: async (id) => {
        try {
          await terminalRegistry.copySelection(id);
        } catch (error) {
          fail(error);
        }
      },

      undoSavedNote: async () => {
        const toast = stateRef.current.noteToast;
        if (!toast) return;
        dispatch({ type: "notes/toast", toast: null });
        try {
          await call("notes_delete", { relPath: toast.relPath, force: true });
        } catch (error) {
          fail(error);
          return;
        }
        if (stateRef.current.notes.openPath === toast.relPath) dispatch({ type: "notes/closed" });
        dispatch({ type: "notes/link-remove", relPath: toast.relPath });
        await refreshTree();
      },

      dismissNoteToast: () => dispatch({ type: "notes/toast", toast: null }),

      newFolderNote: async (id) => {
        try {
          const saved = await createRepoNote(id, "Untitled", "", null);
          if (saved) await openInNotes(saved.relPath);
        } catch (error) {
          fail(error);
        }
      },

      renameFolderNote: async (relPath, title) => {
        const trimmed = title.trim();
        if (!trimmed) return;
        try {
          // A dirty buffer of this note reaches disk before its heading is rewritten.
          if (stateRef.current.notes.openPath === relPath) await writeNote();
          const file = await call("notes_read", { relPath });
          const { body } = splitHeading(file.content);
          await call("notes_write", { relPath, content: noteContent(trimmed, body.replace(/\n+$/, "")) });

          const folder = relPath.includes("/") ? relPath.slice(0, relPath.lastIndexOf("/")) : "";
          const taken = new Set(flattenPaths(stateRef.current.notes.tree));
          taken.delete(relPath);
          const wanted = uniqueNotePath(folder, slugFrom(trimmed), taken);
          const target = wanted === relPath ? relPath : (await call("notes_rename", { relPath, newRelPath: wanted })).relPath;
          dispatch({ type: "notes/link-moved", from: relPath, to: target, title: trimmed });
          await refreshTree();
          if (stateRef.current.notes.openPath === relPath) await openNoteInternal(target);
        } catch (error) {
          fail(error);
        }
      },

      openInNotes,

      readNote: async (relPath) => (await call("notes_read", { relPath })).content,

      focusIndex: (index) => focusVia({ type: "terminal/focus-index", index }),
      movePane: (dir) => dispatch({ type: "terminal/move", dir }),
      moveFocus: (dir) => focusVia({ type: "terminal/move-focus", dir }),

      saveConfig: async (patch) => {
        try {
          dispatch({ type: "config/set", config: await call("config_set", { patch }) });
        } catch (error) {
          fail(error);
        }
      },

      chooseNotesRoot: async () => {
        try {
          const folder = await pickFolder(
            "Choose the notes folder",
            stateRef.current.notes.root ?? undefined,
          );
          return folder;
        } catch (error) {
          fail(error);
          return null;
        }
      },

      setNotesRoot: async (root) => {
        try {
          const result = await call("notes_set_root", { root });
          dispatch({ type: "notes/root", root: result.root });
          dispatch({ type: "notes/closed" });
          await refreshTree();
        } catch (error) {
          fail(error);
        }
      },

      toggleFullscreen: async () => {
        const appWindow = getCurrentWindow();
        if (!(await appWindow.isFullscreen())) {
          maximizedBeforeFullscreen.current = await appWindow.isMaximized();
          await appWindow.setFullscreen(true);
          return;
        }
        await appWindow.setFullscreen(false);
        // A title bar double-click while fullscreen maximizes the window
        // underneath, invisibly. Come back to the frame the user left, so a
        // window that was resizable before fullscreen still is afterwards.
        if (!maximizedBeforeFullscreen.current && (await appWindow.isMaximized())) {
          await appWindow.unmaximize();
        }
      },
      minimize: () => getCurrentWindow().minimize(),
      toggleMaximize: () => getCurrentWindow().toggleMaximize(),
      closeWindow: () => getCurrentWindow().close(),

      openNote: async (relPath) => {
        await writeNote();
        await openNoteInternal(relPath);
      },

      editNote: (content) => dispatch({ type: "notes/edit", content }),
      saveNote: writeNote,

      reloadNote: async () => {
        const open = stateRef.current.notes.openPath;
        dispatch({ type: "notes/conflict-dismissed" });
        if (open) await openNoteInternal(open);
      },

      keepMine: async () => {
        // Drop the mtime guard so the next write wins, then save immediately.
        const notes = stateRef.current.notes;
        dispatch({ type: "notes/conflict-dismissed" });
        if (!notes.openPath) return;
        try {
          const result = await call("notes_write", {
            relPath: notes.openPath,
            content: notes.content,
          });
          dispatch({ type: "notes/saved", mtimeMs: result.mtimeMs, content: notes.content });
        } catch (error) {
          fail(error);
        }
      },

      collapseNotesPanel: (panel, collapsed) =>
        dispatch({ type: "notes/collapse", panel, collapsed }),
      toggleNotesPanel: (panel) => dispatch({ type: "notes/toggle-panel", panel }),
      toggleFolder: (relPath) => dispatch({ type: "notes/toggle-folder", relPath }),

      createNode: async (parentRelPath, kind) => {
        const base = kind === "file" ? "untitled" : "new-folder";
        const suffix = kind === "file" ? ".md" : "";
        const existing = new Set(flattenPaths(stateRef.current.notes.tree));
        let relPath = joinRelPath(parentRelPath, `${base}${suffix}`);
        for (let n = 2; existing.has(relPath); n++) {
          relPath = joinRelPath(parentRelPath, `${base}-${n}${suffix}`);
        }
        try {
          const node = await call("notes_create", { relPath, kind });
          await refreshTree();
          if (kind === "file") await openNoteInternal(node.relPath);
          else dispatch({ type: "notes/toggle-folder", relPath: node.relPath });
        } catch (error) {
          fail(error);
        }
      },

      renameNode: async (relPath, name) => {
        const trimmed = name.trim();
        if (!trimmed) return;
        const parent = relPath.includes("/") ? relPath.slice(0, relPath.lastIndexOf("/")) : null;
        const newRelPath = joinRelPath(parent, trimmed);
        if (newRelPath === relPath) return;
        try {
          const node = await call("notes_rename", { relPath, newRelPath });
          dispatch({ type: "notes/link-moved", from: relPath, to: node.relPath });
          await refreshTree();
          if (stateRef.current.notes.openPath === relPath) await openNoteInternal(node.relPath);
        } catch (error) {
          fail(error);
        }
      },

      deleteNode: async (relPath, kind) => {
        if (kind === "folder") {
          const confirmed = await ask(`Move "${relPath}" and everything in it to the Recycle Bin?`, {
            title: "Delete folder?",
            kind: "warning",
            okLabel: "Delete",
          });
          if (!confirmed) return;
        }
        try {
          await call("notes_delete", { relPath, force: true });
          if (stateRef.current.notes.openPath === relPath) dispatch({ type: "notes/closed" });
          if (kind === "file") dispatch({ type: "notes/link-remove", relPath });
          await refreshTree();
        } catch (error) {
          fail(error);
        }
      },
    };
  }, [fail, openNoteInternal, refreshGit, refreshTree, writeNote]);

  return (
    <StateContext.Provider value={state}>
      <ActionsContext.Provider value={actions}>{children}</ActionsContext.Provider>
    </StateContext.Provider>
  );
}

function joinRelPath(parent: string | null, name: string): string {
  return parent ? `${parent}/${name}` : name;
}

function flattenPaths(nodes: { relPath: string; children: unknown }[]): string[] {
  const out: string[] = [];
  const walk = (list: typeof nodes) => {
    for (const node of list) {
      out.push(node.relPath);
      if (Array.isArray(node.children)) walk(node.children as typeof nodes);
    }
  };
  walk(nodes);
  return out;
}
