import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../../cloud/api";
import { useCloud } from "../../cloud/CloudProvider";
import type { DeviceSession } from "../../cloud/types";
import { Avatar, GitHubMark, StrokeIcon, TeamTile } from "../Account";
import { deviceCount, deviceUsage, notesSyncSummary } from "./accountText";

/**
 * Settings' account rows (settings-account, App Shell board 07): Account,
 * then, signed in, Notes sync, Devices and Teams. Everything else in
 * Settings works the same signed in or out.
 */
export function AccountRows({ onManageDevices }: { onManageDevices: () => void }) {
  const cloud = useCloud();
  const user = cloud.user;
  const [devices, setDevices] = useState<number | "error" | null>(null);
  const userId = user?.id;

  useEffect(() => {
    if (!userId) return;
    let live = true;
    setDevices(null);
    api
      .listSessions()
      .then((sessions) => live && setDevices(sessions.length))
      .catch(() => live && setDevices("error"));
    return () => {
      live = false;
    };
  }, [userId]);

  // The signed-out notice shows for one visit to Settings.
  const notice = cloud.signedOutNotice;
  const dismiss = useRef(cloud.dismissSignedOutNotice);
  dismiss.current = cloud.dismissSignedOutNotice;
  useEffect(() => {
    if (!notice) return;
    return () => dismiss.current();
  }, [notice]);

  const team = cloud.selectedTeam ?? cloud.teams[0] ?? null;
  const sync = notesSyncSummary(cloud.notesSync, cloud.teams, cloud.lastSyncedAt);

  return (
    <>
      <div className="row">
        <div className="row__label">
          Account
          <div className="row__hint">Only needed for teams</div>
        </div>
        {user ? (
          <div className="account-row">
            <Avatar name={user.name || user.login} size={30} />
            <div className="account-row__who">
              <div className="account-row__name">
                {user.name || user.login}
                <span title="Signed in with GitHub">
                  <GitHubMark size={13} />
                </span>
              </div>
              <div className="account-row__detail">
                @{user.login}
                {user.email && ` · ${user.email}`}
              </div>
            </div>
            <button type="button" className="btn btn--outline" onClick={() => void cloud.signOut().catch(() => {})}>
              Sign out
            </button>
          </div>
        ) : (
          <div className="account-row">
            <div className="account-row__status">
              Not signed in
              {notice && <div className="account-row__notice">You were signed out on this device.</div>}
            </div>
            <button type="button" className="btn btn--outline" onClick={() => cloud.openDialog({ kind: "sign-in" })}>
              Sign in…
            </button>
          </div>
        )}
      </div>

      {user && cloud.has("sync") && (
        <div className="row">
          <div className="row__label">Notes sync</div>
          <div className="account-row">
            {sync && <span className="cloud-dot cloud-dot--ok" aria-hidden="true" />}
            <div className="account-row__status account-row__status--strong">{sync ?? "Off"}</div>
            <button
              type="button"
              className="btn btn--outline"
              onClick={() => cloud.openDialog({ kind: "notes-sync-setup" })}
            >
              Change
            </button>
          </div>
        </div>
      )}

      {user && (
        <div className="row">
          <div className="row__label">Devices</div>
          <div className="account-row">
            <div className="account-row__status">
              {devices === "error" ? "Can't check right now" : devices === null ? "" : deviceCount(devices)}
            </div>
            <button type="button" className="btn btn--outline" onClick={onManageDevices}>
              Manage
            </button>
          </div>
        </div>
      )}

      {user && cloud.has("teams") && (
        <div className="row">
          <div className="row__label">Teams</div>
          {team ? (
            <div className="account-row">
              <span className="account-row__team-tile">
                <TeamTile name={team.name} size={22} />
              </span>
              <div className="account-row__status account-row__status--team">
                {team.name} <span className="account-row__role">· {team.role}</span>
              </div>
              <button
                type="button"
                className="btn btn--outline"
                onClick={() => cloud.openDialog({ kind: "team-settings", teamId: team.id })}
              >
                Team settings
              </button>
            </div>
          ) : (
            <div className="account-row">
              <div className="account-row__status">No team yet</div>
              <button type="button" className="btn btn--outline" onClick={() => cloud.openDialog({ kind: "create-team" })}>
                Create or join…
              </button>
            </div>
          )}
        </div>
      )}
    </>
  );
}

/**
 * settings-devices: every signed-in device, in place of the Settings body.
 * Other devices can be signed out from here; signing this one out returns
 * to Settings, signed out.
 */
export function DevicesView({ onBack, onClose }: { onBack: () => void; onClose: () => void }) {
  const cloud = useCloud();
  const [sessions, setSessions] = useState<DeviceSession[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const back = useRef<HTMLButtonElement>(null);

  const load = useCallback(async () => {
    try {
      setSessions(await api.listSessions());
    } catch {
      setError("Can't load your devices right now. Try again in a moment.");
    }
  }, []);

  useEffect(() => {
    void load();
    back.current?.focus();
  }, [load]);

  const revoke = async (id: string) => {
    setBusy(id);
    setError(null);
    try {
      await api.revokeSession(id);
    } catch (failure) {
      // Already gone counts as signed out.
      if (!api.isCloudError(failure, 404)) setError("Couldn't sign that device out. Try again.");
    }
    await load();
    setBusy(null);
  };

  const signOutHere = async () => {
    await cloud.signOut().catch(() => {});
    onBack();
  };

  const now = new Date();

  return (
    <>
      <div className="sheet__head settings-devices__head">
        <button ref={back} type="button" className="settings-devices__back" onClick={onBack}>
          ‹ Settings
        </button>
        <h2 className="sheet__title">Signed-in devices</h2>
        <button type="button" className="sheet__close" aria-label="Close settings" onClick={onClose}>
          ×
        </button>
      </div>

      <div className="sheet__body settings-devices__list">
        {sessions?.map((session) => (
          <div className="settings-device" key={session.id}>
            <span className="settings-device__tile">
              <StrokeIcon name="monitor" size={17} />
            </span>
            <div className="settings-device__text">
              <div className="settings-device__name">
                <span className="ellipsis">{session.deviceName}</span>
                {session.current && <span className="settings-device__pill">This device</span>}
              </div>
              <div className="settings-device__usage">{deviceUsage(session.createdAt, session.lastUsedAt, now)}</div>
            </div>
            {!session.current && (
              <button
                type="button"
                className="btn btn--outline"
                disabled={busy === session.id}
                onClick={() => void revoke(session.id)}
              >
                Sign out
              </button>
            )}
          </div>
        ))}
        {error && (
          <div className="cloud-field__error settings-devices__error" role="alert">
            {error}
          </div>
        )}
      </div>

      <div className="settings-note settings-note--devices">
        Each device signs in on its own and stays signed in for 90 days after it was last used. Signing a device out
        doesn't touch anything stored on it.
      </div>

      <div className="sheet__foot settings-devices__foot">
        <button type="button" className="btn btn--outline settings-devices__sign-out" onClick={() => void signOutHere()}>
          Sign out of this device
        </button>
        <button type="button" className="btn btn--outline" onClick={onBack}>
          Done
        </button>
      </div>
    </>
  );
}
