import { useEffect, useState } from "react";
import * as api from "../../cloud/api";
import { useCloud } from "../../cloud/CloudProvider";
import { countLiveConnections, countLiveProcesses, joinedSummary, personalSummary } from "../../cloud/teams";
import type { Team } from "../../cloud/types";
import { useAppActions } from "../../state/AppProvider";
import { initialOf } from "../Account";
import { TeamDialog, TeamDialogHead, TeamHeadTile } from "./TeamDialog";

/**
 * team-joined (App Shell board 08): Personal is untouched, pick where to go
 * next. The team's member, process and connection counts are fetched here;
 * the sentence leaves out whatever hasn't arrived.
 */
export function JoinedDialog({ team }: { team: Team }) {
  const cloud = useCloud();
  const appActions = useAppActions();
  const [members, setMembers] = useState<number | null>(null);
  const [processes, setProcesses] = useState<number | null>(null);
  const [connections, setConnections] = useState<number | null>(null);
  const showProcesses = cloud.has("processes");
  const showConnections = cloud.has("connections");

  useEffect(() => {
    let cancelled = false;
    void api.getTeam(team.id).then(
      (detail) => !cancelled && setMembers(detail.members.length),
      () => {},
    );
    if (showProcesses) {
      void countLiveProcesses((since) => api.listTeamProcesses(team.id, since)).then(
        (count) => !cancelled && setProcesses(count),
        () => {},
      );
    }
    if (showConnections) {
      void countLiveConnections((since) => api.listTeamConnections(team.id, since)).then(
        (count) => !cancelled && setConnections(count),
        () => {},
      );
    }
    return () => {
      cancelled = true;
    };
  }, [team.id, showProcesses, showConnections]);

  const openTeam = () => {
    cloud.selectSpace(team.id);
    appActions.setTab("process");
    cloud.closeDialog();
  };
  const me = cloud.user?.name || cloud.user?.login || "";

  return (
    <TeamDialog label={`You joined ${team.name}`} width={600} onClose={cloud.closeDialog} onSubmit={openTeam}>
      <TeamDialogHead tile={<TeamHeadTile name={team.name} />} title={`You joined ${team.name}`} onClose={cloud.closeDialog} />
      <div className="cloud-dialog__body">
        <div className="team-joined">
          <div className="team-joined__col team-joined__col--personal">
            <div className="team-joined__name">
              <span className="team-joined__avatar" aria-hidden="true">
                {initialOf(me)}
              </span>
              Personal
            </div>
            <p className="team-joined__text">{personalSummary(cloud.processCounts.personal)}</p>
          </div>
          <div className="team-joined__col">
            <div className="team-joined__name">
              <span className="team-joined__tile" aria-hidden="true">
                {initialOf(team.name)}
              </span>
              {team.name}
            </div>
            <p className="team-joined__text">
              {joinedSummary({
                members,
                processes: showProcesses ? processes : null,
                connections: showConnections ? connections : null,
                role: team.role,
              })}
            </p>
          </div>
        </div>
        <p className="cloud-dialog__small">
          Switch any time from the account chip.
          {cloud.has("sync") && ` ${team.name}'s shared notes folder appears in Notes once sync is on.`} Terminals are
          never shared.
        </p>
      </div>
      <div className="cloud-dialog__foot">
        <button type="button" className="cloud-btn cloud-btn--quiet" onClick={cloud.closeDialog}>
          Stay on Personal
        </button>
        <span className="cloud-dialog__spacer" />
        <button type="button" className="cloud-btn cloud-btn--primary" onClick={openTeam}>
          Open {team.name} processes
        </button>
      </div>
    </TeamDialog>
  );
}
