import { useEffect, useRef, useState } from "react";
import { useCloud } from "../../cloud/CloudProvider";
import { blankProcess, TEMPLATES } from "../../process/examples";
import { startChip, startKindInfo, uniqueId } from "../../process/model";
import { agoShort, clock, paneFor, runHeadline, runningStep, waitActions, waitingStep, whenLabel } from "../../process/runs";
import { scanSecrets } from "../../process/secrets";
import { recordOf, spaceRecords, spaceRuns } from "../../process/state";
import { savedLine } from "../../process/team";
import type { ProcessRecord, Run, Tone } from "../../process/types";
import { latest, working } from "../../process/versions";
import { cx } from "../../lib/cx";
import { MiniLanes } from "./MiniLanes";
import { ProcessIcon } from "./ProcessIcon";
import { RUN_OFFLINE, SAVE_OFFLINE, useProcess } from "./store";
import { useKeptScroll } from "./useKeptScroll";

/** Recent runs shown before All runs is pressed. */
const RECENT = 3;

/**
 * Processes home (board 01, "home"): where the tab opens. Each process has
 * Run, each template has Copy and run, and runs have their own column, so
 * running and editing never share a screen. With a team selected (board 11,
 * home-team) it lists the team's processes, templates are added to the team,
 * and the Runs column says runs stay on this computer.
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
  const templates = TEMPLATES.filter((t) => !q || t.name.toLowerCase().includes(q) || t.desc.toLowerCase().includes(q));
  const offline = !!team && !team.online;

  const newProcess = () => {
    if (!team) return dispatch({ type: "new-process", now: Date.now() });
    const name = "New process";
    void team.create(blankProcess(uniqueId(name, shown.map((r) => working(r).id)), name), "start");
  };
  const addTemplate = (templateId: string) => {
    if (!team || adding) return;
    setAdding(templateId);
    void team.addTemplate(templateId).finally(() => setAdding(null));
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
              <span className="process-home__hint">augry · {state.records.length}</span>
            </>
          )}
        </div>
        {records.length ? (
          <div className="process-home__cards">
            {records.map((record) => (
              <ProcessCard key={record.id} record={record} />
            ))}
          </div>
        ) : (
          <p className="process-home__empty">
            {q ? "No process matches." : team ? `No ${team.team.name} processes yet. Add a template or share one of yours.` : "No processes yet. Copy a template to start."}
          </p>
        )}

        <div className="process-home__section process-home__section--templates">
          <h2 className="process-home__h2">Templates</h2>
          <span className="process-home__hint">
            {team ? `Add one to ${team.team.name} so everyone can run it` : "Copy one into this repo and run it straight away"}
          </span>
        </div>
        <div className="process-home__templates">
          {templates.map((t) => (
            <div key={t.id} className="process-template">
              <MiniLanes process={t.process} />
              <div className="process-template__body">
                <div className="process-template__name">{t.name}</div>
                <div className="process-template__desc">{t.desc}</div>
                <div className="process-template__foot">
                  <StartChip icon={startKindInfo(t.start).icon} text={t.chip} />
                  {team ? (
                    <button
                      type="button"
                      className="process-btn process-btn--small process-btn--solid"
                      disabled={offline || adding === t.id}
                      title={offline ? SAVE_OFFLINE : undefined}
                      onClick={() => addTemplate(t.id)}
                    >
                      Add to {team.team.name}
                    </button>
                  ) : (
                    <button
                      type="button"
                      className="process-btn process-btn--small process-btn--solid"
                      onClick={() => dispatch({ type: "sheet", templateId: t.id })}
                    >
                      Copy and run
                    </button>
                  )}
                </div>
              </div>
            </div>
          ))}
        </div>
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

function cardStatus(run: Run | undefined, now: number): { text: string; tone: Tone } {
  if (!run) return { text: "Not run yet", tone: "muted" };
  if (run.status === "waiting") return { text: `Run ${run.number} · waiting for you`, tone: "warn" };
  if (run.status === "running") return { text: `Run ${run.number} · running`, tone: "accent" };
  const when = whenLabel(run.endedAt ?? run.startedAt, now);
  if (run.status === "stopped") return { text: `Last run ${when} · stopped`, tone: "danger" };
  return { text: `Last run ${when} · ${run.result ?? "finished"}`, tone: "ok" };
}

function ProcessCard({ record }: { record: ProcessRecord }) {
  const { state, dispatch, now, team, runProcess, offline } = useProcess();
  const process = working(record);
  const lastRun = state.runs.find((r) => r.processId === record.id);
  const status = cardStatus(lastRun, now);
  const saved = latest(record);
  const version = saved.version;
  const cantRun = offline(record.id);

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
      {team ? (
        // A team card says who saved the latest version (board 11).
        <div className="process-card__status">
          <span className="process-dot process-dot--team" />
          {savedLine(team.nameOf(saved.byId), version, saved.at, now)}
        </div>
      ) : (
        <button
          type="button"
          className="process-card__status"
          disabled={!lastRun}
          onClick={() => lastRun && dispatch({ type: "navigate", route: { view: "run", runId: lastRun.id } })}
          title={lastRun ? `Open run ${lastRun.number}` : undefined}
        >
          <span className={cx("process-dot", `process-dot--${status.tone}`)} />
          {status.text}
        </button>
      )}
      <div className="process-card__actions">
        <button
          type="button"
          className={cx("process-btn process-btn--primary process-btn--run", cantRun && "process-btn--offline")}
          aria-disabled={cantRun || undefined}
          title={cantRun ? RUN_OFFLINE : undefined}
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

/** The Runs column: what needs you, what is running, and what finished. */
function RunsColumn() {
  const { state, dispatch, now, onOpenPane, team } = useProcess();
  const [all, setAll] = useState(false);
  const scroll = useKeptScroll<HTMLDivElement>("home-runs");
  const nameOf = (run: Run) => {
    const record = recordOf(state, run.processId);
    return record ? working(record).name : run.process.name;
  };
  const open = (run: Run) => dispatch({ type: "navigate", route: { view: "run", runId: run.id } });
  const runs = spaceRuns(state);
  const need = runs.filter((r) => r.status === "waiting");
  const running = runs.filter((r) => r.status === "running");
  const past = runs.filter((r) => r.status === "done" || r.status === "stopped");
  const watching = team ? [] : state.records.map(latest).filter((v) => v.process.start.on === "folder");

  return (
    <aside className="process-runs" aria-label="Runs" ref={scroll.ref} onScroll={scroll.onScroll}>
      <div className="process-runs__head">
        {team ? (
          <>
            <h2 className="process-runs__title">Your runs</h2>
            <span className="process-runs__where">On this computer</span>
          </>
        ) : (
          <>
            <h2 className="process-runs__title">Runs</h2>
            <button type="button" className="process-link" aria-pressed={all} onClick={() => setAll((v) => !v)}>
              {all ? "Recent runs" : "All runs"}
            </button>
          </>
        )}
      </div>

      <div className="process-runs__label process-runs__label--need">Needs you</div>
      {need.length === 0 && <div className="process-runs__none">Nothing is waiting for you.</div>}
      {need.map((run) => {
        const step = waitingStep(run);
        const actions = step ? waitActions(run, step) : null;
        return (
          <div key={run.id} className="process-run process-run--need">
            <div className="process-run__top">
              <button type="button" className="process-run__name" onClick={() => open(run)}>
                {nameOf(run)}
              </button>
              <span className="process-run__meta">
                Run {run.number} · {agoShort(run.since, now)}
              </span>
            </div>
            <div className="process-run__text process-run__text--warn">
              <span className="process-dot process-dot--warn" />
              {runHeadline(run)}
            </div>
            {step && actions && (
              <div className="process-run__buttons">
                <button type="button" className="process-btn process-btn--small process-btn--warn" onClick={() => open(run)}>
                  {actions.primary}
                </button>
                <button
                  type="button"
                  className="process-btn process-btn--small process-btn--soft"
                  onClick={() => dispatch({ type: "approve", runId: run.id, stepId: step, now: Date.now() })}
                >
                  {actions.approve}
                </button>
                <button
                  type="button"
                  className="process-btn process-btn--small process-btn--soft"
                  onClick={() => dispatch({ type: actions.backAction, runId: run.id, stepId: step, now: Date.now() })}
                >
                  {actions.back}
                </button>
              </div>
            )}
          </div>
        );
      })}

      <div className="process-runs__label">Running</div>
      {running.length === 0 && <div className="process-runs__none">No runs right now.</div>}
      {running.map((run) => {
        const step = runningStep(run);
        const pane = step ? paneFor(run, step) : null;
        return (
          <div key={run.id} className="process-run process-run--now">
            <div className="process-run__top">
              <button type="button" className="process-run__name" onClick={() => open(run)}>
                {nameOf(run)}
              </button>
              <span className="process-run__meta">
                Run {run.number} · {agoShort(run.startedAt, now)}
              </span>
            </div>
            <div className="process-run__line">
              <span className="process-dot process-dot--accent" />
              <span className="process-run__text process-run__text--accent">{runHeadline(run)}</span>
              <button
                type="button"
                className="process-btn process-btn--small"
                title={pane ? `Open pane ${pane} in Terminals` : "Open the run"}
                onClick={() => (pane ? onOpenPane(pane) : open(run))}
              >
                Open
              </button>
            </div>
          </div>
        );
      })}

      <div className="process-runs__label">Recent</div>
      <div className="process-runs__recent">
        {(all && !team ? past : past.slice(0, RECENT)).map((run) => (
          <button key={run.id} type="button" className="process-recent" onClick={() => open(run)}>
            <span className={cx("process-recent__glyph", run.status === "stopped" && "process-recent__glyph--stopped")}>
              {run.status === "stopped" ? "×" : "✓"}
            </span>
            <span className="process-recent__body">
              <span className="process-recent__top">
                <span className="process-recent__name">{nameOf(run)}</span>
                <span className="process-recent__run">Run {run.number}</span>
              </span>
              <span className="process-recent__text">{run.result}</span>
            </span>
            <span className="process-recent__time">{whenLabel(run.endedAt ?? run.startedAt, now)}</span>
          </button>
        ))}
        {past.length === 0 && <div className="process-runs__none">No finished runs yet.</div>}
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
      {watching.length > 0 && (
        <div className="process-watch">
          <div className="process-watch__icon">
            <ProcessIcon name="folder" size={17} />
          </div>
          <div className="process-watch__body">
            <div className="process-watch__title">
              Watching {watching.length} folder{watching.length === 1 ? "" : "s"}
            </div>
            {watching.map((v) => (
              <div key={v.process.id} className="process-watch__text">
                {v.process.name} starts when a {String(v.process.start.match ?? "file").replace("*", "")} file lands in {String(v.process.start.path)}.{" "}
                <button
                  type="button"
                  className="process-link"
                  title="Starts a run as if a new file had arrived"
                  onClick={() => dispatch({ type: "file-arrived", processId: v.process.id, now: Date.now() })}
                >
                  Try it now
                </button>
              </div>
            ))}
          </div>
        </div>
      )}
    </aside>
  );
}
