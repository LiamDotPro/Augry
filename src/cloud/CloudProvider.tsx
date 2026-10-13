/**
 * The optional Augry account: who is signed in, the server's features, the
 * teams, which space the Process tab shows, whether the server is reachable,
 * and which cloud dialog is open. Everything works signed out; this only adds
 * teams and sync on top.
 *
 * The session token never reaches the renderer (src-tauri/src/cloud). What
 * should survive a restart (the user as last seen, the teams, the selected
 * space, the notes sync choice) is kept in the app session through
 * AppActions.setCloudSession.
 */
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { call, on } from "../ipc/client";
import { EVENTS } from "../ipc/types";
import { useAppActions, useAppState } from "../state/AppProvider";
import * as api from "./api";
import { inviteFromDeepLink } from "./invite";
import type { Feature, NotesSyncChoice, SessionCreated, Team, User } from "./types";

/** The cloud dialogs, by the design's screen ids (App Shell board 07 and 08, Notes board 02). */
export type CloudDialog =
  /** sign-in → sign-in-code → sign-in-expired. `joinCode` is accepted straight after. */
  | { kind: "sign-in"; joinCode?: string }
  /** team-join and team-join-error. `errorStatus` shows the error for that status. */
  | { kind: "join"; code?: string; errorStatus?: number }
  /** team-join-signed-out: an invite link arrived while signed out. */
  | { kind: "join-signed-out"; code: string }
  /** team-joined. */
  | { kind: "joined"; team: Team }
  /** team-create. */
  | { kind: "create-team" }
  /** notes-sync-setup. */
  | { kind: "notes-sync-setup" };

/** Personal processes, or a team's by id. */
export type SpaceId = "personal" | string;

export interface CloudContextValue {
  /** The first status check is done. */
  ready: boolean;
  user: User | null;
  signedIn: boolean;
  features: Feature[];
  has(feature: Feature): boolean;
  teams: Team[];
  selectedSpace: SpaceId;
  /** The selected team, or null on Personal. */
  selectedTeam: Team | null;
  /** False after a network error or a 5xx, until a call gets through again. */
  online: boolean;
  /** When each space ("personal", a team id, or a notes space key) last synced, epoch ms. */
  lastSyncedAt: Record<string, number>;
  /** This device was signed out by the server (a 401): Settings says so once. */
  signedOutNotice: boolean;
  /** This computer's name, as the devices list shows it. */
  deviceName: string;
  dialog: CloudDialog | null;
  /** How many processes each space has, as the Process tab last counted them. */
  processCounts: Record<SpaceId, number>;
  notesSync: NotesSyncChoice | null;

  openDialog(dialog: CloudDialog): void;
  closeDialog(): void;
  /** After the device flow returns a session (the token is already stored). */
  completeSignIn(session: SessionCreated): Promise<void>;
  signOut(): Promise<void>;
  selectSpace(space: SpaceId): void;
  refreshTeams(): Promise<Team[]>;
  createTeam(name: string): Promise<Team>;
  /** Accepts an invite and selects nothing yet; throws a CloudError (404, 409, 410). */
  joinTeam(code: string): Promise<Team>;
  setNotesSync(choice: NotesSyncChoice): void;
  markSynced(key: string, at?: number): void;
  reportProcessCount(space: SpaceId, count: number): void;
  dismissSignedOutNotice(): void;
}

const CloudContext = createContext<CloudContextValue | null>(null);

export function useCloud(): CloudContextValue {
  const value = useContext(CloudContext);
  if (!value) throw new Error("useCloud outside CloudProvider");
  return value;
}

/** Retry delays while offline: quick at first, then once a minute. */
const BACKOFF_MS = [2000, 5000, 10000, 20000, 40000, 60000];

export function CloudProvider({ children }: { children: ReactNode }) {
  const app = useAppState();
  const appActions = useAppActions();
  const session = app.cloud;
  const setSession = appActions.setCloudSession;

  const [ready, setReady] = useState(false);
  const [signedIn, setSignedIn] = useState(false);
  /** Null until the server answered this launch. */
  const [heardFeatures, setHeardFeatures] = useState<Feature[] | null>(null);
  const [online, setOnline] = useState(true);
  const [lastSyncedAt, setLastSyncedAt] = useState<Record<string, number>>({});
  const [signedOutNotice, setSignedOutNotice] = useState(false);
  const [deviceName, setDeviceName] = useState("");
  const [dialog, setDialog] = useState<CloudDialog | null>(null);
  const [processCounts, setProcessCounts] = useState<Record<SpaceId, number>>({});

  const signedInRef = useRef(false);
  signedInRef.current = signedIn;
  const sessionRef = useRef(session);
  sessionRef.current = session;

  const refreshFeatures = useCallback(async () => {
    try {
      const features = await api.getFeatures();
      setHeardFeatures(features);
      if (signedInRef.current) setSession({ features });
    } catch {
      // Offline: keep what we had.
    }
  }, [setSession]);

  const refreshTeams = useCallback(async () => {
    const teams = await api.listTeams();
    const selected = sessionRef.current.selectedSpace;
    setSession({
      teams,
      // A team you left or that was deleted falls back to Personal.
      ...(selected !== "personal" && !teams.some((t) => t.id === selected) ? { selectedSpace: "personal" } : {}),
    });
    return teams;
  }, [setSession]);

  /** Reloads the user, features and teams; quiet when offline. */
  const refreshAccount = useCallback(async () => {
    await refreshFeatures();
    if (!signedInRef.current) return;
    try {
      const [user] = await Promise.all([api.getMe(), refreshTeams()]);
      setSession({ user });
    } catch {
      // Offline or signed out: the signals below handle both.
    }
  }, [refreshFeatures, refreshTeams, setSession]);

  const forgetAccount = useCallback(() => {
    setSignedIn(false);
    setSession({ user: null, teams: [], selectedSpace: "personal", features: [] });
    setProcessCounts((counts) => ({ personal: counts.personal ?? 0 }));
  }, [setSession]);

  // Startup: once the session file is loaded, ask Rust whether a token exists.
  useEffect(() => {
    if (!app.ready) return;
    let cancelled = false;
    void (async () => {
      try {
        const status = await api.cloudStatus();
        if (cancelled) return;
        setDeviceName(status.deviceName);
        setSignedIn(status.signedIn);
        signedInRef.current = status.signedIn;
        // No token but a remembered user: this device was signed out meanwhile.
        if (!status.signedIn && sessionRef.current.user) forgetAccount();
      } catch {
        // No cloud module (e.g. a browser preview): stay signed out.
      }
      if (cancelled) return;
      setReady(true);
      await refreshAccount();
    })();
    return () => {
      cancelled = true;
    };
    // Runs once per launch, after the session is restored.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [app.ready]);

  // Online, offline and signed out, as every call reports them.
  const wasOffline = useRef(false);
  useEffect(
    () =>
      api.onCloudSignal((signal) => {
        if (signal === "offline") {
          wasOffline.current = true;
          setOnline(false);
        } else if (signal === "online") {
          setOnline(true);
          if (wasOffline.current) {
            wasOffline.current = false;
            void refreshAccount();
          }
        } else if (signal === "unauthorized" && signedInRef.current) {
          forgetAccount();
          setSignedOutNotice(true);
          setDialog(null);
          void refreshFeatures();
        }
      }),
    [forgetAccount, refreshAccount, refreshFeatures],
  );

  // While offline, try again with backoff; the first answer brings it back.
  useEffect(() => {
    if (online || !ready) return;
    let attempt = 0;
    let timer = 0;
    const retry = () => {
      timer = window.setTimeout(() => {
        void api.ping().catch(() => {
          attempt = Math.min(attempt + 1, BACKOFF_MS.length - 1);
          retry();
        });
      }, BACKOFF_MS[attempt]);
    };
    retry();
    return () => window.clearTimeout(timer);
  }, [online, ready]);

  // augry://join/<code> links, from a cold start or while running.
  useEffect(() => {
    if (!ready) return;
    const take = async () => {
      let urls: string[] = [];
      try {
        urls = await call("cloud_take_deep_links", {});
      } catch {
        return;
      }
      for (const url of urls) {
        const code = inviteFromDeepLink(url);
        if (!code) continue;
        setDialog(signedInRef.current ? { kind: "join", code } : { kind: "join-signed-out", code });
      }
    };
    void take();
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void on<string[]>(EVENTS.deepLink, () => void take()).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [ready]);

  const joinTeam = useCallback(
    async (code: string) => {
      const team = await api.acceptInvite(code);
      await refreshTeams().catch(() => setSession({ teams: [...sessionRef.current.teams, team] }));
      return team;
    },
    [refreshTeams, setSession],
  );

  const completeSignIn = useCallback(
    async (created: SessionCreated) => {
      const joinCode = dialog?.kind === "sign-in" ? dialog.joinCode : undefined;
      setSignedIn(true);
      signedInRef.current = true;
      setSignedOutNotice(false);
      setSession({ user: created.user });
      setDialog(null);
      await refreshFeatures();
      await refreshTeams().catch(() => {});
      if (joinCode) {
        try {
          const team = await joinTeam(joinCode);
          setDialog({ kind: "joined", team });
        } catch (error) {
          // The join dialog words it: expired, wrong code, already a member.
          setDialog({ kind: "join", code: joinCode, errorStatus: api.isCloudError(error) ? error.status : 0 });
        }
        return;
      }
      // Asked once after sign in.
      if (sessionRef.current.notesSync === null) setDialog({ kind: "notes-sync-setup" });
    },
    [dialog, joinTeam, refreshFeatures, refreshTeams, setSession],
  );

  const signOut = useCallback(async () => {
    try {
      await api.signOut();
    } finally {
      forgetAccount();
      setSignedOutNotice(false);
      void refreshFeatures();
    }
  }, [forgetAccount, refreshFeatures]);

  const value = useMemo<CloudContextValue>(() => {
    // Started offline: go by what the server listed last time.
    const features = heardFeatures ?? (signedIn ? (session.features ?? []) : []);
    const selectedTeam =
      session.selectedSpace === "personal" ? null : (session.teams.find((t) => t.id === session.selectedSpace) ?? null);
    return {
      ready,
      user: signedIn ? session.user : null,
      signedIn: signedIn && session.user !== null,
      features,
      has: (feature) => features.includes(feature),
      teams: signedIn ? session.teams : [],
      selectedSpace: selectedTeam ? selectedTeam.id : "personal",
      selectedTeam: signedIn ? selectedTeam : null,
      online,
      lastSyncedAt,
      signedOutNotice,
      deviceName,
      dialog,
      processCounts,
      notesSync: session.notesSync,

      openDialog: setDialog,
      closeDialog: () => setDialog(null),
      completeSignIn,
      signOut,
      selectSpace: (space) => setSession({ selectedSpace: space }),
      refreshTeams,
      createTeam: async (name) => {
        const team = await api.createTeam(name.trim());
        setSession({ teams: [...sessionRef.current.teams.filter((t) => t.id !== team.id), team] });
        void refreshTeams().catch(() => {});
        return team;
      },
      joinTeam,
      setNotesSync: (choice) => setSession({ notesSync: choice }),
      markSynced: (key, at = Date.now()) => setLastSyncedAt((current) => ({ ...current, [key]: at })),
      reportProcessCount: (space, count) =>
        setProcessCounts((current) => (current[space] === count ? current : { ...current, [space]: count })),
      dismissSignedOutNotice: () => setSignedOutNotice(false),
    };
  }, [
    ready,
    signedIn,
    session,
    heardFeatures,
    online,
    lastSyncedAt,
    signedOutNotice,
    deviceName,
    dialog,
    processCounts,
    completeSignIn,
    signOut,
    refreshTeams,
    joinTeam,
    setSession,
  ]);

  return <CloudContext.Provider value={value}>{children}</CloudContext.Provider>;
}
