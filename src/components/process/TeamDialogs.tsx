import { useEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent, type ReactNode } from "react";
import * as api from "../../cloud/api";
import { useCloud } from "../../cloud/CloudProvider";
import type { TeamConnection } from "../../cloud/types";
import { cloneProcess } from "../../process/json";
import { maskedAt, parseSecretError, scanSecrets, secretWhat, stepIndexOf } from "../../process/secrets";
import { connectionsUsed } from "../../process/share";
import { recordOf, type TeamDialog } from "../../process/state";
import { firstName, parseTime } from "../../process/team";
import { loadTeamConnections, teamConnectionOf, writeTeamConnection } from "../../process/teamConnections";
import { whenLabel } from "../../process/runs";
import { working } from "../../process/versions";
import { TeamTile } from "../Account";
import { cx } from "../../lib/cx";
import { ProcessIcon } from "./ProcessIcon";
import { SAVE_OFFLINE, useProcess } from "./store";

/** The team dialogs of Process board 11, whichever is open. */
export function TeamDialogs() {
  const { state } = useProcess();
  const dialog = state.teamDialog;
  if (!dialog) return null;
  switch (dialog.kind) {
    case "share":
      return <ShareDialog processId={dialog.processId} />;
    case "share-blocked":
      return <ShareBlockedDialog dialog={dialog} />;
    case "conflict":
      return <ConflictDialog dialog={dialog} />;
  }
}

/**
 * The 680px frame these dialogs share: a 40px tile, a 21px title with its
 * text under it, the body and the footer strip. Scrim click and Escape close,
 * Enter does the primary action, and the first field (else the primary
 * button) has focus.
 */
function Frame({
  label,
  tile,
  title,
  text,
  onClose,
  onSubmit,
  children,
  foot,
}: {
  label: string;
  tile: ReactNode;
  title: string;
  text: ReactNode;
  onClose(): void;
  onSubmit?(): void;
  children: ReactNode;
  foot: ReactNode;
}) {
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    const el = root.current;
    (el?.querySelector<HTMLElement>("select, input") ?? el?.querySelector<HTMLElement>("[data-primary]"))?.focus();
    return () => before?.focus?.();
  }, []);

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      e.nativeEvent.stopPropagation();
      onClose();
    } else if (e.key === "Enter" && onSubmit && !(e.target as HTMLElement).closest("button, select, textarea, a")) {
      e.preventDefault();
      onSubmit();
    }
  };

  return (
    <div className="cloud-scrim" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div
        ref={root}
        className="cloud-dialog team-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={label}
        style={{ "--cloud-dialog-width": "680px" } as CSSProperties}
        onKeyDown={onKeyDown}
      >
        <div className="team-dialog__head">
          {tile}
          <div className="team-dialog__titles">
            <h2 className="team-dialog__title">{title}</h2>
            <p className="team-dialog__text">{text}</p>
          </div>
          <button type="button" className="cloud-dialog__close" aria-label="Close" onClick={onClose}>
            ×
          </button>
        </div>
        <div className="team-dialog__body">{children}</div>
        <div className="cloud-dialog__foot team-dialog__foot">{foot}</div>
      </div>
    </div>
  );
}

function Tick() {
  return <ProcessIcon name="end" size={15} stroke={2} className="team-dialog__tick" />;
}

// share-to-team ---------------------------------------------------------------------

/**
 * "Share Bug fix with Northwind": the team gets a copy as its version 1 and
 * yours stays as it is. Opened from a personal process's ⋯ menu once the
 * local secret check passed (a match opens share-blocked instead).
 */
function ShareDialog({ processId }: { processId: string }) {
  const { state, dispatch } = useProcess();
  const cloud = useCloud();
  const record = recordOf(state, processId);
  const [teamId, setTeamId] = useState(() => cloud.teams[0]?.id ?? "");
  const team = cloud.teams.find((t) => t.id === teamId) ?? null;
  const process = record ? working(record) : null;
  const used = useMemo(() => (process ? connectionsUsed(process, state.connections) : []), [process, state.connections]);
  // Whether the team has each connection, and sharing yours with it, need team connections (cloud epic 005).
  const withConnections = cloud.has("connections");
  const [teamConnections, setTeamConnections] = useState<TeamConnection[] | null>(null);
  const [unticked, setUnticked] = useState<Set<string>>(() => new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setTeamConnections(null);
    if (!withConnections || !teamId) return;
    let cancelled = false;
    loadTeamConnections(teamId, true)
      .then((list) => !cancelled && setTeamConnections(list))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [teamId, withConnections]);

  const close = () => dispatch({ type: "team-dialog", dialog: null });
  if (!record || !process) return null;

  const share = async () => {
    if (!team || busy) return;
    const definition = { ...cloneProcess(process), version: 1 };
    const found = scanSecrets(definition)[0];
    if (found) {
      dispatch({ type: "team-dialog", dialog: { kind: "share-blocked", processId, rule: found.rule, path: found.path, masked: found.masked } });
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const created = await api.createTeamProcess(team.id, definition);
      if (withConnections && teamConnections) {
        // Ticked connections the team lacks go with it. The password stays in this
        // computer's credential store, so only the settings are copied.
        for (const u of used) {
          if (!u.local || !u.kind || unticked.has(u.ref) || teamConnectionOf(u.kind, teamConnections)) continue;
          await writeTeamConnection(team.id, true, u.kind, null, { settings: u.local.settings, editors: "admins" }, teamConnections).catch(() => {});
        }
      }
      dispatch({ type: "team-saved", teamId: team.id, process: created });
      dispatch({ type: "team-dialog", dialog: null });
      // Shown straight away: the team's processes, with the new copy among them.
      cloud.selectSpace(team.id);
    } catch (e) {
      const secret = api.isCloudError(e, 422) ? parseSecretError(e.message) : null;
      if (secret) {
        dispatch({
          type: "team-dialog",
          dialog: { kind: "share-blocked", processId, rule: secret.rule, path: secret.path, masked: maskedAt(definition, secret.path) },
        });
      } else if (api.isCloudError(e) && e.offline) setError("You're offline. Share it once you're back online.");
      else setError(`Couldn't share it with ${team.name}. Try again in a moment.`);
    } finally {
      setBusy(false);
    }
  };

  const openJson = () => {
    dispatch({ type: "team-dialog", dialog: null });
    dispatch({ type: "navigate", route: { view: "editor", processId } });
    dispatch({ type: "editor-ui", processId, patch: { json: true } });
  };

  const teamName = team?.name ?? "the team";
  return (
    <Frame
      label={`Share ${process.name} with ${teamName}`}
      tile={<TeamTile name={teamName} size={40} />}
      title={`Share ${process.name} with ${teamName}`}
      text={`${teamName} gets a copy as its version 1. Your copy stays on this computer and doesn't change. Later edits to either one stay separate.`}
      onClose={close}
      onSubmit={() => void share()}
      foot={
        <>
          <button type="button" className="cloud-btn" onClick={openJson}>
            Open in JSON
          </button>
          <span className="cloud-dialog__spacer" />
          <button type="button" className="cloud-btn cloud-btn--quiet" onClick={close}>
            Cancel
          </button>
          <button type="button" className="cloud-btn cloud-btn--primary" data-primary disabled={!team || busy || !cloud.online} onClick={() => void share()}>
            Share copy
          </button>
        </>
      }
    >
      <div className="team-dialog__field">
        <label className="team-dialog__label" htmlFor="share-team">
          Share to
        </label>
        <div className="team-picker">
          <TeamTile name={teamName} size={20} />
          <select id="share-team" className="team-picker__select" value={teamId} onChange={(e) => setTeamId(e.target.value)}>
            {cloud.teams.map((t) => (
              <option key={t.id} value={t.id}>
                {t.name}
              </option>
            ))}
          </select>
          <span className="team-picker__caret" aria-hidden="true">
            ▼
          </span>
        </div>
      </div>

      <div className="team-dialog__card">
        <div className="team-dialog__label">Checked before sharing</div>
        <div className="team-dialog__check">
          <Tick />
          No keys or tokens in the file
        </div>
        <div className="team-dialog__check">
          <Tick />
          Uses connections by name only
        </div>
        {used.length > 0 && (
          <>
            <div className="team-dialog__rule" />
            <div className="team-dialog__label">Connections it uses</div>
            <div className="team-conns">
              {used.map((u) => {
                if (!withConnections || !teamConnections) {
                  return (
                    <div key={u.ref} className="team-conn">
                      <span className="team-conn__name">{u.label}</span>
                    </div>
                  );
                }
                const has = !!u.kind && !!teamConnectionOf(u.kind, teamConnections);
                if (has) {
                  return (
                    <div key={u.ref} className="team-conn">
                      <span className="team-conn__name">{u.label}</span>
                      <span className="team-conn__has">
                        <Tick />
                        {teamName} has this
                      </span>
                    </div>
                  );
                }
                const on = !unticked.has(u.ref);
                return (
                  <div key={u.ref} className="team-conn team-conn--missing">
                    <div className="team-conn__top">
                      <span className="team-conn__name">{u.label}</span>
                      <span className="team-conn__missing">{teamName} doesn’t have this</span>
                    </div>
                    {u.local && (
                      <>
                        <label className="team-conn__share">
                          <input
                            type="checkbox"
                            className="team-check"
                            checked={on}
                            onChange={() =>
                              setUnticked((s) => {
                                const next = new Set(s);
                                if (on) next.add(u.ref);
                                else next.delete(u.ref);
                                return next;
                              })
                            }
                          />
                          Share my connection with {teamName}
                        </label>
                        <div className="team-conn__note">
                          Its host, user and password are copied to {teamName}. Everyone in the team runs with them and can’t see the password.
                        </div>
                      </>
                    )}
                  </div>
                );
              })}
            </div>
            {!withConnections && <div className="team-conn__note">Teammates run it with their own connections of the same name.</div>}
          </>
        )}
      </div>
      {error && (
        <div className="cloud-field__error" role="alert">
          {error}
        </div>
      )}
    </Frame>
  );
}

// share-blocked ---------------------------------------------------------------------

/** "Remove the token before sharing": the local check or the server's 422 found a key or token. */
function ShareBlockedDialog({ dialog }: { dialog: Extract<TeamDialog, { kind: "share-blocked" }> }) {
  const { state, dispatch } = useProcess();
  const record = recordOf(state, dialog.processId);
  const name = record ? working(record).name : "This process";
  const close = () => dispatch({ type: "team-dialog", dialog: null });
  // The last key of the path is the one to change: lanes[2].steps[1].env.GH_TOKEN
  const split = dialog.path.match(/^(.*?)([^.[\]]+|\[[^\]]+\])$/);
  const [head, last] = split ? [split[1]!, split[2]!] : ["", dialog.path];

  const openJson = () => {
    const processId = dialog.processId;
    close();
    dispatch({ type: "navigate", route: { view: "editor", processId } });
    dispatch({ type: "editor-ui", processId, patch: { json: true } });
    // Jump to the step the path is in, when it is in one.
    const index = stepIndexOf(dialog.path);
    const step = index !== null && record ? working(record).steps[index] : undefined;
    if (step) dispatch({ type: "select", processId, selection: { kind: "step", id: step.id } });
  };

  return (
    <Frame
      label="Remove the token before sharing"
      tile={
        <span className="team-dialog__tile team-dialog__tile--warn">
          <ProcessIcon name="warn" size={18} />
        </span>
      }
      title="Remove the token before sharing"
      text={`${name} has what looks like ${secretWhat(dialog.rule)}. Team processes can't contain keys or tokens, so nothing was shared.`}
      onClose={close}
      onSubmit={openJson}
      foot={
        <>
          <button
            type="button"
            className="cloud-btn"
            onClick={() => {
              close();
              dispatch({ type: "navigate", route: { view: "connections" } });
            }}
          >
            Open Connections
          </button>
          <span className="cloud-dialog__spacer" />
          <button type="button" className="cloud-btn cloud-btn--quiet" onClick={close}>
            Close
          </button>
          <button type="button" className="cloud-btn cloud-btn--primary" data-primary onClick={openJson}>
            Open in JSON
          </button>
        </>
      }
    >
      <div className="team-dialog__card">
        <div className="team-dialog__label">Found at</div>
        <div className="team-found">
          {head}
          <span className="team-found__key">{last}</span>
          {dialog.masked && (
            <>
              <br />
              <span className="team-found__value">"{dialog.masked}"</span>
            </>
          )}
        </div>
      </div>
      <p className="team-dialog__note">
        Put the token in a team connection and refer to it by name, for example <code className="team-dialog__code">{"${conn:github}"}</code>. Your own
        copy keeps working as it is.
      </p>
    </Frame>
  );
}

// team-conflict ---------------------------------------------------------------------

/**
 * "Maya saved a newer version": a save hit a 409. The draft now sits on top of
 * the newer version, so saving it again makes the next one and the other
 * person's stays in the history.
 */
function ConflictDialog({ dialog }: { dialog: Extract<TeamDialog, { kind: "conflict" }> }) {
  const { state, dispatch, now, team, offline } = useProcess();
  const record = recordOf(state, dialog.processId);
  const close = () => dispatch({ type: "team-dialog", dialog: null });
  const nameOf = (id: string | undefined) => team?.nameOf(id, false) ?? "Someone";
  const theirs = dialog.theirs;
  const theirName = nameOf(theirs.updatedBy);
  const teamName = team?.team.name ?? "the team";
  const name = record ? working(record).name : theirs.name;
  const cantSave = offline(dialog.processId);
  const save = () => {
    if (!cantSave) void team?.save(dialog.processId);
  };

  const rows = [
    { n: `v${theirs.version}`, who: theirName, what: `Saved to ${teamName}`, when: whenLabel(parseTime(theirs.updatedAt), now), mine: false },
    { n: "yours", who: "Your changes", what: `Started from v${dialog.base.version}`, when: "not saved", mine: true },
    { n: `v${dialog.base.version}`, who: nameOf(dialog.base.byId), what: "Where you started", when: whenLabel(dialog.base.at, now), mine: false },
  ];

  return (
    <Frame
      label={`${firstName(theirName)} saved a newer version`}
      tile={
        <span className="team-dialog__tile team-dialog__tile--accent">
          <ProcessIcon name="merge" size={18} />
        </span>
      }
      title={`${firstName(theirName)} saved a newer version`}
      text={`You started editing ${name} from version ${dialog.base.version}. ${theirName} saved version ${theirs.version} while you were working, so yours wasn't saved yet.`}
      onClose={close}
      onSubmit={save}
      foot={
        <>
          <button
            type="button"
            className="cloud-btn"
            onClick={() => {
              close();
              dispatch({ type: "navigate", route: { view: "versions", processId: dialog.processId } });
              dispatch({ type: "versions-ui", processId: dialog.processId, patch: { compare: theirs.version, mode: "json" } });
            }}
          >
            Compare v{theirs.version} with yours
          </button>
          <span className="cloud-dialog__spacer" />
          <button
            type="button"
            className="cloud-btn cloud-btn--quiet"
            onClick={() => {
              dispatch({ type: "discard", processId: dialog.processId });
              close();
            }}
          >
            Discard mine
          </button>
          <button type="button" className="cloud-btn cloud-btn--primary" data-primary disabled={cantSave} title={cantSave ? SAVE_OFFLINE : undefined} onClick={save}>
            Save mine as v{theirs.version + 1}
          </button>
        </>
      }
    >
      <div className="team-versions">
        {rows.map((r) => (
          <div key={r.n} className={cx("team-version", r.mine && "team-version--mine")}>
            <span className="team-version__n">{r.n}</span>
            <span className="team-version__body">
              <span className="team-version__who">{r.who}</span>
              <span className="team-version__what">{r.what}</span>
            </span>
            <span className="team-version__when">{r.when}</span>
          </div>
        ))}
      </div>
      <p className="team-dialog__note team-dialog__note--quiet">
        Saving yours makes version {theirs.version + 1}. {firstName(theirName)}'s version {theirs.version} stays in the history and can be restored.
      </p>
    </Frame>
  );
}
