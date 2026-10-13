import { useCallback, useEffect, useMemo, useRef, useState, type Dispatch } from "react";
import * as api from "../../cloud/api";
import { useCloud } from "../../cloud/CloudProvider";
import type { Member, Team, TeamProcess } from "../../cloud/types";
import { cloneProcess } from "../../process/json";
import { parseSecretError, scanSecrets, secretWhat } from "../../process/secrets";
import { recordOf, teamOf, type Action, type ProcessState } from "../../process/state";
import { fetchedVersion, firstName, memberNames, readFeed, type TeamCache } from "../../process/team";
import type { Process } from "../../process/types";
import { latest, working } from "../../process/versions";
import { useAppState } from "../../state/AppProvider";

/** How often the selected team's processes are checked while the tab shows them. */
const POLL_MS = 30000;
/** Versions fetched with their content when Versions opens. */
const VERSIONS = 20;

/** The selected team, as the Process tab works with it. Null on Personal. */
export interface TeamApi {
  team: Team;
  online: boolean;
  /** Null until GET /v1/teams/{id} answers. */
  members: Member[] | null;
  lastSyncedAt: number | undefined;
  /** A user's name; you are "You" unless `you` is false (then your own name). */
  nameOf(userId: string | undefined, you?: boolean): string;
  /** Saves a team process's draft as the next version. False when it didn't save. */
  save(processId: string): Promise<boolean>;
  restore(processId: string, version: number): Promise<void>;
  loadVersions(processId: string): void;
  create(process: Process, open?: "editor" | "start"): Promise<void>;
  /** Installs a server template into the team by its slug. */
  addTemplate(slug: string, name: string): Promise<void>;
  duplicate(processId: string): Promise<void>;
  remove(processId: string): Promise<void>;
}

const storageKey = (teamId: string) => `augry.teamProcesses.${teamId}`;

/** A team's cache as last saved on this computer, so it shows offline after a restart. */
function readSaved(teamId: string): { cache: TeamCache; at: number } | null {
  try {
    const raw = localStorage.getItem(storageKey(teamId));
    if (!raw) return null;
    const saved = JSON.parse(raw) as { cache?: TeamCache; at?: number };
    return saved.cache && typeof saved.cache.lastSeq === "number" ? { cache: saved.cache, at: saved.at ?? 0 } : null;
  } catch {
    return null;
  }
}

function writeSaved(teamId: string, cache: TeamCache, at: number) {
  try {
    localStorage.setItem(storageKey(teamId), JSON.stringify({ cache, at }));
  } catch {
    // Storage full or unavailable: the next load fills it again.
  }
}

const membersKey = (teamId: string) => `augry.teamMembers.${teamId}`;

/** A team's members as last seen here, so "<Name> saved v3" still has names offline. */
function readSavedMembers(teamId: string): Member[] | null {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(membersKey(teamId)) ?? "null");
    return Array.isArray(parsed) ? (parsed as Member[]) : null;
  } catch {
    return null;
  }
}

function writeSavedMembers(teamId: string, members: Member[]) {
  try {
    localStorage.setItem(membersKey(teamId), JSON.stringify(members));
  } catch {
    // Unavailable: names show once the server answers.
  }
}

/**
 * Team processes for the Process tab: loads each team's change feed into the
 * reducer (team-page), polls the selected team while the tab shows it, and
 * does the writes (save, restore, create, delete) the views ask for.
 * Everything here is hidden unless signed in with `teams` and `processes`.
 */
export function useTeam(state: ProcessState, dispatch: Dispatch<Action>): TeamApi | null {
  const cloud = useCloud();
  const activeTab = useAppState().activeTab;
  const enabled = cloud.signedIn && cloud.has("teams") && cloud.has("processes");
  const team = enabled ? cloud.selectedTeam : null;
  const teamId = team?.id ?? null;
  const [members, setMembers] = useState<Record<string, Member[]>>({});

  const stateRef = useRef(state);
  stateRef.current = state;
  const cloudRef = useRef(cloud);
  cloudRef.current = cloud;
  const loading = useRef(new Set<string>());

  const note = useCallback((text: string | null) => dispatch({ type: "team-note", note: text }), [dispatch]);

  // What the home shows follows the account chip.
  useEffect(() => dispatch({ type: "space", space: teamId ?? "personal" }), [teamId, dispatch]);

  const load = useCallback(
    async (id: string) => {
      if (loading.current.has(id)) return;
      loading.current.add(id);
      try {
        const since = stateRef.current.teams[id]?.cache.lastSeq ?? 0;
        await readFeed((s) => api.listTeamProcesses(id, s), since, (page) => dispatch({ type: "team-page", teamId: id, page }));
        cloudRef.current.markSynced(id);
      } catch {
        // Offline flips the banner on; a team you left drops out of the list.
      } finally {
        loading.current.delete(id);
      }
    },
    [dispatch],
  );

  /** The cache object last written to this computer, per team. */
  const written = useRef(new Map<string, TeamCache>());

  // Every team once (the account menu shows their counts), from the saved copy first.
  const teamIds = enabled ? cloud.teams.map((t) => t.id).join(",") : "";
  useEffect(() => {
    if (!teamIds) return;
    for (const id of teamIds.split(",")) {
      if (stateRef.current.teams[id]) continue;
      const saved = readSaved(id);
      if (saved) {
        written.current.set(id, saved.cache);
        dispatch({ type: "team-cache", teamId: id, cache: saved.cache });
        if (saved.at) cloudRef.current.markSynced(id, saved.at);
      }
      void load(id);
    }
  }, [teamIds, dispatch, load]);

  // The selected team: its members and a fresh load.
  useEffect(() => {
    if (!teamId) return;
    void load(teamId);
    let cancelled = false;
    const saved = readSavedMembers(teamId);
    if (saved) setMembers((m) => (m[teamId] ? m : { ...m, [teamId]: saved }));
    api
      .getTeam(teamId)
      .then((detail) => {
        if (cancelled) return;
        writeSavedMembers(teamId, detail.members);
        setMembers((m) => ({ ...m, [teamId]: detail.members }));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [teamId, load]);

  // Poll while the Process tab shows the team, and catch up when back online.
  const online = cloud.online;
  const showing = activeTab === "process";
  useEffect(() => {
    if (!teamId || !online || !showing) return;
    void load(teamId);
    const timer = window.setInterval(() => {
      if (!document.hidden) void load(teamId);
    }, POLL_MS);
    return () => window.clearInterval(timer);
  }, [teamId, online, showing, load]);

  // Keep a copy of each team's cache on this computer; it changed because the server just answered.
  useEffect(() => {
    for (const [id, space] of Object.entries(state.teams)) {
      if (written.current.get(id) === space.cache) continue;
      written.current.set(id, space.cache);
      writeSaved(id, space.cache, Date.now());
    }
  }, [state.teams]);

  // Stable, so Versions fetches once when it opens rather than on every poll.
  const loadVersions = useCallback(
    (processId: string) => {
      const tid = teamOf(stateRef.current, processId);
      if (!tid || !cloudRef.current.online) return;
      void (async () => {
        try {
          const list = await api.listTeamProcessVersions(tid, processId);
          const full = await Promise.all(
            list.slice(0, VERSIONS).map((v) => api.getTeamProcessVersion(tid, processId, v.version).catch(() => null)),
          );
          const versions = full.flatMap((v) => (v ? [fetchedVersion(v, processId)] : [])).filter((v) => v !== null);
          dispatch({ type: "team-versions", teamId: tid, processId, versions });
        } catch {
          // The versions already known still show.
        }
      })();
    },
    [dispatch],
  );

  // Counts for the account menu.
  const personalCount = state.records.length;
  const reportProcessCount = cloud.reportProcessCount;
  useEffect(() => reportProcessCount("personal", personalCount), [personalCount, reportProcessCount]);
  useEffect(() => {
    if (!enabled) return;
    for (const [id, space] of Object.entries(state.teams)) reportProcessCount(id, Object.keys(space.cache.processes).length);
  }, [enabled, state.teams, reportProcessCount]);

  const teamMembers = teamId ? (members[teamId] ?? null) : null;
  const names = useMemo(() => memberNames(teamMembers ?? []), [teamMembers]);
  const me = cloud.user;

  return useMemo<TeamApi | null>(() => {
    if (!team) return null;
    const nameOf = (userId: string | undefined, you = true) => {
      if (userId && me && userId === me.id) return you ? "You" : me.name.trim() || me.login;
      return (userId && names[userId]) || "Someone";
    };
    const unreachable = () => note(`Couldn't reach ${team.name} just now. Try again in a moment.`);

    /** The usual failures of a team write, in our words. */
    const failed = (error: unknown, name: string) => {
      if (api.isCloudError(error) && error.offline) return; // The offline banner says it.
      const secret = api.isCloudError(error, 422) ? parseSecretError(error.message) : null;
      if (secret) note(`${name} wasn't saved to ${team.name}: it has what looks like ${secretWhat(secret.rule)} at ${secret.path}. Team processes can't contain keys or tokens.`);
      else if (api.isCloudError(error, 404)) {
        note(`${name} is no longer in ${team.name}.`);
        void load(team.id);
      } else unreachable();
    };

    /** The local secret check before anything goes to the team. */
    const holdsSecret = (process: Process) => {
      const found = scanSecrets(process)[0];
      if (!found) return false;
      note(`${process.name} wasn't saved to ${team.name}: it has what looks like ${secretWhat(found.rule)} at ${found.path}. Team processes can't contain keys or tokens.`);
      return true;
    };

    const create = async (process: Process, open?: "editor" | "start") => {
      const definition = { ...cloneProcess(process), version: 1 };
      if (holdsSecret(definition)) return;
      try {
        const created = await api.createTeamProcess(team.id, definition);
        note(null);
        dispatch({ type: "team-saved", teamId: team.id, process: created, open });
      } catch (error) {
        failed(error, process.name);
      }
    };

    return {
      team,
      online,
      members: teamMembers,
      lastSyncedAt: cloud.lastSyncedAt[team.id],
      nameOf,

      save: async (processId) => {
        const s = stateRef.current;
        const tid = teamOf(s, processId);
        const record = recordOf(s, processId);
        if (!tid || !record?.draft) return true;
        if (!online) return false;
        const base = latest(record).version;
        const definition = { ...cloneProcess(record.draft), version: base + 1 };
        if (holdsSecret(definition)) return false;
        try {
          const saved = await api.updateTeamProcess(tid, processId, definition, base);
          note(null);
          dispatch({ type: "team-saved", teamId: tid, process: saved });
          return true;
        } catch (error) {
          if (api.isCloudError(error, 409) && error.current) {
            dispatch({ type: "team-conflict", teamId: tid, processId, current: error.current as TeamProcess });
          } else failed(error, record.draft.name);
          return false;
        }
      },

      restore: async (processId, version) => {
        const s = stateRef.current;
        const tid = teamOf(s, processId);
        const record = recordOf(s, processId);
        if (!tid || !record || !online) return;
        try {
          const restored = await api.restoreTeamProcess(tid, processId, version, latest(record).version);
          note(null);
          dispatch({ type: "team-saved", teamId: tid, process: restored });
        } catch (error) {
          if (api.isCloudError(error, 409) && error.current) {
            const current = error.current as TeamProcess;
            dispatch({ type: "team-page", teamId: tid, page: { processes: [current], lastSeq: 0, hasMore: false } });
            note(`${firstName(nameOf(current.updatedBy, false))} saved version ${current.version} a moment ago, so nothing was restored.`);
          } else failed(error, working(record).name);
        }
      },

      loadVersions,

      create,

      addTemplate: async (slug, name) => {
        try {
          const installed = await api.installTemplate(slug, team.id);
          note(null);
          dispatch({ type: "team-saved", teamId: team.id, process: installed });
        } catch (error) {
          failed(error, name);
        }
      },

      duplicate: async (processId) => {
        const record = recordOf(stateRef.current, processId);
        if (!record) return;
        const source = working(record);
        await create({ ...cloneProcess(source), name: `${source.name} copy`, from: `process:${source.id}@${latest(record).version}` });
      },

      remove: async (processId) => {
        const s = stateRef.current;
        const tid = teamOf(s, processId);
        const record = recordOf(s, processId);
        if (!tid || !record) return;
        try {
          await api.deleteTeamProcess(tid, processId, latest(record).version);
          note(null);
          dispatch({ type: "team-removed", teamId: tid, processId });
        } catch (error) {
          if (api.isCloudError(error, 409)) {
            note(`Someone changed ${working(record).name} a moment ago, so it wasn't deleted.`);
            void load(tid);
          } else failed(error, working(record).name);
        }
      },
    };
  }, [team, online, teamMembers, names, me, cloud.lastSyncedAt, dispatch, load, note, loadVersions]);
}
