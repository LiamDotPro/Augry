import { Fragment, useCallback, useEffect, useMemo, useState } from "react";
import { isCloudError } from "../../cloud/api";
import { useCloud } from "../../cloud/CloudProvider";
import type { PublicUser, TeamConnection, TeamConnectionEditors } from "../../cloud/types";
import { connectionFields, type ConnectionField } from "../../process/connections";
import { savedAgo } from "../../process/team";
import {
  canChangeEditors,
  canCreate,
  canEdit,
  catalogOf,
  connectionErrorMessage,
  connectionName,
  connectionRef,
  createTeamConnection,
  isKnownKind,
  isValidConnectionName,
  loadTeamConnections,
  MAX_LIVE_CONNECTIONS,
  NAME_RULE,
  removeTeamConnection,
  saveTeamConnection,
  secretProblem,
  settingsProblem,
  teamConnectionSections,
  type ErrorContext,
} from "../../process/teamConnections";
import type { Connection } from "../../process/types";
import { cx } from "../../lib/cx";
import { TeamTile } from "../Account";
import { ComingNote, ConnectionCard, GROUP_NAME, Tile } from "./ConnectionsView";
import { InspectorField, type Field } from "./InspectorField";
import { ProcessIcon } from "./ProcessIcon";
import { useProcess } from "./store";
import type { TeamApi } from "./useTeam";
import { useKeptScroll } from "./useKeptScroll";

const STORED = "••••••••••••";
const EDITORS = { admins: "Owners and admins", members: "Everyone" } as const;

/** The side panel: one of the team's connections by id, or a new one of a kind. */
type Open = { id: string } | { kind: string } | null;

/**
 * Connections with a team selected (board 11, team-connections): the team's
 * own connections, one set of credentials everyone runs with, then what
 * stays on this computer. A team may have several of a kind, each with its
 * own name. Until the server lists the `connections` feature, the team's
 * kinds read "Not set up for the team" and nothing can be changed.
 */
export function TeamConnectionsView() {
  const { state, dispatch, now, team } = useProcess();
  const cloud = useCloud();
  const enabled = cloud.has("connections");
  const [query, setQuery] = useState("");
  const [list, setList] = useState<TeamConnection[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  /** The team card open in the side panel. Local cards use the tab's own panel. */
  const [open, setOpen] = useState<Open>(null);
  const scroll = useKeptScroll<HTMLDivElement>("connections");
  const teamId = team?.team.id ?? "";
  const teamName = team?.team.name ?? "";

  const reload = useCallback(() => {
    loadTeamConnections(teamId, enabled).then(
      (next) => {
        setList(next);
        setLoadError(null);
      },
      (error: unknown) => setLoadError(connectionErrorMessage(error, { action: "load", teamName })),
    );
  }, [teamId, enabled, teamName]);
  useEffect(() => {
    setList(null);
    setLoadError(null);
    setOpen(null);
    reload();
  }, [reload]);

  const me = cloud.user?.id;
  const addedBy = useCallback((user: PublicUser) => (user.id === me ? "you" : user.name.trim() || user.login), [me]);
  const sections = useMemo(() => teamConnectionSections(state.connections, list, teamName, addedBy), [state.connections, list, teamName, addedBy]);
  if (!team) return null;

  const role = team.team.role;
  const ready = enabled && !!list;
  const atLimit = !!list && list.length >= MAX_LIVE_CONNECTIONS;
  const mayAdd = ready && canCreate(role);
  const q = query.trim().toLowerCase();
  const existing = open && "id" in open ? (list?.find((c) => c.id === open.id) ?? null) : null;
  const newKind = open && "kind" in open ? open.kind : null;
  const pick = (next: Open) => {
    dispatch({ type: "connection-open", id: null });
    setOpen(next);
  };
  const firstUnset = sections.flatMap((s) => s.cards).find((c) => !c.mine && !c.team)?.local.icon ?? "github";

  return (
    <div className={cx("process-connections", "process-connections--team", (existing || newKind) && "process-connections--panel")}>
      <div className="process-home__main process-connections__main" ref={scroll.ref} onScroll={scroll.onScroll}>
        <div className="process-home__head process-connections__head">
          <div className="process-home__titles">
            <button type="button" className="process-backlink" onClick={() => dispatch({ type: "navigate", route: { view: "home" } })}>
              <span aria-hidden="true">‹</span>Processes
            </button>
            <h1 className="process-home__title">Connections</h1>
            <p className="process-home__sub">
              {teamName}’s connections. Everyone in the team runs with the same credentials, and can use them without seeing them.
              {ready && !canCreate(role) && " Owners and admins add them."}
            </p>
          </div>
          <input
            className="process-input process-connections__search"
            type="search"
            placeholder="Search connections"
            aria-label="Search connections"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          {mayAdd && (
            <button
              type="button"
              className="process-btn"
              disabled={atLimit || !team.online}
              title={atLimit ? `${teamName} has ${MAX_LIVE_CONNECTIONS} connections, the most a team can have.` : undefined}
              onClick={() => pick({ kind: firstUnset })}
            >
              <span className="process-btn__plus" aria-hidden="true">+</span>
              Add team connection
            </button>
          )}
        </div>
        {!enabled && <ComingNote />}
        {enabled && loadError && (
          <div className="process-coming" role="status">
            <ProcessIcon name="plug" size={16} />
            <span>{loadError}</span>
          </div>
        )}
        {mayAdd && atLimit && (
          <div className="process-coming" role="note">
            <ProcessIcon name="plug" size={16} />
            <span>
              {teamName} has {MAX_LIVE_CONNECTIONS} connections, the most a team can have. Remove one to add another.
            </span>
          </div>
        )}
        {sections.map((section) => {
          const cards = section.cards.filter((c) => !q || `${c.title} ${c.local.desc}`.toLowerCase().includes(q));
          if (!cards.length) return null;
          return (
            <section key={section.id} className="process-connections__group">
              <div className="process-home__section">
                <h2 className="process-home__h2">{section.title}</h2>
                <span className="process-home__hint">{section.hint}</span>
              </div>
              <div className="process-connections__grid">
                {cards.map((card) => {
                  if (card.mine) {
                    return (
                      <ConnectionCard
                        key={card.key}
                        connection={card.local}
                        picked={false}
                        onOpen={() => {}}
                        // Your own connections can't be set up yet either.
                        view={{ on: false, status: "Not set up", used: card.used, action: null }}
                      />
                    );
                  }
                  const shown = { ...card.local, name: card.title };
                  if (card.team) {
                    const connection = card.team;
                    return (
                      <ConnectionCard
                        key={card.key}
                        connection={shown}
                        picked={existing?.id === connection.id}
                        onOpen={() => pick({ id: connection.id })}
                        view={{ on: true, status: card.status, used: card.used, action: ready ? (canEdit(connection, role) ? "Manage" : "View") : null }}
                      />
                    );
                  }
                  return (
                    <ConnectionCard
                      key={card.key}
                      connection={shown}
                      picked={newKind === card.local.icon}
                      onOpen={() => pick({ kind: card.local.icon })}
                      // Only owners and admins add one, and not past the team's limit.
                      view={{ on: false, status: card.status, used: "", action: mayAdd && !atLimit ? "Set up" : null }}
                    />
                  );
                })}
              </div>
            </section>
          );
        })}
      </div>
      {list && (existing || newKind) && (
        <TeamSetupPanel
          key={existing ? existing.id : `new:${newKind}`}
          existing={existing}
          initialKind={existing?.kind ?? newKind!}
          catalog={state.connections}
          team={team}
          list={list}
          now={now}
          addedBy={addedBy}
          onClose={() => pick(null)}
          onCreated={(created) => {
            // Shown at once; the reload that follows brings the rest.
            setList((l) => (l ? [...l.filter((c) => c.id !== created.id), created] : l));
            setOpen({ id: created.id });
          }}
          onChanged={reload}
        />
      )}
    </div>
  );
}

/** The fields a kind's panel shows. The connection's own name replaces the local "Name" setting. */
function panelFields(kind: string, catalog: readonly Connection[], settings: Record<string, string>): ConnectionField[] {
  if (!isKnownKind(kind, catalog)) return Object.keys(settings).map((key) => ({ key, label: key, type: "text", mono: true }));
  return connectionFields(catalogOf(kind, catalog)).filter((f) => f.type !== "switch" && f.key !== "name");
}

/** A new connection starts with the first choice of each segmented setting. */
function defaultSettings(kind: string, catalog: readonly Connection[]): Record<string, string> {
  return Object.fromEntries(panelFields(kind, catalog, {}).flatMap((f) => (f.type === "segmented" ? [[f.key, f.options![0]!]] : [])));
}

/**
 * A team connection in the side panel (team-connection-setup): who it is
 * shared with and who added and changed it, its name, its settings, a
 * password that is never shown, and who can change it. Owners and admins
 * add connections and choose who can change them; a member changes or
 * removes one only when it allows everyone, and otherwise sees it read only.
 */
function TeamSetupPanel({
  existing,
  initialKind,
  catalog,
  team,
  list,
  now,
  addedBy,
  onClose,
  onCreated,
  onChanged,
}: {
  existing: TeamConnection | null;
  initialKind: string;
  catalog: readonly Connection[];
  team: TeamApi;
  list: TeamConnection[];
  now: number;
  addedBy(user: PublicUser): string;
  onClose(): void;
  onCreated(connection: TeamConnection): void;
  onChanged(): void;
}) {
  const teamName = team.team.name;
  const role = team.team.role;
  const [kind, setKind] = useState(initialKind);
  const c = catalogOf(kind, catalog);
  const editable = team.online && (existing ? canEdit(existing, role) : canCreate(role));
  const editorsEditable = editable && canChangeEditors(role);
  const [settings, setSettings] = useState<Record<string, string>>(() => existing?.settings ?? defaultSettings(kind, catalog));
  const [name, setName] = useState(() => existing?.name ?? connectionName(kind, list));
  const [nameTouched, setNameTouched] = useState(false);
  const [secret, setSecret] = useState("");
  /** "Remove the stored password": the save sends an empty secret. */
  const [clearSecret, setClearSecret] = useState(false);
  const [editors, setEditors] = useState<TeamConnectionEditors>(existing?.editors ?? "admins");
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);

  const defs = panelFields(kind, catalog, settings);
  const secretDef = defs.find((f) => f.type === "secret") ?? { key: "", label: "Password or token", type: "secret" as const, mono: true };
  if (!defs.includes(secretDef)) defs.push(secretDef);
  const secretWord = secretDef.label.toLowerCase();
  const labelOf = (key: string) => defs.find((f) => f.key === key)?.label ?? key;
  const errorContext = (action: ErrorContext["action"]): ErrorContext => ({ action, teamName, name, labelOf, secretLabel: `the ${secretDef.label} field` });

  const nameProblem = existing
    ? null
    : !isValidConnectionName(name)
      ? NAME_RULE
      : list.some((t) => !t.deleted && t.name === name)
        ? `${teamName} already has a connection named ${name}.`
        : null;
  const problem = nameProblem ?? settingsProblem(settings, labelOf, `the ${secretDef.label} field`) ?? secretProblem(secret);
  const dirty =
    !existing || secret !== "" || clearSecret || editors !== existing.editors || JSON.stringify(settings) !== JSON.stringify(existing.settings);

  const changeKind = (label: string) => {
    const next = catalog.find((k) => k.group !== "local" && k.name === label);
    if (!next || next.icon === kind) return;
    setKind(next.icon);
    setSettings(defaultSettings(next.icon, catalog));
    if (!nameTouched) setName(connectionName(next.icon, list));
  };

  const fields: Field[] = [];
  if (!existing) {
    const kinds = catalog.filter((k) => k.group !== "local");
    fields.push({
      type: "select",
      label: "Kind",
      icon: c.icon,
      value: c.name,
      detail: GROUP_NAME[c.group],
      options: editable ? kinds.map((k) => ({ value: k.name, detail: GROUP_NAME[k.group] })) : undefined,
      onChange: editable ? changeKind : undefined,
    });
  }
  fields.push({
    type: "text",
    label: "Name",
    mono: true,
    readOnly: !!existing || !editable,
    value: name,
    onChange: (v) => {
      setName(v);
      setNameTouched(true);
    },
    hint: nameProblem ?? `Team processes use it as ${connectionRef(name)}.${existing ? "" : " It can’t be changed later."}`,
  });
  for (const f of defs) {
    if (f.type === "segmented") {
      const value = settings[f.key] ?? f.options![0]!;
      fields.push({ type: "segmented", label: f.label, options: f.options!, value, readOnly: !editable, onChange: (v) => setSettings((s) => ({ ...s, [f.key]: v })) });
    } else if (f === secretDef) {
      const stored = !!existing?.hasSecret && !clearSecret;
      fields.push({
        type: "text",
        label: f.label,
        mono: true,
        secret: true,
        readOnly: !editable,
        value: secret || (stored ? STORED : ""),
        onChange: (v) => {
          setSecret(v.replace(/•/g, ""));
          setClearSecret(false);
        },
        hint:
          secretProblem(secret) ??
          (clearSecret
            ? `The stored ${secretWord} is removed when you save. Processes that need it stop working until someone adds one.`
            : stored
              ? `Stored by ${teamName}, encrypted. Members can use it but not see it. Leave it as it is to keep it, or type a new one to replace it.`
              : existing
                ? `None stored. Type one to add it; ${teamName} keeps it encrypted.`
                : `Kept by ${teamName}, encrypted. Members can use it but not see it.`),
      });
    } else {
      fields.push({ type: "text", label: f.label, mono: f.mono, readOnly: !editable, value: settings[f.key] ?? "", onChange: (v) => setSettings((s) => ({ ...s, [f.key]: v })) });
    }
  }
  fields.push({
    type: "segmented",
    label: "Who can change it",
    options: [EDITORS.admins, EDITORS.members],
    value: EDITORS[editors],
    readOnly: !editorsEditable,
    onChange: (v) => setEditors(v === EDITORS.members ? "members" : "admins"),
    hint: editorsEditable ? undefined : "Only owners and admins choose this.",
  });

  const members = team.members?.length;
  const who = existing
    ? ` Added by ${addedBy(existing.createdBy)} ${savedAgo(Date.parse(existing.createdAt), now)}${existing.version > 1 ? ` · changed by ${addedBy(existing.updatedBy)} ${savedAgo(Date.parse(existing.updatedAt), now)}` : ""}.`
    : "";

  // Nothing can reach a database or tracker from the app yet.
  const test = () => setResult({ ok: false, text: "Testing a connection isn’t available yet." });

  const failed = (error: unknown, action: ErrorContext["action"]) => {
    setResult({ ok: false, text: connectionErrorMessage(error, errorContext(action)) });
    // A newer or removed connection: show what is stored now.
    if (action !== "create" && (isCloudError(error, 409) || isCloudError(error, 404))) onChanged();
  };

  const save = async () => {
    if (problem) return;
    setBusy(true);
    setResult(null);
    const draft = { settings, editors, secret: clearSecret ? "" : secret.trim() ? secret : undefined };
    try {
      if (existing) await saveTeamConnection(team.team.id, true, existing, draft);
      else onCreated(await createTeamConnection(team.team.id, true, { name, kind, ...draft }));
      setSecret("");
      setClearSecret(false);
      setResult({ ok: true, text: `Saved to ${teamName}.` });
      onChanged();
    } catch (error) {
      failed(error, existing ? "save" : "create");
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!existing) return;
    setBusy(true);
    setResult(null);
    try {
      await removeTeamConnection(team.team.id, true, existing);
      onChanged();
      onClose();
    } catch (error) {
      failed(error, "remove");
    } finally {
      setBusy(false);
    }
  };

  const shown = result ?? (editable && dirty && problem ? { ok: false, text: problem } : null);
  return (
    <aside className="process-panel process-panel--wide" aria-label={`${existing ? existing.name : `New ${c.name}`} settings`}>
      <div className="process-panel__head">
        <Tile connection={c} size={36} />
        <div className="process-panel__titles">
          <div className="process-panel__title">{existing ? `${c.name} · ${existing.name}` : `New ${c.name} connection`}</div>
          <div className="process-panel__sub">
            {GROUP_NAME[c.group]} · shared with {teamName}
          </div>
        </div>
        <button type="button" className="process-x" aria-label="Close" onClick={onClose}>
          ×
        </button>
      </div>
      <div className="process-panel__body">
        <div className="cloud-banner cloud-banner--team team-shared">
          <TeamTile name={teamName} size={22} />
          <div className="team-shared__body">
            <div className="team-shared__title">Shared with {teamName}</div>
            <div className="team-shared__text">
              {members ? `All ${members} members run with these credentials.` : "Everyone in the team runs with these credentials."}
              {who}
            </div>
          </div>
        </div>
        {fields.map((f) => (
          <Fragment key={f.label}>
            <InspectorField field={f} />
            {f.label === secretDef.label && editable && existing?.hasSecret && (
              <button type="button" className="team-secret-toggle" onClick={() => setClearSecret((v) => !v)}>
                {clearSecret ? `Keep the stored ${secretWord}` : `Remove the stored ${secretWord}`}
              </button>
            )}
          </Fragment>
        ))}
      </div>
      <div className="process-panel__foot">
        {shown && (
          <div className={cx("process-panel__result", shown.ok ? "process-panel__result--ok" : "process-panel__result--bad")} role="status">
            <span className={cx("process-dot", shown.ok ? "process-dot--ok" : "process-dot--danger")} />
            {shown.text}
          </div>
        )}
        <div className="process-panel__buttons">
          <button type="button" className="process-btn process-btn--grow" onClick={test}>
            Test connection
          </button>
          {editable && dirty && (
            <button type="button" className="process-btn process-btn--primary" disabled={busy || !!problem} onClick={() => void save()}>
              {existing ? "Save" : `Add to ${teamName}`}
            </button>
          )}
          {editable && existing && (
            <button type="button" className="process-btn process-btn--danger" disabled={busy} onClick={() => void remove()}>
              Remove from team
            </button>
          )}
        </div>
      </div>
    </aside>
  );
}
