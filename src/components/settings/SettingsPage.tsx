import { useEffect, useRef, useState, type ReactNode } from "react";
import * as api from "../../cloud/api";
import { useCloud } from "../../cloud/CloudProvider";
import { roleLabel } from "../../cloud/teams";
import type { AppTab } from "../../ipc/types";
import { cx } from "../../lib/cx";
import { useAppActions, useAppState } from "../../state/AppProvider";
import type { SettingsPage as Page } from "../../state/reducer";
import { Avatar, TeamTile } from "../Account";
import { AccountSettings } from "./AccountSettings";
import { GeneralSettings } from "./GeneralSettings";
import { TeamSettings } from "./TeamSettings";

const TAB_NAMES: Record<AppTab, string> = { process: "Process", terminals: "Terminals", notes: "Notes" };

/** The left column's glyphs (App Shell board 03). */
const GLYPHS = {
  sliders: "M4 7h9M17 7h3M4 17h3M11 17h9M15 5v4M9 15v4",
  user: "M15 8.5a3 3 0 1 1-6 0 3 3 0 0 1 6 0zM5.5 19a6.5 6.5 0 0 1 13 0",
  plus: "M12 5v14M5 12h14",
  join: "M10 17l5-5-5-5M15 12H3M15 4h4v16h-4",
} as const;

/**
 * Settings (App Shell board 03): a page in place of the current tab, not a
 * dialog. The top bar stays, with no tab selected and Settings lit. Back or
 * Escape returns to the tab you came from, where you left it. The left column
 * lists General, Account and, signed in, each team plus Create and Join
 * (which open their dialogs over this page). Changes save as they're made.
 */
export function SettingsPage() {
  const state = useAppState();
  const actions = useAppActions();
  const cloud = useCloud();
  const showTeams = cloud.signedIn && cloud.has("teams");
  const memberCounts = useMemberCounts(showTeams ? cloud.teams.map((team) => team.id) : []);

  // A team you left, or one from before signing out, falls back to General.
  const requested = state.settingsPage;
  const page: Page =
    requested.page === "team" && !(showTeams && cloud.teams.some((team) => team.id === requested.teamId))
      ? { page: "general" }
      : requested;
  const open = (next: Page) => actions.setSettingsOpen(true, next);
  const close = () => actions.setSettingsOpen(false);

  // Escape goes back, unless a dialog over the page has it.
  const dialogOpen = useRef(false);
  dialogOpen.current = cloud.dialog !== null || state.hotkeysOpen;
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || dialogOpen.current || event.defaultPrevented) return;
      event.preventDefault();
      actions.setSettingsOpen(false);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [actions]);

  // Each section starts at the top.
  const panel = useRef<HTMLDivElement>(null);
  const pageKey = page.page === "team" ? `team:${page.teamId}` : page.page;
  useEffect(() => {
    panel.current?.scrollTo({ top: 0 });
  }, [pageKey]);

  const user = cloud.user;

  return (
    <div className="settings-page">
      <nav className="settings-nav" aria-label="Settings">
        <button type="button" className="settings-nav__back" onClick={close}>
          <span aria-hidden="true">‹</span>
          Back to {TAB_NAMES[state.activeTab]}
          <span className="settings-nav__key">Esc</span>
        </button>
        <div className="settings-nav__rule" />

        <NavItem
          active={page.page === "general"}
          icon={<Glyph d={GLYPHS.sliders} />}
          label="General"
          sub="Shell, hotkeys, font, notes folder"
          onClick={() => open({ page: "general" })}
        />
        <NavItem
          active={page.page === "account"}
          icon={user ? <Avatar name={user.name || user.login} size={26} /> : <Glyph d={GLYPHS.user} />}
          label="Account"
          sub={user ? `@${user.login} · devices, notes sync` : "Not signed in"}
          onClick={() => open({ page: "account" })}
        />

        {showTeams && (
          <>
            <div className="settings-nav__group">Teams</div>
            {cloud.teams.map((team) => {
              const members = memberCounts[team.id];
              return (
                <NavItem
                  key={team.id}
                  active={page.page === "team" && page.teamId === team.id}
                  icon={<TeamTile name={team.name} size={26} />}
                  label={team.name}
                  sub={
                    members === undefined
                      ? roleLabel(team.role)
                      : `${roleLabel(team.role)} · ${members} ${members === 1 ? "member" : "members"}`
                  }
                  onClick={() => open({ page: "team", teamId: team.id })}
                />
              );
            })}
            <NavItem
              icon={<Glyph d={GLYPHS.plus} />}
              label="Create a team…"
              onClick={() => cloud.openDialog({ kind: "create-team" })}
            />
            <NavItem icon={<Glyph d={GLYPHS.join} />} label="Join a team…" onClick={() => cloud.openDialog({ kind: "join" })} />
          </>
        )}

        <span className="settings-nav__spacer" />
        <div className="settings-nav__saved">
          <span className="cloud-dot cloud-dot--ok" aria-hidden="true" />
          Changes save as you make them
        </div>
      </nav>

      <div className="settings-panel" ref={panel}>
        <div className="settings-panel__content">
          {page.page === "general" && <GeneralSettings />}
          {page.page === "account" && <AccountSettings />}
          {page.page === "team" && <TeamSettings key={page.teamId} teamId={page.teamId} />}
        </div>
      </div>
    </div>
  );
}

/** A section's title and the line under it. */
export function SettingsHeading({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="settings-heading">
      <h1 className="settings-heading__title">{title}</h1>
      <p className="settings-heading__sub">{children}</p>
    </div>
  );
}

function NavItem({
  active = false,
  icon,
  label,
  sub,
  onClick,
}: {
  active?: boolean;
  icon: ReactNode;
  label: string;
  sub?: string;
  onClick(): void;
}) {
  return (
    <button
      type="button"
      className={cx("settings-nav__item", active && "settings-nav__item--active")}
      aria-current={active ? "page" : undefined}
      onClick={onClick}
    >
      {icon}
      <span className="settings-nav__text">
        <span className="settings-nav__label">{label}</span>
        {sub && <span className="settings-nav__sub">{sub}</span>}
      </span>
    </button>
  );
}

function Glyph({ d }: { d: string }) {
  return (
    <span className="settings-nav__glyph" aria-hidden="true">
      <svg width="15" height="15" viewBox="0 0 24 24" focusable="false">
        <path d={d} fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    </span>
  );
}

/** Each team's member count, for the left column. Fetched once per page visit. */
function useMemberCounts(teamIds: string[]): Record<string, number> {
  const [counts, setCounts] = useState<Record<string, number>>({});
  const key = teamIds.join(",");
  useEffect(() => {
    let live = true;
    for (const id of key ? key.split(",") : []) {
      api
        .getTeam(id)
        .then((detail) => live && setCounts((current) => ({ ...current, [id]: detail.members.length })))
        .catch(() => {});
    }
    return () => {
      live = false;
    };
  }, [key]);
  return counts;
}
