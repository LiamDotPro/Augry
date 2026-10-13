/**
 * Runs notes sync in the background and tells the Notes tab what to show
 * (Notes board 02). Sync is on when signed in, the server has the "sync"
 * feature and the "Sync your notes?" choice turned on Personal or a team;
 * otherwise this provides `active: false` and the Notes tab is as it was.
 *
 * A round runs after a local save, after notes change on disk (debounced),
 * when sync is turned on, when the app is back online, and every 30 s while
 * online.
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import * as api from "../cloud/api";
import { useCloud } from "../cloud/CloudProvider";
import type { TeamDetail } from "../cloud/types";
import { on } from "../ipc/client";
import { EVENTS, type NoteNode } from "../ipc/types";
import { useAppActions, useAppState } from "../state/AppProvider";
import { EMPTY_SNAPSHOT, NotesSyncEngine, type LocalConflict, type SyncSnapshot } from "./engine";
import { notesIO, syncApi } from "./io";
import { isSyncableNote, PERSONAL, type SpaceKey, type SpaceLayout } from "./paths";
import { footerCopy, type SyncTone } from "./status";
import {
  lastMemberCounts,
  loadIndex,
  rememberMemberCount,
  removeIndex,
  saveIndex,
  teamFolderFor,
} from "./store";

const PERIOD_MS = 30_000;
const AFTER_SAVE_MS = 500;
const AFTER_CHANGE_MS = 1500;

/** A top-level root of the tree while sync is on: Personal, then each synced team. */
export interface SyncRoot {
  key: SpaceKey;
  kind: "personal" | "team";
  label: string;
  /** The folder it shows, relative to the notes folder; "" for Personal. */
  folder: string;
  /** "only you", "team · 6". */
  mark: string;
}

export interface RowMark {
  text: string;
  tone: "muted" | "warn";
  /** Why, for the mark's tooltip: the reason a note stays on this computer. */
  reason?: string;
}

export interface NotesSyncValue {
  active: boolean;
  roots: SyncRoot[];
  /** Offline, by the last call or the last round. */
  offline: boolean;
  snapshot: SyncSnapshot;
  footer: { tone: SyncTone; line: string; text: string } | null;
  /** Personal syncs. */
  personal: boolean;
  /** The root a note shows under and its path inside it. */
  rootOf(relPath: string): { root: SyncRoot; path: string } | null;
  /** The small mark at the end of a tree row. */
  markOf(node: NoteNode, selected: boolean): RowMark | null;
  /** The conflict a note is either side of. */
  conflictFor(relPath: string): LocalConflict | null;
  keepMine(conflict: LocalConflict): Promise<void>;
  useTheirs(conflict: LocalConflict): Promise<void>;
  /** Switches the editor between yours and theirs. */
  compare(conflict: LocalConflict): Promise<void>;
}

const INACTIVE: NotesSyncValue = {
  active: false,
  roots: [],
  offline: false,
  snapshot: EMPTY_SNAPSHOT,
  footer: null,
  personal: false,
  rootOf: () => null,
  markOf: () => null,
  conflictFor: () => null,
  keepMine: async () => {},
  useTheirs: async () => {},
  compare: async () => {},
};

const NotesSyncContext = createContext<NotesSyncValue>(INACTIVE);

export function useNotesSync(): NotesSyncValue {
  return useContext(NotesSyncContext);
}

export function NotesSyncProvider({ children }: { children: ReactNode }) {
  const cloud = useCloud();
  const { notes } = useAppState();
  const actions = useAppActions();

  const engine = useMemo(
    () => new NotesSyncEngine(notesIO, syncApi, { load: loadIndex, save: saveIndex, remove: removeIndex }),
    [],
  );
  const snapshot = useSyncExternalStore(
    useCallback((listener: () => void) => engine.subscribe(listener), [engine]),
    () => engine.snapshot,
  );

  // What syncs ------------------------------------------------------------

  const choice = cloud.notesSync;
  const userId = cloud.user?.id ?? null;
  const root = notes.root;
  // Started offline, cloud.has goes by the features the server listed last time.
  const has = cloud.has;
  const allowed = cloud.signedIn && has("sync") && choice !== null && root !== null && userId !== null;
  const teams = has("teams") ? cloud.teams : [];
  const syncedTeams = allowed ? teams.filter((team) => choice!.teams.includes(team.id)) : [];
  const personal = allowed && choice!.personal;
  const active = personal || syncedTeams.length > 0;

  const layoutKey = JSON.stringify([
    personal,
    syncedTeams.map((team) => [team.id, team.name]),
    teams.map((team) => team.name),
  ]);
  const layout = useMemo<SpaceLayout | null>(() => {
    if (!active) return null;
    return {
      spaces: [
        ...(personal ? [{ key: PERSONAL, folder: "" }] : []),
        ...syncedTeams.map((team) => ({ key: team.id, folder: teamFolderFor(team.id, team.name, true) })),
      ],
      teamFolders: teams.map((team) => teamFolderFor(team.id, team.name, false)),
    };
    // layoutKey stands for personal, syncedTeams and teams.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, layoutKey]);

  // Teammates, for "team · 6" and the name on a conflict copy -------------

  const [members, setMembers] = useState<Record<string, TeamDetail["members"]>>({});
  const membersRef = useRef(members);
  membersRef.current = members;
  const syncedIds = syncedTeams.map((team) => team.id).join(",");
  useEffect(() => {
    if (!syncedIds || !cloud.online) return;
    let cancelled = false;
    for (const id of syncedIds.split(",")) {
      api
        .getTeam(id)
        .then((team) => {
          if (cancelled) return;
          setMembers((current) => ({ ...current, [id]: team.members }));
          if (userId) rememberMemberCount(userId, id, team.members.length);
        })
        .catch(() => {});
    }
    return () => {
      cancelled = true;
    };
  }, [syncedIds, cloud.online, userId]);

  // The engine ------------------------------------------------------------

  const notesRef = useRef(notes);
  notesRef.current = notes;
  const onlineRef = useRef(cloud.online);
  onlineRef.current = cloud.online;
  const markSyncedRef = useRef(cloud.markSynced);
  markSyncedRef.current = cloud.markSynced;
  const refreshTeamsRef = useRef(cloud.refreshTeams);
  refreshTeamsRef.current = cloud.refreshTeams;

  const run = useCallback(() => {
    void engine.sync(onlineRef.current).catch((error) => console.warn("notes sync", error));
  }, [engine]);

  useEffect(() => {
    if (!layout || !userId || !root) {
      engine.configure(null);
      return;
    }
    engine.configure({
      userId,
      root,
      layout,
      nameOf: async (space, who) => {
        if (space === PERSONAL || who === userId) return "another device";
        let list = membersRef.current[space];
        if (!list) {
          try {
            list = (await api.getTeam(space)).members;
          } catch {
            list = [];
          }
        }
        const member = list.find((m) => m.user.id === who);
        return member ? member.user.name || member.user.login : "a teammate";
      },
      editing: (relPath) => {
        const current = notesRef.current;
        if (current.openPath !== relPath) return false;
        return current.conflict !== null || current.saveState === "dirty" || current.saveState === "saving" || current.saveState === "error";
      },
      onSynced: (space, at) => markSyncedRef.current(space === PERSONAL ? "notes:personal" : `notes:${space}`, at),
      // Left or deleted: the teams list catches up, and the team drops out of what syncs.
      onGone: () => void refreshTeamsRef.current().catch(() => {}),
    });
    run();
  }, [engine, layout, userId, root, run]);

  // Back online (or just gone offline: work out what waits).
  useEffect(() => {
    if (active) run();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cloud.online]);

  const timer = useRef(0);
  const schedule = useCallback(
    (ms: number) => {
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(run, ms);
    },
    [run],
  );
  useEffect(() => () => window.clearTimeout(timer.current), []);

  // After a save.
  useEffect(() => {
    if (active && notes.saveState === "saved") schedule(AFTER_SAVE_MS);
  }, [active, notes.saveState, notes.mtimeMs, schedule]);

  // After notes change on disk (other apps, git, our own writes settle quickly).
  useEffect(() => {
    if (!active) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void on(EVENTS.notesChanged, () => schedule(AFTER_CHANGE_MS))
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [active, schedule]);

  // Pull now and then while online.
  useEffect(() => {
    if (!active || !cloud.online) return;
    const interval = window.setInterval(run, PERIOD_MS);
    return () => window.clearInterval(interval);
  }, [active, cloud.online, run]);

  // What the tab shows ----------------------------------------------------

  const value = useMemo<NotesSyncValue>(() => {
    if (!active || !layout) return INACTIVE;
    const offline = !cloud.online || snapshot.offline;
    const seen = userId ? lastMemberCounts(userId) : {};
    const countOf = (teamId: string): number | null => members[teamId]?.length ?? seen[teamId] ?? null;
    const roots: SyncRoot[] = [
      { key: PERSONAL, kind: "personal", label: "Personal", folder: "", mark: personal ? "only you" : "" },
      ...syncedTeams.map((team) => {
        const count = countOf(team.id);
        return {
          key: team.id,
          kind: "team" as const,
          label: team.name,
          folder: layout.spaces.find((space) => space.key === team.id)?.folder ?? team.name,
          mark: count === null ? "team" : `team · ${count}`,
        };
      }),
    ];
    const teamRoots = roots.filter((r) => r.kind === "team");
    const unsyncedFolders = layout.teamFolders.filter((folder) => !teamRoots.some((r) => r.folder === folder));
    const waiting = new Set(snapshot.waiting);

    const rootOf = (relPath: string) => {
      const team = teamRoots.find((r) => relPath === r.folder || relPath.startsWith(`${r.folder}/`));
      if (team) return { root: team, path: relPath.slice(team.folder.length + 1) };
      return { root: roots[0], path: relPath };
    };

    const conflictFor = (relPath: string) =>
      snapshot.conflicts.find((c) => c.path === relPath || c.copyPath === relPath) ?? null;

    return {
      active: true,
      roots,
      offline,
      snapshot,
      personal,
      footer: footerCopy({
        conflicts: snapshot.conflicts.length,
        offline,
        retrying: snapshot.retrying,
        waiting: snapshot.waitingCount,
        syncedAt: snapshot.syncedAt,
        personal,
        teams: syncedTeams.map((team) => ({ name: team.name, members: countOf(team.id) })),
      }),
      rootOf,
      conflictFor,
      markOf: (node, selected) => {
        if (selected) return null;
        if (snapshot.conflicts.some((c) => c.copyPath === node.relPath)) return { text: "conflict", tone: "warn" };
        const reason = snapshot.reasons[node.relPath];
        if (reason) return { text: "not synced", tone: "muted", reason };
        if ((offline || snapshot.retrying) && waiting.has(node.relPath)) return { text: "waiting", tone: "warn" };
        const { root: where } = rootOf(node.relPath);
        const syncs = where.kind === "team" || personal;
        if (!syncs) return null;
        if (node.kind === "file" && !isSyncableNote(node.name)) {
          return { text: "not synced", tone: "muted", reason: "Stays on this computer: only notes ending in .md sync." };
        }
        // A team folder that doesn't sync stays on this computer.
        if (where.kind === "personal" && node.kind === "folder" && unsyncedFolders.includes(node.relPath)) {
          return { text: "not synced", tone: "muted" };
        }
        return null;
      },
      keepMine: async (conflict) => {
        await actions.saveNote();
        if (notesRef.current.openPath === conflict.copyPath) await actions.openNote(conflict.path);
        await engine.keepMine(conflict.space, conflict.docId).catch(() => {
          // Offline or saved again meanwhile: the banner stays, with the newer copy.
        });
      },
      useTheirs: async (conflict) => {
        await actions.saveNote();
        const wasOnCopy = notesRef.current.openPath === conflict.copyPath;
        await engine.useTheirs(conflict.space, conflict.docId, onlineRef.current).catch(() => {});
        if (wasOnCopy) await actions.openNote(conflict.path);
      },
      compare: (conflict) =>
        actions.openNote(notesRef.current.openPath === conflict.path ? conflict.copyPath : conflict.path),
    };
    // syncedTeams and personal are covered by layout.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, layout, cloud.online, snapshot, members, userId, actions, engine]);

  return <NotesSyncContext.Provider value={value}>{children}</NotesSyncContext.Provider>;
}
