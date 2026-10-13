// IPC contract between the Rust core and the frontend.
// This is the one file the frontend imports from. Keep it in sync with docs/technical-design.md section 10.
// Rust structs use serde(rename_all = "camelCase"); command arguments are camelCase on this side.

import type { CloudSession } from "../cloud/types";
import type { StoredProcess } from "../process/disk";

export type TerminalId = string;

export type ShellKind = "pwsh" | "windowsPowerShell" | "cmd" | "bash" | "zsh" | "other";

export interface TerminalInfo {
  id: TerminalId;
  cwd: string;
  shell: ShellKind;
  shellPid: number | null;
  exited: boolean;
  exitCode: number | null;
}

// Git ---------------------------------------------------------------------

export interface Worktree {
  path: string;
  head: string | null;
  branch: string | null;
  isMain: boolean;
  isCurrent: boolean;
  detached: boolean;
  locked: boolean;
  prunable: boolean;
  openIn: TerminalId[];
}

export interface GitInfo {
  inRepo: boolean;
  repoRoot: string | null;
  commonDir: string | null;
  repoName: string | null;
  branch: string | null;
  headShort: string | null;
  userName: string | null;
  /** The repo's effective user.name differs from the global one. */
  userNameOverridden: boolean;
  userEmail: string | null;
  remoteUrl: string | null;
  dirty: boolean;
  ahead: number;
  behind: number;
  isWorktree: boolean;
  worktrees: Worktree[];
}

// Review (focus mode) -----------------------------------------------------

/** "?" is an untracked file, "!" a merge conflict; the rest are git's own letters. */
export type ChangeStatus = "M" | "A" | "D" | "R" | "C" | "T" | "?" | "!";

export interface ChangedFile {
  /** Relative to the repository root, forward slashes. */
  path: string;
  oldPath: string | null;
  status: ChangeStatus;
  additions: number | null;
  deletions: number | null;
  binary: boolean;
}

export interface ReviewStatus {
  repoRoot: string;
  branch: string | null;
  head: string | null;
  headShort: string | null;
  staged: ChangedFile[];
  unstaged: ChangedFile[];
}

/**
 * The versions a file's diff is drawn between: HEAD to index for a staged
 * file, index to working tree for an unstaged one. `disk` is what the editor
 * opens. Text is null where the file does not exist, or is binary or too large.
 */
export interface FileVersions {
  old: string | null;
  new: string | null;
  disk: string | null;
  diskMtimeMs: number | null;
  binary: boolean;
  tooLarge: boolean;
}

// Notes -------------------------------------------------------------------

export type NodeKind = "file" | "folder";

export interface NoteNode {
  name: string;
  relPath: string;
  kind: NodeKind;
  mtimeMs: number;
  size: number;
  children: NoteNode[] | null;
}

// Config and session ------------------------------------------------------

export type HotkeyModifier = "ctrl+alt" | "ctrl+shift";

/**
 * "system" follows the OS light/dark setting; "black" is the total darkness
 * theme; "glass" (macOS only) shows the desktop through the window.
 */
export type ThemePreference = "system" | "light" | "dark" | "black" | "glass";

export interface Config {
  version: 1;
  shell: string | null;
  shellArgs: string[];
  notesRoot: string | null;
  agentPatterns: string[];
  idleTimeoutMs: number;
  hotkeyModifier: HotkeyModifier;
  fontFamily: string | null;
  fontSize: number;
  scrollback: number;
  restoreSessionOnLaunch: boolean;
  theme: ThemePreference;
  compactLayout: boolean;
  /** `#rrggbb` for the focused pane's ring; null follows the theme. */
  focusColor: string | null;
  /** `#rrggbb` for a pane whose agent finished; null follows the theme. */
  finishedColor: string | null;
}

/**
 * A note copied out of a pane's output, linked to that terminal session
 * (design turns 6 and 7). Notes written by hand belong to the repository's
 * notes folder instead and have no link.
 */
export interface NoteLink {
  /** Relative to the notes root. */
  relPath: string;
  title: string;
  /** Who wrote what was saved: the agent's name, or "Terminal" for plain shell output. */
  source: string;
  branch: string | null;
  /** Epoch ms when it was saved. */
  at: number;
}

export interface SavedTerminal {
  id: TerminalId;
  cwd: string;
  labelOverride: string | null;
  /** Notes linked to this session; absent in sessions written before them. */
  notes?: NoteLink[];
}

/** The top bar's tabs, in the order they sit there. */
export type AppTab = "process" | "terminals" | "notes";

export interface Session {
  version: 1;
  activeTab: AppTab;
  focusedId: TerminalId | null;
  page: number;
  /** How panes stack; absent in sessions written before the option existed. */
  layoutMode?: "grid" | "columns" | "rows";
  terminals: SavedTerminal[];
  notes: {
    openRelPath: string | null;
    expandedFolders: string[];
    /** Side panels folded away; absent in older sessions. */
    treeCollapsed?: boolean;
    previewCollapsed?: boolean;
    /**
     * Every note copied out of a terminal, by path, so it still reads as
     * copied after its pane is closed. Absent in older sessions.
     */
    copied?: Record<string, NoteLink>;
  };
  /** First-time setup finished or skipped; absent in sessions written before it. */
  setupDone?: boolean;
  /**
   * The cloud account as last seen on this device, and its choices. The
   * session token is never here: it lives in the OS keychain.
   */
  cloud?: CloudSession;
}

// Errors ------------------------------------------------------------------

export type AppErrorCode =
  | "NotFound"
  | "SpawnFailed"
  | "Io"
  | "OutsideRoot"
  | "Conflict"
  | "InvalidName"
  | "GitTimeout"
  | "GitNotInstalled"
  /** The cloud API could not be reached. */
  | "Offline";

export interface AppError {
  code: AppErrorCode;
  message: string;
  currentMtimeMs?: number; // present on Conflict
}

// Events (Rust to frontend) -------------------------------------------------

export const EVENTS = {
  output: "terminal://output",
  exit: "terminal://exit",
  cwd: "terminal://cwd",
  git: "terminal://git",
  agent: "terminal://agent",
  stats: "terminal://stats",
  task: "terminal://task",
  notesChanged: "notes://changed",
  /** augry:// links arrived; take them with cloud_take_deep_links. */
  deepLink: "cloud://deep-link",
} as const;

export interface OutputEvent { id: TerminalId; dataB64: string }
export interface ExitEvent { id: TerminalId; code: number | null }
export interface CwdEvent { id: TerminalId; cwd: string }
export interface GitEvent { id: TerminalId; info: GitInfo }

export type AgentSource = "process" | "shell" | "bell" | "idle";
export interface AgentEvent {
  id: TerminalId;
  event: "started" | "finished";
  name: string;
  durationMs?: number;
  source: AgentSource;
}

// Resource usage of the process tree behind a session. Emitted on the AgentWatcher
// tick (design section 5.1) for every session with a live shell; the pane header
// renders cpu/mem next to the agent badge.
export interface StatsEvent {
  id: TerminalId;
  cpuPercent: number;
  memBytes: number;
}

// What the pane's agent says it is working on (design section 5.5). Every shell
// gets TERMINAL_GRID_TASK_FILE; whatever is written there arrives here as one
// line. `task` is null when the file was emptied.
export interface TaskEvent {
  id: TerminalId;
  task: string | null;
  updatedAtMs: number;
}

export interface NotesChangedEvent {
  relPath: string;
  kind: "create" | "modify" | "remove" | "rename";
}

// Commands (frontend to Rust) ---------------------------------------------
// Keyed by command name. Use with a typed invoke wrapper, e.g.
//   invoke<Commands["create_terminal"]["out"]>("create_terminal", args)

export interface Commands {
  // pty
  create_terminal: { in: { cwd?: string; cols: number; rows: number }; out: TerminalInfo };
  write_terminal: { in: { id: TerminalId; data: string }; out: void };
  write_terminal_bytes: { in: { id: TerminalId; dataB64: string }; out: void };
  resize_terminal: { in: { id: TerminalId; cols: number; rows: number }; out: void };
  close_terminal: { in: { id: TerminalId }; out: void };
  restart_terminal: { in: { id: TerminalId }; out: TerminalInfo };
  list_terminals: { in: Record<string, never>; out: TerminalInfo[] };
  set_terminal_cwd: { in: { id: TerminalId; cwd: string }; out: void };
  report_cwd: { in: { id: TerminalId; cwd: string }; out: void };
  set_focused_terminal: { in: { id: TerminalId | null }; out: void };

  // agents
  /** Agent CLIs found on PATH, for the pane header launchers. */
  installed_agents: { in: Record<string, never>; out: string[] };

  // git
  get_git_info: { in: { id: TerminalId }; out: GitInfo };
  refresh_git_info: { in: { id: TerminalId }; out: void };
  /** Adds a worktree for `branch` next to the main checkout; returns its path. */
  worktree_create: { in: { id: TerminalId; branch: string }; out: string };
  /** Deletes a linked worktree's folder (or prunes it when already gone). */
  worktree_remove: { in: { id: TerminalId; path: string }; out: void };

  // review (focus mode)
  review_status: { in: { id: TerminalId }; out: ReviewStatus };
  /** Every file of the repository, for the edit tree. */
  review_files: { in: { id: TerminalId }; out: string[] };
  review_file: {
    in: { id: TerminalId; path: string; oldPath: string | null; staged: boolean };
    out: FileVersions;
  };
  review_stage: { in: { id: TerminalId; paths: string[] }; out: void };
  review_unstage: { in: { id: TerminalId; paths: string[] }; out: void };
  review_write_file: {
    in: { id: TerminalId; path: string; content: string; expectedMtimeMs?: number };
    out: { mtimeMs: number };
  };

  // notes
  notes_get_root: { in: Record<string, never>; out: { root: string } };
  notes_set_root: { in: { root: string }; out: { root: string } };
  notes_tree: { in: Record<string, never>; out: NoteNode[] };
  notes_read: { in: { relPath: string }; out: { content: string; mtimeMs: number } };
  notes_write: { in: { relPath: string; content: string; expectedMtimeMs?: number }; out: { mtimeMs: number } };
  notes_create: { in: { relPath: string; kind: NodeKind }; out: NoteNode };
  notes_rename: { in: { relPath: string; newRelPath: string }; out: NoteNode };
  notes_delete: { in: { relPath: string; force?: boolean }; out: void };
  notes_reveal: { in: { relPath: string }; out: void };

  // config and session
  config_get: { in: Record<string, never>; out: Config };
  config_set: { in: { patch: Partial<Config> }; out: Config };
  session_get: { in: Record<string, never>; out: Session | null };
  session_set: { in: { session: Session }; out: void };

  // window
  set_window_glass: { in: { enabled: boolean }; out: void };

  // cloud (src-tauri/src/cloud). The token stays on the Rust side; use src/cloud/api.ts.
  cloud_status: { in: Record<string, never>; out: { signedIn: boolean; baseUrl: string; deviceName: string } };
  /** `path` is an API path like /v1/teams. Rejects with Offline when unreachable. */
  cloud_request: {
    in: { method: string; path: string; body: unknown };
    out: { status: number; body: unknown };
  };
  cloud_device_start: { in: Record<string, never>; out: { status: number; body: unknown } };
  /** On 200 the token is stored and left out of the body. */
  cloud_device_poll: { in: { handle: string }; out: { status: number; body: unknown } };
  cloud_sign_out: { in: Record<string, never>; out: void };
  /** github.com and augry.dev (or augry.app) pages only. */
  cloud_open_url: { in: { url: string }; out: void };
  cloud_take_deep_links: { in: Record<string, never>; out: string[] };

  // processes (src-tauri/src/processes): your processes as files; see src/process/disk.ts.
  processes_load: { in: Record<string, never>; out: StoredProcess[] };
  /** Appends a saved version and makes it `<id>.json`; clears the draft. */
  process_save_version: { in: { id: string; version: unknown }; out: void };
  /** Keeps unsaved edits, or clears them with null. */
  process_save_draft: { in: { id: string; draft: unknown | null }; out: void };
  /** Moves the process's files to the trash. */
  process_delete: { in: { id: string }; out: void };
  /** Opens `<id>.json` in the system's app for JSON. */
  process_open_file: { in: { id: string }; out: void };
}

export type CommandName = keyof Commands;
