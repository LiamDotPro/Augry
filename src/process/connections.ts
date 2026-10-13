/**
 * Connections: what processes start from and what their steps can reach. A
 * process refers to a connection by name and never holds a secret, so the
 * settings here keep everything but passwords and tokens, which belong in the
 * system credential store. None of them is connected to anything yet: they
 * all show as not set up, and setting one up comes in a later step.
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
  { key: "password", label: "Password", type: "secret", mono: true, hint: "Kept in your system’s credential store." },
  { key: "access", label: "Steps can", type: "segmented", options: ["Read only", "Read and write"] },
  { key: "rowTrigger", label: "Start a run when a row changes", type: "switch" },
  { key: "showResults", label: "Show query results in Review changes", type: "switch" },
];

const TRACKER: ConnectionField[] = [
  { key: "name", label: "Name", type: "text" },
  { key: "account", label: "Account or workspace", type: "text", mono: true },
  { key: "token", label: "Token", type: "secret", mono: true, hint: "Kept in your system’s credential store." },
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

const notSetUp = (group: ConnectionGroup, icon: string, name: string, desc: string): Connection => ({
  id: icon,
  group,
  icon,
  name,
  on: false,
  status: "Not set up",
  desc,
  used: "",
  settings: {},
});

/** Every kind of connection the app knows. None is set up yet. */
export const CONNECTIONS: Connection[] = [
  notSetUp("work", "github", "GitHub Issues", "Start from issues, comment, and open linked PRs."),
  notSetUp("work", "linear", "Linear", "Start from issues and move them through your workflow states."),
  notSetUp("work", "jira", "Jira", "Start from issues and update their status. Cloud or Data Center."),
  notSetUp("work", "gitlab", "GitLab", "Issues and merge requests for GitLab projects."),
  notSetUp("local", "folder", "Folders", "Start when a file is added to or changed in a folder."),
  notSetUp("local", "schedule", "Schedule", "Start at set times, like weekdays at 09:00."),
  notSetUp("local", "branch", "Git", "Start on commit, push or branch change in a local repo."),
  notSetUp("local", "webhook", "Local webhook", "Accept requests from tools on this computer, or through the relay."),
  notSetUp("systems", "postgresql", "PostgreSQL", "Query a database in a step, or start when a row changes."),
  notSetUp("systems", "mysql", "MySQL", "Query MySQL or MariaDB, local or remote."),
  notSetUp("systems", "sqlite", "SQLite", "Point at a database file, like an app’s local data."),
  notSetUp("systems", "redis", "Redis", "Read keys, or start when a message is published."),
];

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
