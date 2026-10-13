import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../../cloud/api";
import { useCloud } from "../../cloud/CloudProvider";
import type { DeviceSession } from "../../cloud/types";
import { Avatar, GitHubMark, StrokeIcon } from "../Account";
import { clockTime, deviceUsage, notesSyncedAt, notesSyncNames } from "../cloud/accountText";
import { SettingsHeading } from "./SettingsPage";

/**
 * Settings → Account (App Shell board 03, `settings-signed-out` and
 * `settings-account`): signing in, and once signed in the profile, notes
 * sync and the signed-in devices.
 */
export function AccountSettings() {
  const cloud = useCloud();

  // The signed-out notice shows for one visit to Settings.
  const notice = cloud.signedOutNotice;
  const dismiss = useRef(cloud.dismissSignedOutNotice);
  dismiss.current = cloud.dismissSignedOutNotice;
  useEffect(() => {
    if (!notice) return;
    return () => dismiss.current();
  }, [notice]);

  return cloud.user ? <SignedIn /> : <SignedOut notice={notice} />;
}

function SignedOut({ notice }: { notice: boolean }) {
  const cloud = useCloud();
  return (
    <>
      <SettingsHeading title="Account">
        Only needed for teams. Terminals, Notes and your own processes work without one, offline, and stay on this
        computer.
      </SettingsHeading>

      <div className="settings-section settings-account">
        {notice && (
          <div className="settings-account__notice" role="status">
            You were signed out on this device.
          </div>
        )}
        <div className="settings-account__who">
          <span className="settings-account__mark" aria-hidden="true">
            <GitHubMark size={22} />
          </span>
          <div>
            <div className="settings-account__name">Not signed in</div>
            <div className="settings-account__detail">Sign in to create or join a team.</div>
          </div>
        </div>

        <div>
          <button
            type="button"
            className="cloud-btn cloud-btn--primary"
            onClick={() => cloud.openDialog({ kind: "sign-in" })}
          >
            <GitHubMark size={15} />
            Continue with GitHub
          </button>
        </div>

        <div className="settings-account__columns">
          <div className="settings-account__column">
            <div className="settings-account__column-title">Stays on this computer</div>
            <Perk ok>Terminals</Perk>
            <Perk ok>Your processes and runs</Perk>
            <Perk ok>Your connections and their keys</Perk>
          </div>
          <div className="settings-account__column">
            <div className="settings-account__column-title">Shared with a team</div>
            <Perk>Team processes and versions</Perk>
            <Perk>Templates added to the team</Perk>
            <Perk>Team connections, one set of keys</Perk>
            <Perk>Notes, synced to you or the team</Perk>
          </div>
        </div>

        <p className="settings-footnote">
          GitHub opens in your browser and shows a short code to enter. Augry gets your GitHub name, login and email.
          It never sees your password.
        </p>
      </div>
    </>
  );
}

function SignedIn() {
  const cloud = useCloud();
  const user = cloud.user!;
  const name = user.name || user.login;
  const syncing = notesSyncNames(cloud.notesSync, cloud.teams);
  const syncedAt = notesSyncedAt(cloud.notesSync, cloud.teams, cloud.lastSyncedAt);

  return (
    <>
      <SettingsHeading title="Account">Signed in with GitHub. Only needed for teams and notes sync.</SettingsHeading>

      <div className="settings-section settings-account">
        <div className="settings-account__who">
          <Avatar name={name} size={48} />
          <div className="settings-account__text">
            <div className="settings-account__name">
              {name}
              <span title="Signed in with GitHub">
                <GitHubMark size={14} />
              </span>
            </div>
            <div className="settings-account__detail">
              @{user.login}
              {user.email && ` · ${user.email}`}
            </div>
          </div>
          <button type="button" className="cloud-btn cloud-btn--small" onClick={() => void cloud.signOut().catch(() => {})}>
            Sign out
          </button>
        </div>
        <p className="settings-footnote">
          Signing out keeps everything on this computer. Team processes are hidden until you sign in again.
        </p>
      </div>

      {cloud.has("sync") && (
        <div className="settings-section">
          <div className="settings-section__title">Notes sync</div>
          <div className="settings-line">
            <span className={syncing ? "cloud-dot cloud-dot--ok" : "cloud-dot cloud-dot--ring"} aria-hidden="true" />
            <div className="settings-line__text">
              <div className="settings-line__name">{syncing ?? "Off"}</div>
              <div className="settings-line__detail">
                {!syncing
                  ? "Notes save to this computer only"
                  : syncedAt === null
                    ? "Notes still save to this computer first"
                    : `Synced ${clockTime(syncedAt)} · notes still save to this computer first`}
              </div>
            </div>
            <button
              type="button"
              className="cloud-btn cloud-btn--small"
              onClick={() => cloud.openDialog({ kind: "notes-sync-setup" })}
            >
              Change
            </button>
          </div>
        </div>
      )}

      <Devices />
    </>
  );
}

/** Every signed-in device; other devices can be signed out from here. */
function Devices() {
  const [sessions, setSessions] = useState<DeviceSession[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setSessions(await api.listSessions());
      setError(null);
    } catch {
      setError("Can't load your devices right now. Try again in a moment.");
    }
  }, []);

  useEffect(() => {
    void load();
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

  const now = new Date();

  return (
    <div className="settings-section">
      <div className="settings-section__title">
        Signed-in devices
        {sessions && <span className="settings-section__count">{sessions.length}</span>}
      </div>
      {sessions?.map((session) => (
        <div className="settings-line" key={session.id}>
          <span className="settings-line__tile" aria-hidden="true">
            <StrokeIcon name="monitor" size={16} />
          </span>
          <div className="settings-line__text">
            <div className="settings-line__name">
              <span className="ellipsis">{session.deviceName}</span>
              {session.current && <span className="settings-device__pill">This device</span>}
            </div>
            <div className="settings-line__detail">{deviceUsage(session.createdAt, session.lastUsedAt, now)}</div>
          </div>
          {!session.current && (
            <button
              type="button"
              className="cloud-btn cloud-btn--small"
              disabled={busy === session.id}
              onClick={() => void revoke(session.id)}
            >
              Sign out
            </button>
          )}
        </div>
      ))}
      {error && (
        <div className="cloud-field__error" role="alert">
          {error}
        </div>
      )}
      <p className="settings-footnote">
        Each device signs in on its own and stays signed in for 90 days after it was last used. Signing a device out
        doesn't touch anything stored on it.
      </p>
    </div>
  );
}

function Perk({ ok = false, children }: { ok?: boolean; children: string }) {
  return (
    <div className="settings-perk">
      <span className={ok ? "settings-perk__mark settings-perk__mark--ok" : "settings-perk__mark"} aria-hidden="true">
        {ok ? "✓" : "+"}
      </span>
      {children}
    </div>
  );
}
