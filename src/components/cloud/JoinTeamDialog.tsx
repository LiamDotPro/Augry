import { useState } from "react";
import { isCloudError } from "../../cloud/api";
import { useCloud } from "../../cloud/CloudProvider";
import { parseInviteCode } from "../../cloud/invite";
import { joinErrorMessage } from "../../cloud/teams";
import { cx } from "../../lib/cx";
import { PersonTile, TeamDialog, TeamDialogHead, TeamFieldError } from "./TeamDialog";

/**
 * team-join and team-join-error (App Shell board 08): paste an invite link
 * or code and accept it. `errorStatus` opens straight on that error, after a
 * sign in that went on to accept the invite.
 */
export function JoinTeamDialog({ code, errorStatus }: { code?: string; errorStatus?: number }) {
  const cloud = useCloud();
  const [value, setValue] = useState(code ?? "");
  /** The status whose error shows; a pasted text that isn't a code reads as 404. */
  const [status, setStatus] = useState<number | null>(errorStatus ?? null);
  const [busy, setBusy] = useState(false);

  const join = async () => {
    if (busy || !value.trim()) return;
    const parsed = parseInviteCode(value);
    if (!parsed) {
      setStatus(404);
      return;
    }
    setBusy(true);
    try {
      const team = await cloud.joinTeam(parsed);
      cloud.openDialog({ kind: "joined", team });
    } catch (error) {
      const failed = isCloudError(error) ? error.status : -1;
      setStatus(failed);
      // Already a member: the team list may just be out of date.
      if (failed === 409) void cloud.refreshTeams().catch(() => {});
      setBusy(false);
    }
  };

  return (
    <TeamDialog label="Join a team" width={560} onClose={cloud.closeDialog} onSubmit={() => void join()}>
      <TeamDialogHead tile={<PersonTile />} title="Join a team" onClose={cloud.closeDialog} />
      <div className="cloud-dialog__body">
        <p className="cloud-dialog__text">
          Paste the invite link or the code someone sent you. Opening an augry.dev/join link fills this in for you.
        </p>
        <div className="cloud-field">
          <label className="cloud-field__label" htmlFor="team-join-code">
            Invite link or code
          </label>
          <input
            id="team-join-code"
            className={cx(
              "cloud-field__input",
              "cloud-field__input--mono",
              status !== null && "cloud-field__input--error",
            )}
            value={value}
            spellCheck={false}
            autoComplete="off"
            aria-invalid={status !== null}
            aria-describedby="team-join-note"
            onChange={(event) => {
              setValue(event.target.value);
              setStatus(null);
            }}
          />
          {status === null ? (
            <div id="team-join-note" className="cloud-field__hint">
              Each code works once and has an expiry date.
            </div>
          ) : (
            <TeamFieldError id="team-join-note">{joinErrorMessage(status)}</TeamFieldError>
          )}
        </div>
      </div>
      <div className="cloud-dialog__foot">
        <button type="button" className="cloud-btn cloud-btn--quiet" onClick={cloud.closeDialog}>
          Cancel
        </button>
        <span className="cloud-dialog__spacer" />
        <button
          type="button"
          className="cloud-btn cloud-btn--primary"
          disabled={busy || !value.trim()}
          onClick={() => void join()}
        >
          Join
        </button>
      </div>
    </TeamDialog>
  );
}
