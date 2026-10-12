import { useCloud } from "../../cloud/CloudProvider";
import { GitHubMark } from "../Account";
import { PersonTile, TeamDialog, TeamDialogHead } from "./TeamDialog";

/**
 * team-join-signed-out (App Shell board 08): an invite link arrived while
 * signed out. Signing in keeps the code and accepts it straight after
 * (CloudProvider.completeSignIn), then shows team-joined or the join error.
 */
export function JoinSignedOutDialog({ code }: { code: string }) {
  const cloud = useCloud();
  const signIn = () => cloud.openDialog({ kind: "sign-in", joinCode: code });

  return (
    <TeamDialog label="Sign in to join the team" width={600} onClose={cloud.closeDialog} onSubmit={signIn}>
      <TeamDialogHead tile={<PersonTile />} title="Sign in to join the team" onClose={cloud.closeDialog} />
      <div className="cloud-dialog__body">
        <p className="cloud-dialog__text">
          You opened an invite link. Sign in with GitHub and you join straight after. The code is kept until then.
        </p>
        <div className="team-invite-code">
          <span className="team-invite-code__label">Invite</span>
          <span className="team-invite-code__value">{code}</span>
        </div>
      </div>
      <div className="cloud-dialog__foot">
        <button type="button" className="cloud-btn cloud-btn--quiet" onClick={cloud.closeDialog}>
          Not now
        </button>
        <span className="cloud-dialog__spacer" />
        <button type="button" className="cloud-btn cloud-btn--primary" onClick={signIn}>
          <GitHubMark size={15} />
          Continue with GitHub
        </button>
      </div>
    </TeamDialog>
  );
}
