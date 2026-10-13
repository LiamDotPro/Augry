/**
 * The cloud API, through the Rust side (src-tauri/src/cloud): the renderer
 * names a path and never sees the session token. Every call either resolves
 * with the parsed body or throws a CloudError. Network failures and 5xx are
 * "offline"; a 401 means this device was signed out. Both are also announced
 * to listeners (the CloudProvider) so the whole app can react.
 */
import { call } from "../ipc/client";
import { asAppError } from "../lib/appError";
import type {
  ChangePage,
  DeviceSession,
  DeviceStart,
  Feature,
  Invite,
  Member,
  ProcessPage,
  ProcessVersionInfo,
  Role,
  SessionCreated,
  Space,
  SyncDocument,
  Team,
  TeamDetail,
  TeamProcess,
  CreateTeamConnectionRequest,
  SaveTeamConnectionRequest,
  TeamConnection,
  TeamConnectionPage,
  Template,
  User,
} from "./types";

export class CloudError extends Error {
  /** HTTP status; 0 when the server could not be reached. */
  readonly status: number;
  /** The API's error code ("conflict", "gone"…), or "offline". */
  readonly code: string;
  /** The 409 body's `current` (a process or a document), when present. */
  readonly current: unknown;

  constructor(status: number, code: string, message: string, current?: unknown) {
    super(message);
    this.name = "CloudError";
    this.status = status;
    this.code = code;
    this.current = current;
  }

  get offline(): boolean {
    return this.status === 0 || this.status >= 500;
  }
}

export function isCloudError(error: unknown, status?: number): error is CloudError {
  return error instanceof CloudError && (status === undefined || error.status === status);
}

export type CloudSignal = "online" | "offline" | "unauthorized";
const listeners = new Set<(signal: CloudSignal) => void>();

/** Hears every call's outcome: back online, offline, signed out. */
export function onCloudSignal(listener: (signal: CloudSignal) => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function signal(value: CloudSignal) {
  for (const listener of listeners) listener(value);
}

interface RawResponse {
  status: number;
  body: unknown;
}

function toError(response: RawResponse): CloudError {
  const body = (response.body ?? {}) as { error?: { code?: string; message?: string }; current?: unknown };
  return new CloudError(
    response.status,
    body.error?.code ?? "unknown",
    body.error?.message ?? `HTTP ${response.status}`,
    body.current,
  );
}

/** Runs a raw command and turns its answer into a body or a CloudError. */
async function settle<T>(run: () => Promise<RawResponse>, { authed }: { authed: boolean }): Promise<T> {
  let response: RawResponse;
  try {
    response = await run();
  } catch (error) {
    const appError = asAppError(error);
    if (appError?.code === "Offline") {
      signal("offline");
      throw new CloudError(0, "offline", appError.message);
    }
    throw error;
  }
  if (response.status >= 500) {
    signal("offline");
    throw toError(response);
  }
  signal("online");
  if (response.status === 401 && authed) signal("unauthorized");
  if (response.status >= 400) throw toError(response);
  return response.body as T;
}

function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  return settle<T>(() => call("cloud_request", { method, path, body: body ?? null }) as Promise<RawResponse>, {
    authed: true,
  });
}

const enc = encodeURIComponent;

// Sign in (the token never leaves Rust) -------------------------------------

export function cloudStatus() {
  return call("cloud_status", {});
}

/** Starts GitHub's device flow; Rust also opens the code page in the browser. */
export function startDeviceFlow(): Promise<DeviceStart> {
  return settle<DeviceStart>(() => call("cloud_device_start", {}) as Promise<RawResponse>, { authed: false });
}

export type DevicePoll =
  | { status: "pending"; interval: number }
  | { status: "done"; session: SessionCreated };

/**
 * One poll. 202 is still waiting; 200 signed in (the token is already in the
 * keychain). Anything else throws: 410 expired, 403 denied, 422 no verified
 * email, 502 GitHub down.
 */
export async function pollDeviceFlow(handle: string): Promise<DevicePoll> {
  let raw: RawResponse | undefined;
  const body = await settle<unknown>(
    async () => {
      raw = (await call("cloud_device_poll", { handle })) as RawResponse;
      return raw;
    },
    { authed: false },
  );
  if (raw?.status === 202) {
    const interval = (body as { interval?: number }).interval ?? 5;
    return { status: "pending", interval };
  }
  return { status: "done", session: body as SessionCreated };
}

export function signOut(): Promise<void> {
  return call("cloud_sign_out", {});
}

export function openUrl(url: string): Promise<void> {
  return call("cloud_open_url", { url });
}

// Account -----------------------------------------------------------------------

export const getMe = () => request<User>("GET", "/v1/me");

export async function getFeatures(): Promise<Feature[]> {
  const body = await request<{ features: Feature[] }>("GET", "/v1/features");
  return body.features;
}

export async function listSessions(): Promise<DeviceSession[]> {
  return (await request<{ sessions: DeviceSession[] }>("GET", "/v1/sessions")).sessions;
}

export const revokeSession = (id: string) => request<void>("DELETE", `/v1/sessions/${enc(id)}`);

// Teams -------------------------------------------------------------------------

export async function listTeams(): Promise<Team[]> {
  return (await request<{ teams: Team[] }>("GET", "/v1/teams")).teams;
}

export const createTeam = (name: string) => request<Team>("POST", "/v1/teams", { name });
export const getTeam = (id: string) => request<TeamDetail>("GET", `/v1/teams/${enc(id)}`);
export const renameTeam = (id: string, name: string) => request<Team>("PATCH", `/v1/teams/${enc(id)}`, { name });
export const deleteTeam = (id: string) => request<void>("DELETE", `/v1/teams/${enc(id)}`);
export const setMemberRole = (teamId: string, userId: string, role: Role) =>
  request<Member>("PATCH", `/v1/teams/${enc(teamId)}/members/${enc(userId)}`, { role });
export const removeMember = (teamId: string, userId: string) =>
  request<void>("DELETE", `/v1/teams/${enc(teamId)}/members/${enc(userId)}`);
export const createInvite = (teamId: string, role: "admin" | "member", expiresInHours: number) =>
  request<Invite>("POST", `/v1/teams/${enc(teamId)}/invites`, { role, expiresInHours });
/** 404 bad code, 410 expired or used, 409 already a member. */
export const acceptInvite = (code: string) => request<Team>("POST", `/v1/invites/${enc(code)}/accept`);

// Team processes ----------------------------------------------------------------

export const listTeamProcesses = (teamId: string, since: number) =>
  request<ProcessPage>("GET", `/v1/teams/${enc(teamId)}/processes?since=${since}`);
export const createTeamProcess = (teamId: string, definition: unknown) =>
  request<TeamProcess>("POST", `/v1/teams/${enc(teamId)}/processes`, { definition });
export const getTeamProcess = (teamId: string, id: string) =>
  request<TeamProcess>("GET", `/v1/teams/${enc(teamId)}/processes/${enc(id)}`);
/** 409 carries the newer process as `current`. */
export const updateTeamProcess = (teamId: string, id: string, definition: unknown, baseVersion: number) =>
  request<TeamProcess>("PUT", `/v1/teams/${enc(teamId)}/processes/${enc(id)}`, { definition, baseVersion });
export const deleteTeamProcess = (teamId: string, id: string, baseVersion: number) =>
  request<void>("DELETE", `/v1/teams/${enc(teamId)}/processes/${enc(id)}?baseVersion=${baseVersion}`);
export async function listTeamProcessVersions(teamId: string, id: string): Promise<ProcessVersionInfo[]> {
  return (
    await request<{ versions: ProcessVersionInfo[] }>("GET", `/v1/teams/${enc(teamId)}/processes/${enc(id)}/versions`)
  ).versions;
}
export const getTeamProcessVersion = (teamId: string, id: string, version: number) =>
  request<ProcessVersionInfo & { definition: unknown }>(
    "GET",
    `/v1/teams/${enc(teamId)}/processes/${enc(id)}/versions/${version}`,
  );
export const restoreTeamProcess = (teamId: string, id: string, version: number, baseVersion: number) =>
  request<TeamProcess>("POST", `/v1/teams/${enc(teamId)}/processes/${enc(id)}/restore`, { version, baseVersion });

export async function listTemplates(): Promise<Template[]> {
  return (await request<{ templates: Template[] }>("GET", "/v1/templates")).templates;
}
/** One template with its definition, in the cloud's lanes format (schemaVersion 1). */
export const getTemplate = (slug: string) => request<Template & { definition: unknown }>("GET", `/v1/templates/${enc(slug)}`);
export const installTemplate = (slug: string, teamId: string) =>
  request<TeamProcess>("POST", `/v1/templates/${enc(slug)}/install`, { teamId });

// Team connections (cloud epic 005, behind the "connections" feature) -----------

/** The team's connection feed: changes after `since`, tombstones included, 200 a page. */
export const listTeamConnections = (teamId: string, since: number) =>
  request<TeamConnectionPage>("GET", `/v1/teams/${enc(teamId)}/connections?since=${since}`);
/**
 * Owners and admins only (403 for members). 409 when a live connection has
 * the name (a plain error, no `current`), 400 past 100 live connections, 422
 * when a setting looks like a token.
 */
export const createTeamConnection = (teamId: string, body: CreateTeamConnectionRequest) =>
  request<TeamConnection>("POST", `/v1/teams/${enc(teamId)}/connections`, body);
/**
 * Owners and admins, or any member when its `editors` is "members" (403
 * otherwise, and for a member who sends a different `editors`). 409 carries
 * the newer connection as `current`.
 */
export const saveTeamConnection = (teamId: string, id: string, body: SaveTeamConnectionRequest) =>
  request<TeamConnection>("PUT", `/v1/teams/${enc(teamId)}/connections/${enc(id)}`, body);
/** Who may: as for saving. Also clears its secret. 409 carries the stored connection as `current`. */
export const deleteTeamConnection = (teamId: string, id: string, baseVersion: number) =>
  request<void>("DELETE", `/v1/teams/${enc(teamId)}/connections/${enc(id)}?baseVersion=${baseVersion}`);

// Notes sync --------------------------------------------------------------------

export const getPersonalSpace = () => request<Space>("GET", "/v1/me/space");
export const getTeamSpace = (teamId: string) => request<Space>("GET", `/v1/teams/${enc(teamId)}/space`);
/** 410 means the cursor is ahead of the server: resync from an empty cursor. */
export const listChanges = (spaceId: string, cursor: string, limit = 500) =>
  request<ChangePage>("GET", `/v1/spaces/${enc(spaceId)}/changes?cursor=${enc(cursor)}&limit=${limit}`);
/** At most 100 ids. */
export async function getDocuments(spaceId: string, ids: string[]): Promise<SyncDocument[]> {
  return (
    await request<{ documents: SyncDocument[] }>("GET", `/v1/spaces/${enc(spaceId)}/documents?ids=${ids.map(enc).join(",")}`)
  ).documents;
}
/** baseRevision 0 creates. 409 carries the newer document as `current`. */
export const putDocument = (spaceId: string, docId: string, path: string, content: string, baseRevision: number) =>
  request<SyncDocument>("PUT", `/v1/spaces/${enc(spaceId)}/documents/${enc(docId)}`, { path, content, baseRevision });
export const deleteDocument = (spaceId: string, docId: string, baseRevision: number) =>
  request<SyncDocument>("DELETE", `/v1/spaces/${enc(spaceId)}/documents/${enc(docId)}?baseRevision=${baseRevision}`);

/** Any reachable call, to find out whether the server is back. */
export const ping = () => request<unknown>("GET", "/v1/features");
