import { useCloud } from "../../cloud/CloudProvider";
import { NotesSyncSetupDialog } from "../notes/NotesSyncSetupDialog";
import { CreateTeamDialog } from "./CreateTeamDialog";
import { JoinedDialog } from "./JoinedDialog";
import { JoinSignedOutDialog } from "./JoinSignedOutDialog";
import { JoinTeamDialog } from "./JoinTeamDialog";
import { SignInDialog } from "./SignInDialog";
import { TeamSettingsDialog } from "./TeamSettingsDialog";

/** Whichever cloud dialog is open (useCloud().openDialog), over the whole window. */
export function CloudDialogs() {
  const { dialog } = useCloud();
  switch (dialog?.kind) {
    case undefined:
      return null;
    case "sign-in":
      return <SignInDialog joinCode={dialog.joinCode} />;
    case "join":
      return <JoinTeamDialog code={dialog.code} errorStatus={dialog.errorStatus} />;
    case "join-signed-out":
      return <JoinSignedOutDialog code={dialog.code} />;
    case "joined":
      return <JoinedDialog team={dialog.team} />;
    case "create-team":
      return <CreateTeamDialog />;
    case "team-settings":
      return <TeamSettingsDialog teamId={dialog.teamId} />;
    case "notes-sync-setup":
      return <NotesSyncSetupDialog />;
  }
}
