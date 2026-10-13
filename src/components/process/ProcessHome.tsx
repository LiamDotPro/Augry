import { useEffect, useRef, useState, type ReactNode } from "react";
import { useCloud } from "../../cloud/CloudProvider";
import { blankProcess } from "../../process/examples";
import { cloneProcess } from "../../process/json";
import { startChip, startKindInfo, uniqueId } from "../../process/model";
import { scanSecrets } from "../../process/secrets";
import { spaceRecords } from "../../process/state";
import { clock } from "../../process/runs";
import { savedLine } from "../../process/team";
import type { ProcessRecord } from "../../process/types";
import { latest, working } from "../../process/versions";
import { cx } from "../../lib/cx";
import { MiniLanes } from "./MiniLanes";
import { ProcessIcon } from "./ProcessIcon";
import { SAVE_OFFLINE, useProcess } from "./store";
import { useKeptScroll } from "./useKeptScroll";
import type { LoadedTemplate } from "./useTemplates";

/**
 * Processes home (board 01, "home"): where the tab opens. Your processes are
 * files on this computer; templates come from the server and Use template
 * saves a copy you can edit. Runs have their own column, which says plainly
 * that nothing runs yet. With a team selected (board 11, home-team) it lists
 * the team's processes and templates are added to the team.
 */
export function ProcessHome() {
  const { state, dispatch, team } = useProcess();
  const cloud = useCloud();
  const [query, setQuery] = useState("");
  const [adding, setAdding] = useState<string | null>(null);
  const scroll = useKeptScroll<HTMLDivElement>("home");
  const q = query.trim().toLowerCase();
  const shown = spaceRecords(state);
  const records = shown.filter((r) => !q || working(r).name.toLowerCase().includes(q));
  const offline = !!team && !team.online;
  const loading = !team && state.disk.status === "loading";

  const newProcess = () => {
    if (!team) return dispatch({ type: "new-process", now: Date.now() });
    const name = "New process";
    void team.create(blankProcess(uniqueId(name, shown.map((r) => working(r).id)), name), "start");
  };
  const useTemplate = (template: LoadedTemplate) => {
    if (team) {
      if (adding) return;
      setAdding(template.slug);
      void team.addTemplate(template.slug, template.name).finally(() => setAdding(null));
    } else if (template.process) {
      dispatch({ type: "use-template", process: cloneProcess(template.process), name: template.name, now: Date.now() });
    }
  };

  return (
    <div className="process-home">
      <div className="process-home__main" ref={scroll.ref} onScroll={scroll.onScroll}>
        <div className="process-home__head">
          <div className="process-home__titles">
            <h1 className="process-home__title">Processes</h1>
            <p className="process-home__sub">Pick one and press Run. Edit only when you want to change how it works.</p>
          </div>
          <input
            className="process-input process-home__search"
            type="search"
            placeholder="Search processes and templates"
            aria-label="Search processes and templates"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <button type="button" className="process-btn" onClick={() => dispatch({ type: "navigate", route: { view: "connections" } })}>
            <ProcessIcon name="plug" size={15} />
            Connections
          </button>
          <button
            type="button"
            className="process-btn"
            disabled={offline}
            title={offline ? SAVE_OFFLINE : undefined}
            onClick={() => dispatch({ type: "import-open", open: true })}
          >
            Import JSON
          </button>
          <button type="button" className="process-btn" disabled={offline} title={offline ? SAVE_OFFLINE : undefined} onClick={newProcess}>
            <span className="process-btn__plus" aria-hidden="true">+</span>
            New process
          </button>
        </div>

        {team && offline && (
          <div className="team-offline" role="status">
            <span className="cloud-dot cloud-dot--ring" aria-hidden="true" />
            <span className="team-offline__text">
              You're offline. {team.team.name} processes can't run or save until you're back online. Personal processes still run.
            </span>
            <button type="button" className="team-offline__switch" onClick={() => cloud.selectSpace("personal")}>
              Switch to Personal
            </button>
          </div>
        )}
        {!team && state.disk.problems.length > 0 && (
          <div className="cloud-banner cloud-banner--warn team-note" role="status">
            <span className="team-note__text">
              {state.disk.problems.length === 1 ? "A file" : `${state.disk.problems.length} files`} in your processes folder couldn’t be read:{" "}
              {state.disk.problems.map((p) => `${p.id} (${p.message})`).join(", ")}.
            </span>
          </div>
        )}
        {state.teamNote && (
          <div className="cloud-banner cloud-banner--warn team-note" role="status">
            <span className="team-note__text">{state.teamNote}</span>
            <button type="button" className="process-x" aria-label="Dismiss" onClick={() => dispatch({ type: "team-note", note: null })}>
              ×
            </button>
          </div>
        )}

        <div className="process-home__section">
          {team ? (
            <>
              <h2 className="process-home__h2">{team.team.name} processes</h2>
              <span className="process-home__hint">{teamHint(team.online, team.members?.length ?? null, team.lastSyncedAt, shown.length)}</span>
            </>
          ) : (
            <>
              <h2 className="process-home__h2">Your processes</h2>
              <span className="process-home__hint">
                {loading ? "Reading your processes…" : `On this computer · ${state.records.length}`}
              </span>
            </>
          )}
        </div>
        {loading ? null : !team && state.disk.status === "failed" ? (
          <EmptyState icon="warn" title="Your processes couldn’t be read">
            The processes folder on this computer didn’t open: {state.disk.error}. Nothing in it has been changed.
          </EmptyState>
        ) : records.length ? (
          <div className="process-home__cards">
            {records.map((record) => (
              <ProcessCard key={record.id} record={record} />
            ))}
          </div>
        ) : q ? (
          <p className="process-home__empty">No process matches “{query.trim()}”.</p>
        ) : team ? (
          <EmptyState icon="share" title={`No ${team.team.name} processes yet`}>
            Add a template below so everyone in {team.team.name} can use it, or share one of your own from its ⋯ menu.
          </EmptyState>
        ) : (
          <EmptyState
            icon="doc"
            title="No processes yet"
            actions={
              <>
                <button type="button" className="process-btn process-btn--solid" onClick={newProcess}>
                  <span className="process-btn__plus" aria-hidden="true">+</span>
                  New process
                </button>
                <button type="button" className="process-btn" onClick={() => dispatch({ type: "import-open", open: true })}>
                  Import JSON
                </button>
              </>
            }
          >
            A process is a way of working you can repeat: which agents and terminals do what, in what order, and where
            you step in. Start from a template below, write one from scratch, or import a file.
          </EmptyState>
        )}

        <div className="process-home__section process-home__section--templates">
          <h2 className="process-home__h2">Templates</h2>
          <span className="process-home__hint">
            {team ? `Add one to ${team.team.name} so everyone can use it` : "Make your own copy, then change it however you like"}
          </span>
        </div>
        <Templates query={q} adding={adding} offline={offline} onUse={useTemplate} />
      </div>
      <RunsColumn />
    </div>
  );
}

/** "Shared with 6 members · 3", or offline "Last synced 10:42 · 3". */
function teamHint(online: boolean, members: number | null, syncedAt: number | undefined, count: number): string {
  if (!online) return syncedAt ? `Last synced ${clock(syncedAt)} · ${count}` : String(count);
  return members ? `Shared with ${members} member${members === 1 ? "" : "s"} · ${count}` : String(count);
}

export function StartChip({ icon, text }: { icon: string; text: string }) {
  return (
    <span className="process-chip">
      <ProcessIcon name={icon} size={12} stroke={1.9} />
      {text}
    </span>
  );
}

function ProcessCard({ record }: { record: ProcessRecord }) {
  const { dispatch, now, team, runProcess, runBlocked } = useProcess();
  const process = working(record);
  const saved = latest(record);
  const version = saved.version;
  const blocked = runBlocked(record.id);
  const cantRun = blocked !== null;

  return (
    <div className="process-card">
      <div className="process-card__top">
        <div className="process-card__name">{process.name}</div>
        <div className="process-card__version">
          v{version}
          {record.draft && <span className="process-card__draft"> · draft</span>}
        </div>
      </div>
      <div>
        <StartChip icon={startKindInfo(process.start.on).icon} text={startChip(process.start)} />
      </div>
      <MiniLanes process={process} big />
      {/* Who saved the latest version and when; a team card names the teammate (board 11). */}
      <div className="process-card__status">
        <span className={team ? "process-dot process-dot--team" : "process-dot process-dot--muted"} />
        {savedLine(team ? team.nameOf(saved.byId) : "You", version, saved.at, now)}
      </div>
      <div className="process-card__actions">
        <button
          type="button"
          className={cx("process-btn process-btn--primary process-btn--run", cantRun && "process-btn--offline")}
          aria-disabled={cantRun || undefined}
          title={blocked ?? undefined}
          onClick={() => !cantRun && runProcess(record.id)}
        >
          <span className="process-btn__play" aria-hidden="true">▶</span>
          Run
        </button>
        <button
          type="button"
          className="process-btn process-btn--tall"
          onClick={() => dispatch({ type: "navigate", route: { view: "editor", processId: record.id } })}
        >
          Edit
        </button>
        <MoreMenu record={record} />
      </div>
    </div>
  );
}

function MoreMenu({ record }: { record: ProcessRecord }) {
  const { dispatch, team, offline } = useProcess();
  const cloud = useCloud();
  const [open, setOpen] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  // Sharing makes a team copy of a personal process (board 11, share-to-team).
  const canShare = !team && cloud.signedIn && cloud.has("teams") && cloud.has("processes") && cloud.teams.length > 0;
  const cantWrite = offline(record.id);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  const close = () => {
    setOpen(false);
    setConfirm(false);
  };

  const share = () => {
    // The same check the server runs, so a token is caught before anything is sent.
    const found = scanSecrets(working(record))[0];
    dispatch({
      type: "team-dialog",
      dialog: found ? { kind: "share-blocked", processId: record.id, rule: found.rule, path: found.path, masked: found.masked } : { kind: "share", processId: record.id },
    });
    close();
  };

  return (
    <div className="process-menu" ref={root} onKeyDown={(e) => e.key === "Escape" && close()}>
      <button
        type="button"
        className="process-btn process-btn--icon"
        aria-label="More"
        title="More"
        aria-expanded={open}
        onClick={() => {
          setOpen((v) => !v);
          setConfirm(false);
        }}
      >
        ⋯
      </button>
      {open && (
        <div className="process-menu__list" role="menu">
          <button type="button" role="menuitem" className="process-menu__item" onClick={() => dispatch({ type: "navigate", route: { view: "versions", processId: record.id } })}>
            <ProcessIcon name="history" size={14} />
            Versions
          </button>
          {canShare && (
            <button type="button" role="menuitem" className="process-menu__item" onClick={share}>
              <ProcessIcon name="share" size={14} />
              Share to team…
            </button>
          )}
          <button
            type="button"
            role="menuitem"
            className="process-menu__item"
            disabled={cantWrite}
            title={cantWrite ? SAVE_OFFLINE : undefined}
            onClick={() => {
              if (team) void team.duplicate(record.id);
              else dispatch({ type: "duplicate", processId: record.id, now: Date.now() });
              close();
            }}
          >
            <ProcessIcon name="doc" size={14} />
            Duplicate
          </button>
          <button
            type="button"
            role="menuitem"
            className="process-menu__item process-menu__item--danger"
            disabled={cantWrite}
            title={cantWrite ? SAVE_OFFLINE : undefined}
            onClick={() => {
              if (!confirm) return setConfirm(true);
              if (team) {
                void team.remove(record.id);
                close();
              } else dispatch({ type: "delete", processId: record.id });
            }}
          >
            {confirm ? "Delete it, with its history" : "Delete"}
          </button>
        </div>
      )}
    </div>
  );
}

/**
 * The Runs column. There is no run engine yet, so it says so instead of
 * showing runs; with a team selected it also says where runs will happen.
 */
function RunsColumn() {
  const { team } = useProcess();
  const scroll = useKeptScroll<HTMLDivElement>("home-runs");

  return (
    <aside className="process-runs" aria-label="Runs" ref={scroll.ref} onScroll={scroll.onScroll}>
      <div className="process-runs__head">
        <h2 className="process-runs__title">{team ? "Your runs" : "Runs"}</h2>
        <span className="process-runs__where">On this computer</span>
      </div>
      <div className="process-runs__empty">
        <span className="process-runs__empty-icon" aria-hidden="true">
          <ProcessIcon name="trigger" size={18} />
        </span>
        <div className="process-runs__empty-title">Running processes isn’t available yet</div>
        <p className="process-runs__empty-text">
          When it is, this is where runs show: what needs you, what’s running, and what finished. For now you can
          build processes, save versions and share them.
        </p>
      </div>
      <div className="process-runs__fill" />
      {team && (
        <div className="process-watch">
          <div className="process-watch__icon">
            <ProcessIcon name="folder" size={17} />
          </div>
          <div className="process-watch__body">
            <div className="process-watch__title">Runs stay on this computer</div>
            <div className="process-watch__text">Teammates see the process and its versions. Runs happen on whoever started them.</div>
          </div>
        </div>
      )}
    </aside>
  );
}

/** A section with nothing in it yet: what it is for and what to do next. */
function EmptyState({ icon, title, children, actions }: { icon: string; title: string; children: ReactNode; actions?: ReactNode }) {
  return (
    <div className="process-empty">
      <span className="process-empty__icon" aria-hidden="true">
        <ProcessIcon name={icon} size={20} />
      </span>
      <div className="process-empty__body">
        <div className="process-empty__title">{title}</div>
        <p className="process-empty__text">{children}</p>
        {actions && <div className="process-empty__actions">{actions}</div>}
      </div>
    </div>
  );
}

/** The server's templates (GET /v1/templates): loading, couldn't load, none, or the cards. */
function Templates({
  query,
  adding,
  offline,
  onUse,
}: {
  query: string;
  adding: string | null;
  offline: boolean;
  onUse(template: LoadedTemplate): void;
}) {
  const { team, templates } = useProcess();
  const state = templates.state;

  if (state.status === "loading") {
    return (
      <div className="process-home__templates" aria-busy="true" aria-label="Loading templates">
        {[0, 1, 2].map((n) => (
          <div key={n} className="process-template process-template--loading" />
        ))}
      </div>
    );
  }
  if (state.status === "failed") {
    return (
      <EmptyState
        icon="warn"
        title="Templates couldn’t load"
        actions={
          <button type="button" className="process-btn" onClick={templates.retry}>
            Try again
          </button>
        }
      >
        {state.offline
          ? "Templates come from the Augry server, and this computer can’t reach it right now. Your own processes still work."
          : "The Augry server didn’t answer as expected. Your own processes still work."}
      </EmptyState>
    );
  }
  const shown = state.templates.filter((t) => !query || `${t.name} ${t.description}`.toLowerCase().includes(query));
  if (!state.templates.length) {
    return <p className="process-home__empty">No templates yet.</p>;
  }
  if (!shown.length) return <p className="process-home__empty">No template matches “{query}”.</p>;

  return (
    <div className="process-home__templates">
      {shown.map((t) => (
        <div key={t.slug} className="process-template">
          {t.process ? <MiniLanes process={t.process} /> : <div className="process-template__nolanes" />}
          <div className="process-template__body">
            <div className="process-template__name">{t.name}</div>
            <div className="process-template__desc">{t.description}</div>
            <div className="process-template__foot">
              <span className="process-chip">{t.category}</span>
              {team ? (
                <button
                  type="button"
                  className="process-btn process-btn--small process-btn--solid"
                  disabled={offline || adding === t.slug}
                  title={offline ? SAVE_OFFLINE : undefined}
                  onClick={() => onUse(t)}
                >
                  Add to {team.team.name}
                </button>
              ) : (
                <button
                  type="button"
                  className="process-btn process-btn--small process-btn--solid"
                  disabled={!t.process}
                  title={t.process ? "Saves a copy to your processes and opens it" : "This template couldn’t be read"}
                  onClick={() => onUse(t)}
                >
                  Use template
                </button>
              )}
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}
