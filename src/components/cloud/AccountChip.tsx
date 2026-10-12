import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { useCloud } from "../../cloud/CloudProvider";
import { cx } from "../../lib/cx";
import { useSolePopover } from "../../lib/popover";
import { useAppActions } from "../../state/AppProvider";
import { Avatar, StrokeIcon, TeamTile } from "../Account";
import { useDismiss } from "../process/useDismiss";
import { spaceDetail } from "./accountText";

/**
 * The top bar's account chip (App Shell board 01) and its menu, team-switch
 * (board 08): who is signed in, which space the Process tab shows, and
 * whether the server is reachable. Signed out it isn't there at all.
 */
export function AccountChip(_props: Record<string, never>) {
  const cloud = useCloud();
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(open, root, close);
  useSolePopover(open, close);

  // Signing out (here or by the server) closes the menu with the chip.
  useEffect(() => {
    if (!cloud.user) setOpen(false);
  }, [cloud.user]);

  if (!cloud.user) return null;

  const name = cloud.user.name || cloud.user.login;
  const team = cloud.has("teams") ? cloud.selectedTeam : null;
  const offline = !cloud.online;
  const tooltip = `${name} · ${team ? `${team.name} team` : "Personal"}${offline ? " · offline" : ""}`;

  return (
    <div className="account-chip" ref={root}>
      <button
        type="button"
        className={cx("account-chip__button", open && "account-chip__button--open")}
        title={tooltip}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((current) => !current)}
      >
        <span className="account-chip__faces">
          {team && <TeamTile name={team.name} size={18} />}
          <span className="account-chip__avatar">
            <Avatar name={name} size={20} />
            {offline && <span className="account-chip__offline-dot" aria-hidden="true" />}
          </span>
        </span>
        <span className="account-chip__label">{team ? team.name : "Personal"}</span>
        {offline && <span className="account-chip__offline">Offline</span>}
      </button>
      {open && <AccountMenu onClose={close} />}
    </div>
  );
}

/** team-switch: Personal and each team, then the team and account actions. */
function AccountMenu({ onClose }: { onClose: () => void }) {
  const cloud = useCloud();
  const actions = useAppActions();
  const menu = useRef<HTMLDivElement>(null);
  const teams = cloud.has("teams") ? cloud.teams : [];
  const selected = cloud.has("teams") ? cloud.selectedSpace : "personal";
  const selectedTeam = cloud.has("teams") ? cloud.selectedTeam : null;
  const user = cloud.user;

  // Focus moves into the menu, so Escape (useDismiss listens on the chip) closes it.
  useEffect(() => {
    menu.current?.querySelector<HTMLElement>("[role^=menuitem]")?.focus();
  }, []);

  if (!user) return null;

  const run = (action: () => void) => () => {
    onClose();
    action();
  };

  return (
    <div className="account-menu" role="menu" aria-label="Account" ref={menu}>
      <div className="account-menu__label">Signed in as @{user.login}</div>
      <SpaceRow
        title="Personal"
        detail={spaceDetail("personal", cloud.processCounts.personal)}
        selected={selected === "personal"}
        face={<Avatar name={user.name || user.login} size={28} />}
        onPick={run(() => cloud.selectSpace("personal"))}
      />
      {teams.map((team) => (
        <SpaceRow
          key={team.id}
          title={team.name}
          detail={spaceDetail(team, cloud.processCounts[team.id])}
          selected={selected === team.id}
          face={<TeamTile name={team.name} size={28} />}
          onPick={run(() => cloud.selectSpace(team.id))}
        />
      ))}
      <div className="account-menu__divider" role="separator" />
      {cloud.has("teams") && (
        <>
          <MenuItem onClick={run(() => cloud.openDialog({ kind: "join" }))}>Join a team…</MenuItem>
          <MenuItem onClick={run(() => cloud.openDialog({ kind: "create-team" }))}>Create a team…</MenuItem>
          <div className="account-menu__divider" role="separator" />
        </>
      )}
      {selectedTeam && (
        <MenuItem onClick={run(() => cloud.openDialog({ kind: "team-settings", teamId: selectedTeam.id }))}>
          {selectedTeam.name} settings
        </MenuItem>
      )}
      <MenuItem onClick={run(() => actions.setSettingsOpen(true))}>Account settings</MenuItem>
      <MenuItem onClick={run(() => void cloud.signOut())}>Sign out</MenuItem>
    </div>
  );
}

function SpaceRow({
  title,
  detail,
  selected,
  face,
  onPick,
}: {
  title: string;
  detail: string;
  selected: boolean;
  face: ReactNode;
  onPick: () => void;
}) {
  return (
    <button
      type="button"
      role="menuitemradio"
      aria-checked={selected}
      className={cx("account-menu__space", selected && "account-menu__space--selected")}
      onClick={onPick}
    >
      {face}
      <span className="account-menu__space-text">
        <span className="account-menu__space-title">{title}</span>
        <span className="account-menu__space-detail">{detail}</span>
      </span>
      {selected && (
        <span className="account-menu__check">
          <StrokeIcon name="check" size={15} weight={2} />
        </span>
      )}
    </button>
  );
}

function MenuItem({ onClick, children }: { onClick: () => void; children: ReactNode }) {
  return (
    <button type="button" role="menuitem" className="account-menu__item" onClick={onClick}>
      {children}
    </button>
  );
}
