import { ask } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../../cloud/api";
import { useCloud } from "../../cloud/CloudProvider";
import {
  canChangeRole,
  canManage,
  canRemove,
  DEFAULT_INVITE_HOURS,
  displayInviteUrl,
  INVITE_EXPIRY_OPTIONS,
  inviteSummary,
  isValidTeamName,
  roleChoices,
  roleLabel,
  roleNoun,
  settingsErrorMessage,
  TEAM_NAME_RULE,
} from "../../cloud/teams";
import type { Invite, Member, Role, TeamDetail } from "../../cloud/types";
import { cx } from "../../lib/cx";
import { initialOf } from "../Account";
import { TeamDialog, TeamFieldError } from "./TeamDialog";

type InviteRole = Invite["role"];

/** Asks before something that can't be undone; falls back to the browser's confirm outside Tauri. */
async function confirmAction(message: string, title: string, okLabel: string, cancelLabel: string): Promise<boolean> {
  try {
    return await ask(message, { title, kind: "warning", okLabel, cancelLabel });
  } catch {
    return window.confirm(message);
  }
}

const statusOf = (error: unknown) => (api.isCloudError(error) ? error.status : -1);

/**
 * team-settings (App Shell board 08): rename, members and their roles, invite
 * someone, leave or delete. Members (not admins or owners) see the list
 * read-only and no invite column. Every change goes straight to the API;
 * there is no draft to save.
 */
export function TeamSettingsDialog({ teamId }: { teamId: string }) {
  const cloud = useCloud();
  const known = cloud.teams.find((team) => team.id === teamId) ?? null;
  const [detail, setDetail] = useState<TeamDetail | null>(null);
  /** Why loading the members failed, by status; null when it didn't. */
  const [loadStatus, setLoadStatus] = useState<number | null>(null);
  /** Which change is in flight: "name", "invite", "leave", "delete" or "member:<userId>". */
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [nameDraft, setNameDraft] = useState<string | null>(null);
  const [inviteRole, setInviteRole] = useState<InviteRole>("member");
  const [inviteHours, setInviteHours] = useState<number>(DEFAULT_INVITE_HOURS);
  const [invite, setInvite] = useState<Invite | null>(null);
  const [inviteError, setInviteError] = useState<string | null>(null);
  const [copied, setCopied] = useState<"link" | "code" | null>(null);
  const copiedTimer = useRef(0);

  const name = detail?.name ?? known?.name ?? "";
  const myRole: Role = detail?.role ?? known?.role ?? "member";
  const myId = cloud.user?.id;
  const managing = canManage(myRole);
  const offline = !cloud.online;

  const load = useCallback(async () => {
    try {
      const fresh = await api.getTeam(teamId);
      setDetail(fresh);
      setLoadStatus(null);
    } catch (failure) {
      setLoadStatus(statusOf(failure));
    }
  }, [teamId]);

  useEffect(() => {
    void load();
  }, [load]);

  // Back online after a load that couldn't reach the server: try again.
  const unreachable = loadStatus !== null && (loadStatus === 0 || loadStatus >= 500);
  useEffect(() => {
    if (cloud.online && unreachable) void load();
  }, [cloud.online, unreachable, load]);

  useEffect(() => () => window.clearTimeout(copiedTimer.current), []);

  /** Runs one change at a time and words its failure in `show`. */
  const run = async (key: string, work: () => Promise<void>, show: (message: string | null) => void = setError) => {
    if (busy) return;
    setBusy(key);
    show(null);
    try {
      await work();
    } catch (failure) {
      show(settingsErrorMessage(statusOf(failure)));
    } finally {
      setBusy(null);
    }
  };

  const saveName = () => {
    if (nameDraft === null) return;
    const next = nameDraft.trim();
    if (next === name) {
      setNameDraft(null);
      return;
    }
    if (!isValidTeamName(next)) {
      setError(TEAM_NAME_RULE);
      return;
    }
    void run("name", async () => {
      const renamed = await api.renameTeam(teamId, next);
      setDetail((current) => (current ? { ...current, name: renamed.name } : current));
      setNameDraft(null);
      void cloud.refreshTeams().catch(() => {});
    });
  };

  const changeRole = (member: Member, role: Role) =>
    run(`member:${member.user.id}`, async () => {
      const previous = detail;
      setDetail((current) =>
        current
          ? { ...current, members: current.members.map((m) => (m.user.id === member.user.id ? { ...m, role } : m)) }
          : current,
      );
      try {
        await api.setMemberRole(teamId, member.user.id, role);
      } catch (failure) {
        setDetail(previous);
        throw failure;
      }
      // Your own role decides what this dialog lets you do.
      if (member.user.id === myId) {
        await load();
        void cloud.refreshTeams().catch(() => {});
      }
    });

  const remove = (member: Member) =>
    run(`member:${member.user.id}`, async () => {
      await api.removeMember(teamId, member.user.id);
      setDetail((current) =>
        current ? { ...current, members: current.members.filter((m) => m.user.id !== member.user.id) } : current,
      );
    });

  const createInvite = () =>
    run(
      "invite",
      async () => {
        setInvite(await api.createInvite(teamId, inviteRole, inviteHours));
        setCopied(null);
      },
      setInviteError,
    );

  const copy = (kind: "link" | "code", text: string) => {
    void navigator.clipboard
      .writeText(text)
      .then(() => {
        setCopied(kind);
        window.clearTimeout(copiedTimer.current);
        copiedTimer.current = window.setTimeout(() => setCopied(null), 1500);
      })
      .catch(() => {});
  };

  /** After leaving or deleting: the team is gone from the list, Personal shows. */
  const forget = async () => {
    await cloud.refreshTeams().catch(() => {});
    if (cloud.selectedSpace === teamId) cloud.selectSpace("personal");
    cloud.closeDialog();
  };

  const leave = async () => {
    if (!myId || busy) return;
    const confirmed = await confirmAction(
      `You'll need a new invite to join ${name} again. Your personal processes stay as they are.`,
      `Leave ${name}?`,
      "Leave team",
      "Stay",
    );
    if (!confirmed) return;
    await run("leave", async () => {
      await api.removeMember(teamId, myId);
      await forget();
    });
  };

  const deleteTeam = async () => {
    if (busy) return;
    const confirmed = await confirmAction(
      `Deletes ${name} for everyone in it, with its processes, their versions and its shared notes. This can't be undone.`,
      `Delete ${name}?`,
      "Delete team",
      "Keep it",
    );
    if (!confirmed) return;
    await run("delete", async () => {
      await api.deleteTeam(teamId);
      await forget();
    });
  };

  const members = detail?.members ?? [];
  const choices = roleChoices(myRole);

  return (
    <TeamDialog label={`${name || "Team"} settings`} width={800} className="team-settings" onClose={cloud.closeDialog}>
      <div className="team-settings__head">
        <span className="cloud-dialog__tile team-dialog__tile--team" aria-hidden="true">
          {initialOf(name)}
        </span>
        <div className="team-settings__heading">
          <h2 className="team-settings__title">{name || "Team settings"}</h2>
          <div className="team-settings__sub">Team settings · you're {roleNoun(myRole)}</div>
        </div>
        <button type="button" className="team-settings__close" aria-label="Close" onClick={cloud.closeDialog}>
          ×
        </button>
      </div>

      <div className={cx("team-settings__grid", !managing && "team-settings__grid--single")}>
        <div className="team-settings__main">
          <div className="cloud-field">
            <label className="cloud-field__label" htmlFor="team-settings-name">
              Name
            </label>
            <input
              id="team-settings-name"
              className="cloud-field__input"
              value={nameDraft ?? name}
              readOnly={!managing || offline || busy === "name"}
              autoComplete="off"
              onChange={(event) => {
                setNameDraft(event.target.value);
                setError(null);
              }}
              onBlur={saveName}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  saveName();
                }
              }}
            />
          </div>

          <div className="team-members">
            <div className="team-members__head">
              <span className="cloud-field__label">Members</span>
              {detail && <span className="team-members__count">{members.length}</span>}
            </div>
            {!detail && loadStatus === null && <div className="team-members__status">Loading members…</div>}
            {!detail && loadStatus !== null && (
              <div className="team-members__status">
                {unreachable
                  ? "You're offline. Members show when you're back online."
                  : settingsErrorMessage(loadStatus)}
              </div>
            )}
            {members.map((member) => {
              const isMe = member.user.id === myId;
              const editable = managing && !offline && canChangeRole(myRole, member);
              const pending = busy === `member:${member.user.id}`;
              const label = member.user.name || member.user.login;
              return (
                <div key={member.user.id} className="team-member">
                  <span className={cx("team-member__avatar", isMe && "team-member__avatar--me")} aria-hidden="true">
                    {initialOf(label)}
                  </span>
                  <div className="team-member__who">
                    <div className="team-member__name">
                      {label}
                      {isMe && <span className="team-member__you"> · you</span>}
                    </div>
                    <div className="team-member__login">@{member.user.login}</div>
                  </div>
                  {editable ? (
                    <span className="team-role">
                      <select
                        className="team-role__select"
                        aria-label={`${label}'s role`}
                        value={member.role}
                        disabled={pending}
                        onChange={(event) => void changeRole(member, event.target.value as Role)}
                      >
                        {choices.map((role) => (
                          <option key={role} value={role}>
                            {roleLabel(role)}
                          </option>
                        ))}
                      </select>
                      <span className="team-role__chevron" aria-hidden="true">
                        ▼
                      </span>
                    </span>
                  ) : (
                    <span className="team-role team-role--static">{roleLabel(member.role)}</span>
                  )}
                  {managing &&
                    (canRemove(myRole, myId, member) ? (
                      <button
                        type="button"
                        className="team-member__remove"
                        title="Remove"
                        aria-label={`Remove ${label}`}
                        disabled={pending || offline}
                        onClick={() => void remove(member)}
                      >
                        ×
                      </button>
                    ) : (
                      <span className="team-member__remove" aria-hidden="true" />
                    ))}
                </div>
              );
            })}
          </div>

          {(error || (offline && detail)) && (
            <TeamFieldError>{error ?? "You're offline. Changes can't be saved until you're back online."}</TeamFieldError>
          )}
          <p className="team-settings__note">
            Owners and admins can invite, rename and change roles. Only an owner can make someone an owner. A team
            always keeps at least one owner.
          </p>
        </div>

        {managing && (
          <div className="team-invite">
            <div className="team-invite__title">Invite someone</div>
            <div className="cloud-field">
              <span className="cloud-field__label" id="team-invite-role">
                Joins as
              </span>
              <div className="team-seg" role="radiogroup" aria-labelledby="team-invite-role">
                {(["member", "admin"] as const).map((role) => (
                  <button
                    key={role}
                    type="button"
                    role="radio"
                    aria-checked={inviteRole === role}
                    className={cx("team-seg__option", inviteRole === role && "team-seg__option--on")}
                    onClick={() => setInviteRole(role)}
                  >
                    {roleLabel(role)}
                  </button>
                ))}
              </div>
            </div>
            <div className="cloud-field">
              <label className="cloud-field__label" htmlFor="team-invite-expiry">
                Expires after
              </label>
              <span className="team-select">
                <select
                  id="team-invite-expiry"
                  className="team-select__input"
                  value={inviteHours}
                  onChange={(event) => setInviteHours(Number(event.target.value))}
                >
                  {INVITE_EXPIRY_OPTIONS.map((option) => (
                    <option key={option.hours} value={option.hours}>
                      {option.label}
                    </option>
                  ))}
                </select>
                <span className="team-select__chevron" aria-hidden="true">
                  ▼
                </span>
              </span>
            </div>
            <button
              type="button"
              className="team-invite__create"
              disabled={busy === "invite" || offline}
              onClick={() => void createInvite()}
            >
              Create invite
            </button>
            {inviteError && <TeamFieldError>{inviteError}</TeamFieldError>}
            {invite && (
              <div className="team-invite-card">
                <div className="team-invite-card__label">Send this to them</div>
                <div className="team-invite-card__row">
                  <span className="team-invite-card__url" title={invite.url}>
                    {displayInviteUrl(invite.url)}
                  </span>
                  <button type="button" className="team-invite-card__copy" onClick={() => copy("link", invite.url)}>
                    {copied === "link" ? "Copied" : "Copy link"}
                  </button>
                </div>
                <div className="team-invite-card__row">
                  <span className="team-invite-card__code">{invite.code}</span>
                  <button type="button" className="team-invite-card__copy" onClick={() => copy("code", invite.code)}>
                    {copied === "code" ? "Copied" : "Copy code"}
                  </button>
                </div>
                <div className="team-invite-card__meta">{inviteSummary(invite)}</div>
              </div>
            )}
          </div>
        )}
      </div>

      <div className="team-settings__foot">
        <button
          type="button"
          className="team-settings__btn"
          disabled={!myId || busy !== null || offline}
          onClick={() => void leave()}
        >
          Leave team
        </button>
        {myRole === "owner" && (
          <button
            type="button"
            className="team-settings__btn team-settings__btn--danger"
            disabled={busy !== null || offline}
            onClick={() => void deleteTeam()}
          >
            Delete team
          </button>
        )}
        <span className="cloud-dialog__spacer" />
        <button type="button" className="team-settings__btn team-settings__btn--done" onClick={cloud.closeDialog}>
          Done
        </button>
      </div>
    </TeamDialog>
  );
}
