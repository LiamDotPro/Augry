import { useCallback, useEffect, useMemo, useState } from "react";
import { isCloudError } from "../../cloud/api";
import { useCloud } from "../../cloud/CloudProvider";
import type { PublicUser, TeamConnection } from "../../cloud/types";
import { connectionFields, testConnection } from "../../process/connections";
import { savedAgo } from "../../process/team";
import {
  canEdit,
  connectionName,
  loadTeamConnections,
  referenceOf,
  removeTeamConnection,
  teamConnectionSections,
  writeTeamConnection,
  type TeamConnectionCard,
} from "../../process/teamConnections";
import { cx } from "../../lib/cx";
import { TeamTile } from "../Account";
import { ConnectionCard, GROUP_NAME, SetupPanel, Tile } from "./ConnectionsView";
import { InspectorField, type Field } from "./InspectorField";
import { useProcess } from "./store";
import type { TeamApi } from "./useTeam";
import { useKeptScroll } from "./useKeptScroll";

const STORED = "••••••••••••";
const EDITORS = { admins: "Owners and admins", members: "Everyone" } as const;

/**
 * Connections with a team selected (board 11, team-connections): the team's
 * own connections, one set of credentials everyone runs with, then what
 * stays on this computer. The team connections API is cloud epic 005; until
 * the server lists the `connections` feature, the team's kinds read "Not set
 * up for the team" and only your local connections can be changed.
 */
export function TeamConnectionsView() {
  const { state, dispatch, now, team } = useProcess();
  const cloud = useCloud();
  const enabled = cloud.has("connections");
  const [query, setQuery] = useState("");
  const [list, setList] = useState<TeamConnection[] | null>(null);
  /** The team card open in the side panel, by kind. Local cards use the tab's own panel. */
  const [openKind, setOpenKind] = useState<string | null>(null);
  const scroll = useKeptScroll<HTMLDivElement>("connections");
  const teamId = team?.team.id ?? "";

  const reload = useCallback(() => {
    loadTeamConnections(teamId, enabled)
      .then(setList)
      .catch(() => {});
  }, [teamId, enabled]);
  useEffect(() => {
    setList(null);
    setOpenKind(null);
    reload();
  }, [reload]);

  const me = cloud.user?.id;
  const addedBy = useCallback((user: PublicUser) => (user.id === me ? "you" : user.name.trim() || user.login), [me]);
  const sections = useMemo(
    () => teamConnectionSections(state.connections, list, team?.team.name ?? "", addedBy),
    [state.connections, list, team, addedBy],
  );
  if (!team) return null;

  const q = query.trim().toLowerCase();
  const local = state.connections.find((c) => c.id === state.connection) ?? null;
  const openCard = openKind ? sections.flatMap((s) => s.cards).find((c) => !c.mine && c.local.icon === openKind) ?? null : null;
  const pickLocal = (id: string | null) => {
    setOpenKind(null);
    dispatch({ type: "connection-open", id });
  };
  const pickTeam = (kind: string | null) => {
    dispatch({ type: "connection-open", id: null });
    setOpenKind(kind);
  };

  return (
    <div className={cx("process-connections", (local || openCard) && "process-connections--panel")}>
      <div className="process-home__main process-connections__main" ref={scroll.ref} onScroll={scroll.onScroll}>
        <div className="process-home__head process-connections__head">
          <div className="process-home__titles">
            <button type="button" className="process-backlink" onClick={() => dispatch({ type: "navigate", route: { view: "home" } })}>
              <span aria-hidden="true">‹</span>Processes
            </button>
            <h1 className="process-home__title">Connections</h1>
            <p className="process-home__sub">
              {team.team.name}’s connections. Everyone in the team runs with the same credentials, and can use them without seeing them.
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
          {enabled && list && (
            <button
              type="button"
              className="process-btn"
              onClick={() => pickTeam(sections.flatMap((s) => s.cards).find((c) => !c.mine && !c.on)?.local.icon ?? "github")}
            >
              <span className="process-btn__plus" aria-hidden="true">+</span>
              Add team connection
            </button>
          )}
        </div>
        {sections.map((section) => {
          const cards = section.cards.filter((c) => !q || `${c.local.name} ${c.local.desc}`.toLowerCase().includes(q));
          if (!cards.length) return null;
          return (
            <section key={section.id} className="process-connections__group">
              <div className="process-home__section">
                <h2 className="process-home__h2">{section.title}</h2>
                <span className="process-home__hint">{section.hint}</span>
              </div>
              <div className="process-connections__grid">
                {cards.map((card) =>
                  card.mine ? (
                    <ConnectionCard
                      key={card.local.id}
                      connection={card.local}
                      picked={card.local.id === state.connection}
                      onOpen={() => pickLocal(card.local.id)}
                      view={{ on: card.on, status: card.status, used: card.used, action: card.on ? "Manage" : "Set up" }}
                    />
                  ) : (
                    <ConnectionCard
                      key={card.local.id}
                      connection={card.local}
                      picked={card.local.icon === openKind}
                      onOpen={() => pickTeam(card.local.icon)}
                      // Without team connections there is nothing to set up yet.
                      view={{ on: card.on, status: card.status, used: card.used, action: enabled && list ? (card.on ? "Manage" : "Set up") : null }}
                    />
                  ),
                )}
              </div>
            </section>
          );
        })}
      </div>
      {local && <SetupPanel connection={local} onClose={() => pickLocal(null)} />}
      {openCard && list && (
        <TeamSetupPanel
          key={openCard.local.icon}
          card={openCard}
          team={team}
          list={list}
          now={now}
          addedBy={addedBy}
          onClose={() => pickTeam(null)}
          onChanged={reload}
        />
      )}
    </div>
  );
}

/**
 * A team connection in the side panel (team-connection-setup): who it is
 * shared with and who added and changed it, its settings, a password that is
 * never shown, and who can change it. Test connection runs from this
 * computer. Members who can't change it see it read only.
 */
function TeamSetupPanel({
  card,
  team,
  list,
  now,
  addedBy,
  onClose,
  onChanged,
}: {
  card: TeamConnectionCard;
  team: TeamApi;
  list: TeamConnection[];
  now: number;
  addedBy(user: PublicUser): string;
  onClose(): void;
  onChanged(): void;
}) {
  const existing = card.team;
  const c = card.local;
  const teamName = team.team.name;
  const editable = canEdit(existing, team.team.role) && team.online;
  // A new one starts empty, with the first choice of each segmented setting.
  const [settings, setSettings] = useState<Record<string, string>>(
    () => existing?.settings ?? Object.fromEntries(connectionFields(c).flatMap((f) => (f.type === "segmented" ? [[f.key, f.options![0]!]] : []))),
  );
  const [secret, setSecret] = useState("");
  const [editors, setEditors] = useState<"admins" | "members">(existing?.editors ?? "admins");
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const dirty = !existing || secret !== "" || editors !== existing.editors || JSON.stringify(settings) !== JSON.stringify(existing.settings);
  const name = existing?.name ?? connectionName(settings.name || c.icon, list);

  const fields: Field[] = connectionFields(c)
    .filter((f) => f.type !== "switch")
    .map((f): Field => {
      if (f.type === "segmented") {
        const value = settings[f.key] ?? f.options![0]!;
        return { type: "segmented", label: f.label, options: f.options!, value, readOnly: !editable, onChange: (v) => setSettings((s) => ({ ...s, [f.key]: v })) };
      }
      if (f.type === "secret") {
        return {
          type: "text",
          label: f.label,
          mono: true,
          secret: true,
          readOnly: !editable,
          value: secret || (existing?.hasSecret ? STORED : ""),
          onChange: (v) => setSecret(v.replace(/•/g, "")),
          hint: `Stored by ${teamName}, encrypted. Members can use it but not see it. Type a new one to replace it.`,
        };
      }
      const hint = f.key === "name" && c.group === "systems" ? `Team processes use it as ${referenceOf(c.group, name)}.` : undefined;
      return { type: "text", label: f.label, mono: f.mono, readOnly: !editable, value: settings[f.key] ?? "", onChange: (v) => setSettings((s) => ({ ...s, [f.key]: v })), hint };
    });
  fields.push({
    type: "segmented",
    label: "Who can change it",
    options: [EDITORS.admins, EDITORS.members],
    value: EDITORS[editors],
    readOnly: !editable,
    onChange: (v) => setEditors(v === EDITORS.members ? "members" : "admins"),
  });

  const members = team.members?.length;
  const who = existing
    ? ` Added by ${addedBy(existing.createdBy)}${existing.version > 1 ? ` · changed by ${addedBy(existing.updatedBy)} ${savedAgo(Date.parse(existing.updatedAt), now)}` : ""}.`
    : "";

  const test = () => {
    const outcome = testConnection({ ...c, settings });
    setResult(outcome.ok ? { ok: true, text: `${outcome.text} · from this computer` } : outcome);
  };

  const failed = (error: unknown) => {
    if (isCloudError(error, 409)) {
      setResult({ ok: false, text: "Someone changed it a moment ago. Look it over and save again." });
      onChanged();
    } else if (isCloudError(error) && error.offline) setResult({ ok: false, text: "You're offline. Save it once you're back online." });
    else setResult({ ok: false, text: `Couldn't save it to ${teamName}. Try again in a moment.` });
  };

  const save = async () => {
    setBusy(true);
    try {
      await writeTeamConnection(team.team.id, true, c.icon, existing, { settings, secret, editors }, list);
      setSecret("");
      onChanged();
    } catch (error) {
      failed(error);
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!existing) return;
    setBusy(true);
    try {
      await removeTeamConnection(team.team.id, true, existing);
      onChanged();
      onClose();
    } catch (error) {
      failed(error);
    } finally {
      setBusy(false);
    }
  };

  return (
    <aside className="process-panel process-panel--wide" aria-label={`${c.name} settings`}>
      <div className="process-panel__head">
        <Tile connection={c} size={36} />
        <div className="process-panel__titles">
          <div className="process-panel__title">{c.name}</div>
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
          <InspectorField key={f.label} field={f} />
        ))}
      </div>
      <div className="process-panel__foot">
        {result && (
          <div className={cx("process-panel__result", result.ok ? "process-panel__result--ok" : "process-panel__result--bad")} role="status">
            <span className={cx("process-dot", result.ok ? "process-dot--ok" : "process-dot--danger")} />
            {result.text}
          </div>
        )}
        <div className="process-panel__buttons">
          <button type="button" className="process-btn process-btn--grow" onClick={test}>
            Test connection
          </button>
          {editable && dirty && (
            <button type="button" className="process-btn process-btn--primary" disabled={busy} onClick={() => void save()}>
              Save
            </button>
          )}
          {editable && existing && team.team.role !== "member" && (
            <button type="button" className="process-btn process-btn--danger" disabled={busy} onClick={() => void remove()}>
              Remove from team
            </button>
          )}
        </div>
      </div>
    </aside>
  );
}
