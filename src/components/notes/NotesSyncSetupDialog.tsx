import { useEffect, useRef, useState, type ReactNode, type Ref } from "react";
import { useCloud } from "../../cloud/CloudProvider";
import { cx } from "../../lib/cx";
import { joinRoot } from "../../notesSync/paths";
import { teamFolderFor } from "../../notesSync/store";
import { useAppState } from "../../state/AppProvider";
import { Avatar, TeamTile } from "../Account";

/**
 * notes-sync-setup (Notes board 02): which notes sync. Asked once after sign
 * in and from Settings → Notes sync → Change. Both are on in the dialog the
 * first time; nothing syncs until "Turn on sync". Re-opened, it shows the
 * current choice.
 */
export function NotesSyncSetupDialog(_props: Record<string, never>) {
  const cloud = useCloud();
  const { notes } = useAppState();
  const teams = cloud.has("teams") ? cloud.teams : [];
  const current = cloud.notesSync;

  const [personal, setPersonal] = useState(current ? current.personal : true);
  const [teamIds, setTeamIds] = useState<string[]>(() =>
    current ? current.teams : teams.map((team) => team.id),
  );
  const first = useRef<HTMLButtonElement>(null);
  const root = notes.root ?? "";

  const close = cloud.closeDialog;
  const turnOn = () => {
    cloud.setNotesSync({ personal, teams: teamIds.filter((id) => teams.some((team) => team.id === id)) });
    close();
  };
  const notNow = () => {
    // Remembered, so it isn't asked again after the next sign in.
    cloud.setNotesSync({ personal: false, teams: [] });
    close();
  };

  // Without the server's sync feature there is nothing to ask.
  const syncable = cloud.signedIn && cloud.has("sync");
  useEffect(() => {
    if (!syncable) close();
  }, [syncable, close]);

  useEffect(() => {
    first.current?.focus();
  }, []);

  const submit = useRef(turnOn);
  submit.current = turnOn;
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        close();
      } else if (event.key === "Enter") {
        event.preventDefault();
        event.stopPropagation();
        submit.current();
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [close]);

  if (!syncable) return null;

  const toggleTeam = (id: string) =>
    setTeamIds((ids) => (ids.includes(id) ? ids.filter((other) => other !== id) : [...ids, id]));

  return (
    <div
      className="cloud-scrim"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) close();
      }}
    >
      <div
        className="cloud-dialog notes-sync-setup"
        role="dialog"
        aria-modal="true"
        aria-labelledby="notes-sync-setup-title"
        style={{ ["--cloud-dialog-width" as string]: "620px" }}
      >
        <div className="cloud-dialog__head">
          <h2 id="notes-sync-setup-title" className="cloud-dialog__title">
            Sync your notes?
          </h2>
          <button type="button" className="cloud-dialog__close" aria-label="Close" onClick={close}>
            ×
          </button>
        </div>

        <div className="cloud-dialog__body notes-sync-setup__body">
          <p className="cloud-dialog__text">
            Notes keep working offline either way. Sync copies them to your account so they follow you to other
            devices.
          </p>

          <SyncOption
            icon={<Avatar name={cloud.user?.name || cloud.user?.login || "?"} size={28} />}
            title="Personal notes"
            text="Only you can see them. They sync to every device you sign in on."
            path={root}
            on={personal}
            onToggle={() => setPersonal((on) => !on)}
            switchRef={first}
          />

          {teams.map((team) => (
            <SyncOption
              key={team.id}
              icon={<TeamTile name={team.name} size={28} />}
              title={`${team.name} notes`}
              text="A shared folder. Every member can read and edit it. Markdown files only."
              path={root ? joinRoot(root, teamFolderFor(team.id, team.name, false)) : ""}
              on={teamIds.includes(team.id)}
              onToggle={() => toggleTeam(team.id)}
            />
          ))}

          <p className="notes-sync-setup__small">
            If two people change the same note, both versions are kept and you pick. Each note can be up to 1 MB.
          </p>
        </div>

        <div className="cloud-dialog__foot">
          <span className="cloud-dialog__spacer" />
          <button type="button" className="cloud-btn cloud-btn--quiet" onClick={notNow}>
            Not now
          </button>
          <button type="button" className="cloud-btn cloud-btn--primary" onClick={turnOn}>
            Turn on sync
          </button>
        </div>
      </div>
    </div>
  );
}

interface SyncOptionProps {
  icon: ReactNode;
  title: string;
  text: string;
  path: string;
  on: boolean;
  onToggle: () => void;
  switchRef?: Ref<HTMLButtonElement>;
}

/** One option card: what syncs, where it lives, and its switch. */
function SyncOption({ icon, title, text, path, on, onToggle, switchRef }: SyncOptionProps) {
  return (
    <div className={cx("notes-sync-option", on && "notes-sync-option--on")}>
      <div className="notes-sync-option__icon">{icon}</div>
      <div className="notes-sync-option__copy">
        <div className="notes-sync-option__title">{title}</div>
        <div className="notes-sync-option__text">{text}</div>
        {path && <div className="notes-sync-option__path">{path}</div>}
      </div>
      <button
        ref={switchRef}
        type="button"
        role="switch"
        aria-checked={on}
        aria-label={`Sync ${title.toLowerCase()}`}
        className={cx("notes-sync-switch", on && "notes-sync-switch--on")}
        onClick={onToggle}
      >
        <span className="notes-sync-switch__knob" aria-hidden="true" />
      </button>
    </div>
  );
}
