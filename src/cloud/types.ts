// Shapes from the Augry cloud API (LiamDotPro/Terminal-Grid-Cloud,
// libs/api-spec/openapi.yaml). JSON is camelCase; timestamps are UTC ISO
// strings with milliseconds; server ids are UUIDv7 strings.

/** What the server switches on: hide UI for anything not listed. */
export type Feature = "accounts" | "templates" | "teams" | "sync" | "processes" | "connections";

/** GET /v1/me. */
export interface User {
  id: string;
  login: string;
  name: string;
  avatarUrl: string;
  email: string;
  createdAt: string;
}

/** A teammate as other members see them (no email). */
export interface PublicUser {
  id: string;
  login: string;
  name: string;
  avatarUrl: string;
}

export type Role = "owner" | "admin" | "member";

export interface Team {
  id: string;
  name: string;
  slug: string;
  plan: "free" | "pro";
  /** The caller's role. */
  role: Role;
  createdAt: string;
}

export interface Member {
  user: PublicUser;
  role: Role;
  joinedAt: string;
}

export interface TeamDetail extends Team {
  /** In join order. */
  members: Member[];
}

export interface Invite {
  id: string;
  code: string;
  url: string;
  role: "admin" | "member";
  expiresAt: string;
}

/** A signed-in device (GET /v1/sessions). */
export interface DeviceSession {
  id: string;
  deviceName: string;
  createdAt: string;
  lastUsedAt: string;
  expiresAt: string;
  current: boolean;
}

export interface DeviceStart {
  userCode: string;
  verificationUri: string;
  handle: string;
  /** Seconds between polls. */
  interval: number;
  /** Seconds the code works for. */
  expiresIn: number;
}

export interface SessionCreated {
  sessionId: string;
  user: User;
}

/** A team process. `definition` is the augry/process@1 file, byte for byte. */
export interface TeamProcess {
  id: string;
  teamId: string;
  name: string;
  version: number;
  seq: number;
  deleted: boolean;
  templateId?: string;
  templateVersion?: number;
  /** A user id; look the name up in the team's members. */
  updatedBy: string;
  updatedAt: string;
  definition: unknown;
}

export interface ProcessPage {
  processes: TeamProcess[];
  lastSeq: number;
  hasMore: boolean;
}

export interface ProcessVersionInfo {
  version: number;
  name: string;
  createdBy: string;
  createdAt: string;
}

export interface Template {
  slug: string;
  name: string;
  description: string;
  category: string;
  version: number;
  installs: number;
}

/** Who may change or remove a team connection besides owners and admins: nobody, or every member. */
export type TeamConnectionEditors = "admins" | "members";

/**
 * A team connection (cloud epic 005, only used behind the `connections`
 * feature). `settings` is readable by every member; the secret is write only
 * here and never comes back, only `hasSecret`.
 */
export interface TeamConnection {
  id: string;
  teamId: string;
  /**
   * Unique among the team's live connections and set for good on create:
   * processes refer to it as `${conn:<name>}`. Several may share a kind.
   */
  name: string;
  /** The app's catalogue id: "github", "postgresql"… */
  kind: string;
  settings: Record<string, string>;
  hasSecret: boolean;
  editors: TeamConnectionEditors;
  version: number;
  seq: number;
  deleted: boolean;
  createdBy: PublicUser;
  createdAt: string;
  updatedBy: PublicUser;
  updatedAt: string;
}

/** POST /v1/teams/{teamId}/connections. A missing or empty `secret` means none. */
export interface CreateTeamConnectionRequest {
  name: string;
  kind: string;
  settings: Record<string, string>;
  secret?: string;
  editors?: TeamConnectionEditors;
}

/**
 * PUT /v1/teams/{teamId}/connections/{id}. `settings` replace the stored ones.
 * `secret` left out keeps the stored one, `""` removes it, anything else
 * replaces it. `editors` left out keeps it; only owners and admins change it.
 */
export interface SaveTeamConnectionRequest {
  settings: Record<string, string>;
  secret?: string;
  editors?: TeamConnectionEditors;
  baseVersion: number;
}

export interface TeamConnectionPage {
  connections: TeamConnection[];
  lastSeq: number;
  hasMore: boolean;
}

/** A notes sync space: one personal per user, one per team. */
export interface Space {
  id: string;
  kind: "team" | "personal";
  teamId?: string;
  lastSeq: number;
}

export interface SyncDocument {
  /** Chosen by the client (UUID), so a create can be retried. */
  id: string;
  spaceId: string;
  /** Relative to the space root, forward slashes. */
  path: string;
  content: string;
  sha256: string;
  revision: number;
  deleted: boolean;
  updatedBy: string;
  updatedAt: string;
}

export interface Change {
  docId: string;
  path: string;
  revision: number;
  deleted: boolean;
  sha256: string;
}

export interface ChangePage {
  changes: Change[];
  cursor: string;
  hasMore: boolean;
}

/** Which notes sync, chosen in the "Sync your notes?" dialog. */
export interface NotesSyncChoice {
  personal: boolean;
  /** Team ids whose shared folder syncs. */
  teams: string[];
}

/**
 * What the app remembers about the account on this device, in the session
 * file. Never the token: that is in the OS keychain, on the Rust side.
 */
export interface CloudSession {
  /** The signed-in user as last seen, so the chip shows offline too. */
  user: User | null;
  /** Teams as last seen. */
  teams: Team[];
  /** What the Process tab shows: personal processes or a team's. */
  selectedSpace: "personal" | string;
  /** Null until the user answered the sync dialog. */
  notesSync: NotesSyncChoice | null;
  /**
   * The server's features as last heard while signed in, so a launch with no
   * network still shows the team and sync UI (offline) instead of hiding it.
   */
  features?: Feature[];
}

export const EMPTY_CLOUD_SESSION: CloudSession = {
  user: null,
  teams: [],
  selectedSpace: "personal",
  notesSync: null,
};
