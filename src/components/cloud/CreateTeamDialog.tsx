import { useState } from "react";
import { isCloudError } from "../../cloud/api";
import { useCloud } from "../../cloud/CloudProvider";
import { createErrorMessage, isValidTeamName, TEAM_NAME_MAX, TEAM_NAME_RULE } from "../../cloud/teams";
import { cx } from "../../lib/cx";
import { PlusTile, TeamDialog, TeamDialogHead, TeamFieldError } from "./TeamDialog";

/**
 * team-create (App Shell board 08). The new team is selected and its
 * settings open next, so inviting people is the obvious next step.
 */
export function CreateTeamDialog(_props: Record<string, never>) {
  const cloud = useCloud();
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const valid = isValidTeamName(name);
  const tooLong = name.trim().length > TEAM_NAME_MAX;
  const shownError = error ?? (tooLong ? TEAM_NAME_RULE : null);

  const create = async () => {
    if (busy || !valid) return;
    setBusy(true);
    try {
      const team = await cloud.createTeam(name);
      cloud.selectSpace(team.id);
      cloud.openDialog({ kind: "team-settings", teamId: team.id });
    } catch (failure) {
      setError(createErrorMessage(isCloudError(failure) ? failure.status : -1));
      setBusy(false);
    }
  };

  return (
    <TeamDialog label="Create a team" width={560} onClose={cloud.closeDialog} onSubmit={() => void create()}>
      <TeamDialogHead tile={<PlusTile />} title="Create a team" onClose={cloud.closeDialog} />
      <div className="cloud-dialog__body">
        <div className="cloud-field">
          <label className="cloud-field__label" htmlFor="team-create-name">
            Team name
          </label>
          <input
            id="team-create-name"
            className={cx("cloud-field__input", shownError && "cloud-field__input--error")}
            value={name}
            autoComplete="off"
            aria-invalid={shownError !== null}
            onChange={(event) => {
              setName(event.target.value);
              setError(null);
            }}
          />
          {shownError && <TeamFieldError>{shownError}</TeamFieldError>}
        </div>
        <p className="cloud-dialog__small">
          You'll be the owner. Invite people once it's created. Your personal processes stay where they are; share
          copies of them later if you want.
        </p>
      </div>
      <div className="cloud-dialog__foot">
        <button type="button" className="cloud-btn cloud-btn--quiet" onClick={cloud.closeDialog}>
          Cancel
        </button>
        <span className="cloud-dialog__spacer" />
        <button
          type="button"
          className="cloud-btn cloud-btn--primary"
          disabled={busy || !valid}
          onClick={() => void create()}
        >
          Create team
        </button>
      </div>
    </TeamDialog>
  );
}
