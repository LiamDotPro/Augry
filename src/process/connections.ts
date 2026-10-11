/**
 * Connections: what processes start from and what their steps can reach. A
 * process refers to a connection by name and never holds a secret, so the
 * settings here keep everything but passwords and tokens, which the real app
 * puts in the system credential store. Test connection is simulated.
 */

import type { Connection, ConnectionGroup } from "./types";

export const CONNECTION_GROUPS: { id: ConnectionGroup; title: string; hint: string }[] = [
  { id: "work", title: "Project management", hint: "Start runs from tickets and update them as steps finish" },
  { id: "local", title: "On this computer", hint: "Start runs from things that happen locally. Nothing leaves this computer" },
  { id: "systems", title: "Systems and data", hint: "What steps can query or call. Read only unless you allow writes" },
];

/** A setting in the side panel. Secrets are typed in but never stored here. */
export interface ConnectionField {
  key: string;
  label: string;
  type: "text" | "secret" | "segmented" | "switch";
  options?: string[];
  mono?: boolean;
  hint?: string;
}

const DATABASE: ConnectionField[] = [
  { key: "name", label: "Name", type: "text", hint: "Processes use it as db.local-dev." },
  { key: "host", label: "Host and port", type: "text", mono: true },
  { key: "database", label: "Database", type: "text", mono: true },
  { key: "user", label: "User", type: "text", mono: true },
  { key: "password", label: "Password", type: "secret", mono: true, hint: "Stored in Windows Credential Manager." },
  { key: "access", label: "Steps can", type: "segmented", options: ["Read only", "Read and write"] },
  { key: "rowTrigger", label: "Start a run when a row changes", type: "switch" },
  { key: "showResults", label: "Show query results in Review changes", type: "switch" },
];

const TRACKER: ConnectionField[] = [
  { key: "name", label: "Name", type: "text" },
  { key: "account", label: "Account or workspace", type: "text", mono: true },
  { key: "token", label: "Token", type: "secret", mono: true, hint: "Stored in Windows Credential Manager." },
  { key: "access", label: "Steps can", type: "segmented", options: ["Read only", "Comment and update"] },
];

const LOCAL: Record<string, ConnectionField[]> = {
  folder: [
    { key: "folders", label: "Watched folders", type: "text", mono: true, hint: "One per line. Each process picks its own folder and files." },
    { key: "settle", label: "Wait until a file stops changing", type: "switch" },
  ],
  schedule: [
    { key: "zone", label: "Time zone", type: "text", hint: "Schedules use this computer’s clock." },
    { key: "missed", label: "If the computer was off", type: "segmented", options: ["Skip it", "Run when it wakes"] },
  ],
  branch: [{ key: "repos", label: "Repositories", type: "text", mono: true, hint: "Local repos processes can start from." }],
  webhook: [
    { key: "port", label: "Port", type: "text", mono: true },
    { key: "reach", label: "Reachable from", type: "segmented", options: ["This computer", "Relay"] },
  ],
};

export function connectionFields(connection: Connection): ConnectionField[] {
  if (connection.group === "systems") return DATABASE;
  if (connection.group === "work") return TRACKER;
  return LOCAL[connection.icon] ?? [];
}

const conn = (
  group: ConnectionGroup,
  icon: string,
  name: string,
  on: boolean,
  status: string,
  desc: string,
  used: string,
  settings: Record<string, string> = {},
): Connection => ({ id: icon, group, icon, name, on, status, desc, used, settings });

export function seedConnections(): Connection[] {
  return [
    conn("work", "github", "GitHub Issues", true, "Connected · liam-dev", "Start from issues, comment, and open linked PRs.", "Used by Feature delivery", {
      name: "GitHub", account: "liam-dev", access: "Comment and update",
    }),
    conn("work", "linear", "Linear", true, "Connected · Augry workspace", "Start from issues and move them through your workflow states.", "Not used yet", {
      name: "Linear", account: "augry", access: "Read only",
    }),
    conn("work", "jira", "Jira", false, "Not set up", "Start from issues and update their status. Cloud or Data Center.", "", { name: "Jira", access: "Read only" }),
    conn("work", "gitlab", "GitLab", false, "Not set up", "Issues and merge requests for GitLab projects.", "", { name: "GitLab", access: "Read only" }),
    conn("local", "folder", "Folders", true, "1 watched · ~/crash-dumps", "Start when a file is added to or changed in a folder.", "Used by Crash report", {
      folders: "~/crash-dumps", settle: "on",
    }),
    conn("local", "schedule", "Schedule", true, "On · uses this computer’s clock", "Start at set times, like weekdays at 09:00.", "Used by Dependency update", {
      zone: "Europe/London", missed: "Skip it",
    }),
    conn("local", "branch", "Git", true, "3 local repos", "Start on commit, push or branch change in a local repo.", "Not used yet", {
      repos: "~/code/augry\n~/code/site\n~/code/notes",
    }),
    conn("local", "webhook", "Local webhook", true, "Listening · localhost:4319", "Accept requests from tools on this computer, or through the relay.", "Used by Bug fix", {
      port: "4319", reach: "Relay",
    }),
    conn("systems", "postgresql", "PostgreSQL", true, "Connected · localhost:5432", "Query a database in a step, or start when a row changes.", "Used by Bug fix", {
      name: "Local dev database", host: "localhost:5432", database: "terminal_grid_dev", user: "liam", access: "Read only", rowTrigger: "off", showResults: "on",
    }),
    conn("systems", "mysql", "MySQL", false, "Not set up", "Query MySQL or MariaDB, local or remote.", "", { access: "Read only" }),
    conn("systems", "sqlite", "SQLite", false, "Not set up", "Point at a database file, like an app’s local data.", "", { access: "Read only" }),
    conn("systems", "redis", "Redis", false, "Not set up", "Read keys, or start when a message is published.", "", { access: "Read only" }),
  ];
}

/** Brand colours of the connection tiles (the design's, for the dark theme). */
export const CONNECTION_COLORS: Record<string, string> = {
  github: "currentColor",
  linear: "#8f97f2",
  jira: "#4c9aff",
  gitlab: "#fc6d26",
  postgresql: "#7ea2ee",
  mysql: "#7fb0de",
  sqlite: "#78c0e8",
  redis: "#ff5f52",
};

/**
 * The simulated Test connection. A database or tracker needs its address (and
 * for a tracker, an account) filled in; local connections always pass.
 */
export function testConnection(connection: Connection): { ok: boolean; text: string } {
  const s = connection.settings;
  if (connection.group === "systems") {
    if (!s.host?.trim()) return { ok: false, text: "Add a host and port first" };
    if (!/^[\w.-]+:\d+$/.test(s.host.trim())) return { ok: false, text: `Could not reach ${s.host.trim()}` };
    const label = connection.icon === "postgresql" ? "PostgreSQL 16.2" : connection.name;
    return { ok: true, text: `Connected · ${label} · 38 ms` };
  }
  if (connection.group === "work") {
    if (!s.account?.trim()) return { ok: false, text: "Add an account or workspace first" };
    return { ok: true, text: `Connected · ${s.account.trim()}` };
  }
  return { ok: true, text: "Working · nothing leaves this computer" };
}

/** The card's status line once a test has passed and the connection is saved. */
export function connectedStatus(connection: Connection): string {
  const s = connection.settings;
  if (connection.group === "systems") return `Connected · ${s.host ?? ""}`;
  if (connection.group === "work") return `Connected · ${s.account ?? ""}`;
  return connection.status;
}
