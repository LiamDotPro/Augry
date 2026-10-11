import { useEffect, useRef, useState } from "react";
import { TEMPLATES } from "../../process/examples";
import { startChip, startKindInfo } from "../../process/model";
import { agoShort, paneFor, runHeadline, runningStep, waitActions, waitingStep, whenLabel } from "../../process/runs";
import type { ProcessRecord, Run, Tone } from "../../process/types";
import { latest, working } from "../../process/versions";
import { cx } from "../../lib/cx";
import { MiniLanes } from "./MiniLanes";
import { ProcessIcon } from "./ProcessIcon";
import { useProcess } from "./store";
import { useKeptScroll } from "./useKeptScroll";

/** Recent runs shown before All runs is pressed. */
const RECENT = 3;

/**
 * Processes home (board 01, "home"): where the tab opens. Each process has
 * Run, each template has Copy and run, and runs have their own column, so
 * running and editing never share a screen.
 */
export function ProcessHome() {
  const { state, dispatch } = useProcess();
  const [query, setQuery] = useState("");
  const scroll = useKeptScroll<HTMLDivElement>("home");
  const q = query.trim().toLowerCase();
  const records = state.records.filter((r) => !q || working(r).name.toLowerCase().includes(q));
  const templates = TEMPLATES.filter((t) => !q || t.name.toLowerCase().includes(q) || t.desc.toLowerCase().includes(q));

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
          <button type="button" className="process-btn" onClick={() => dispatch({ type: "import-open", open: true })}>
            Import JSON
          </button>
          <button type="button" className="process-btn" onClick={() => dispatch({ type: "new-process", now: Date.now() })}>
            <span className="process-btn__plus" aria-hidden="true">+</span>
            New process
          </button>
        </div>

        <div className="process-home__section">
          <h2 className="process-home__h2">Your processes</h2>
          <span className="process-home__hint">augry · {state.records.length}</span>
        </div>
        {records.length ? (
          <div className="process-home__cards">
            {records.map((record) => (
              <ProcessCard key={record.id} record={record} />
            ))}
          </div>
        ) : (
          <p className="process-home__empty">{q ? "No process matches." : "No processes yet. Copy a template to start."}</p>
        )}

        <div className="process-home__section process-home__section--templates">
          <h2 className="process-home__h2">Templates</h2>
          <span className="process-home__hint">Copy one into this repo and run it straight away</span>
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
                  <button
                    type="button"
                    className="process-btn process-btn--small process-btn--solid"
                    onClick={() => dispatch({ type: "sheet", templateId: t.id })}
                  >
                    Copy and run
                  </button>
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
  const { state, dispatch, now } = useProcess();
  const process = working(record);
  const lastRun = state.runs.find((r) => r.processId === record.id);
  const status = cardStatus(lastRun, now);
  const version = latest(record).version;

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
      <div className="process-card__actions">
        <button
          type="button"
          className="process-btn process-btn--primary process-btn--run"
          onClick={() => dispatch({ type: "run", processId: record.id, now: Date.now() })}
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
  const { dispatch } = useProcess();
  const [open, setOpen] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const root = useRef<HTMLDivElement>(null);

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
          <button
            type="button"
            role="menuitem"
            className="process-menu__item"
            onClick={() => {
              dispatch({ type: "duplicate", processId: record.id, now: Date.now() });
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
            onClick={() => (confirm ? dispatch({ type: "delete", processId: record.id }) : setConfirm(true))}
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
  const { state, dispatch, now, onOpenPane } = useProcess();
  const [all, setAll] = useState(false);
  const scroll = useKeptScroll<HTMLDivElement>("home-runs");
  const nameOf = (run: Run) => {
    const record = state.records.find((r) => r.id === run.processId);
    return record ? working(record).name : run.process.name;
  };
  const open = (run: Run) => dispatch({ type: "navigate", route: { view: "run", runId: run.id } });
  const need = state.runs.filter((r) => r.status === "waiting");
  const running = state.runs.filter((r) => r.status === "running");
  const past = state.runs.filter((r) => r.status === "done" || r.status === "stopped");
  const watching = state.records.map(latest).filter((v) => v.process.start.on === "folder");

  return (
    <aside className="process-runs" aria-label="Runs" ref={scroll.ref} onScroll={scroll.onScroll}>
      <div className="process-runs__head">
        <h2 className="process-runs__title">Runs</h2>
        <button type="button" className="process-link" aria-pressed={all} onClick={() => setAll((v) => !v)}>
          {all ? "Recent runs" : "All runs"}
        </button>
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
        {(all ? past : past.slice(0, RECENT)).map((run) => (
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
