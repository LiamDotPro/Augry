import { useCallback, useRef, useState, type ReactNode } from "react";
import { agoShort, clock } from "../../process/runs";
import type { Run } from "../../process/types";
import { working } from "../../process/versions";
import { cx } from "../../lib/cx";
import { ProcessIcon } from "./ProcessIcon";
import { useProcess } from "./store";
import { useDismiss } from "./useDismiss";

/** A small dropdown under a bar chip. */
function BarMenu({ label, trigger, className, children }: { label: string; trigger: ReactNode; className: string; children(close: () => void): ReactNode }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(open, root, close);
  return (
    <div className="process-menu" ref={root}>
      <button type="button" className={className} aria-haspopup="menu" aria-expanded={open} aria-label={label} onClick={() => setOpen((v) => !v)}>
        {trigger}
      </button>
      {open && (
        <div className="process-menu__list process-menu__list--bar" role="menu">
          {children(close)}
        </div>
      )}
    </div>
  );
}

/** "‹ Processes / name ▼": back to the home, or over to another process's editor. */
function Crumb({ name, processId }: { name: string; processId: string }) {
  const { state, dispatch } = useProcess();
  return (
    <>
      <button type="button" className="process-bar__back" title="All processes" onClick={() => dispatch({ type: "navigate", route: { view: "home" } })}>
        <span className="process-bar__chev" aria-hidden="true">‹</span>
        Processes
      </button>
      <span className="process-bar__rule" />
      <BarMenu label={`${name}: switch process`} className="process-bar__name" trigger={<>{name}<span className="process-bar__caret" aria-hidden="true">▼</span></>}>
        {(close) =>
          state.records.map((r) => (
            <button
              key={r.id}
              type="button"
              role="menuitem"
              className={cx("process-menu__item", r.id === processId && "process-menu__item--on")}
              onClick={() => {
                close();
                dispatch({ type: "navigate", route: { view: "editor", processId: r.id } });
              }}
            >
              {working(r).name}
            </button>
          ))
        }
      </BarMenu>
    </>
  );
}

/**
 * The editing bar (board 03, Components use="bar" mode="edit"):
 * "‹ Processes / name / Editing v7", Lanes / Runbook, JSON, and Run.
 */
export function EditBar({
  processId,
  name,
  version,
  changes,
  layout,
  json,
  versionsOpen,
  onLayout,
  onJson,
  onRun,
}: {
  processId: string;
  name: string;
  version: number;
  /** How many changes the draft holds; 0 when saved. */
  changes: number;
  layout?: "lanes" | "runbook";
  json?: boolean;
  versionsOpen?: boolean;
  onLayout?(layout: "lanes" | "runbook"): void;
  onJson?(): void;
  onRun(): void;
}) {
  const { dispatch } = useProcess();
  return (
    <div className="process-bar">
      <div className="process-bar__group">
        <Crumb name={name} processId={processId} />
        <span className="process-bar__mode">Editing</span>
        <button
          type="button"
          className={cx("process-bar__version", versionsOpen && "process-bar__version--on")}
          title="Versions"
          aria-pressed={!!versionsOpen}
          onClick={() =>
            dispatch({ type: "navigate", route: versionsOpen ? { view: "editor", processId } : { view: "versions", processId } })
          }
        >
          <ProcessIcon name="history" size={13} stroke={1.8} />v{version}
        </button>
        {changes ? (
          <button
            type="button"
            className="process-bar__saved process-bar__saved--draft"
            title="Open versions to save the draft"
            onClick={() => dispatch({ type: "navigate", route: { view: "versions", processId } })}
          >
            <span className="process-dot process-dot--warn" />
            Draft · {changes} change{changes === 1 ? "" : "s"}
          </button>
        ) : (
          <span className="process-bar__saved">Saved</span>
        )}
      </div>
      <div className="process-bar__fill" />
      {layout && onLayout && onJson && (
        <div className="process-bar__group process-bar__group--tight">
          <div className="process-seg" role="group" aria-label="View">
            {(["lanes", "runbook"] as const).map((l) => (
              <button key={l} type="button" className={cx("process-seg__item", layout === l && "process-seg__item--on")} aria-pressed={layout === l} onClick={() => onLayout(l)}>
                {l === "lanes" ? "Lanes" : "Runbook"}
              </button>
            ))}
          </div>
          <button type="button" className={cx("process-bar__json", json && "process-bar__json--on")} aria-pressed={!!json} onClick={onJson}>
            <ProcessIcon name="braces" size={14} stroke={1.6} />
            JSON
          </button>
        </div>
      )}
      <button type="button" className="process-btn process-btn--primary process-bar__run" onClick={onRun} title={changes ? "Saves the draft as a new version, then runs it" : "Run this version"}>
        <span className="process-btn__play" aria-hidden="true">▶</span>
        Run
      </button>
    </div>
  );
}

/**
 * The run bar (Components use="bar" mode="run"): "‹ Processes / name / Run 4 ·
 * view only", the run's status, Edit process and Stop run.
 */
export function RunBar({ run, name }: { run: Run; name: string }) {
  const { state, dispatch, now } = useProcess();
  const siblings = state.runs.filter((r) => r.processId === run.processId);
  const live = run.status === "running" || run.status === "waiting";
  const sub = `${run.trigger} · ${clock(run.startedAt)}`;

  return (
    <div className="process-bar">
      <div className="process-bar__group">
        <Crumb name={name} processId={run.processId} />
        <BarMenu
          label={`Run ${run.number}: switch run`}
          className="process-bar__runchip"
          trigger={
            <>
              <span className={cx("process-dot", live ? "process-dot--accent" : run.status === "stopped" ? "process-dot--danger" : "process-dot--ok")} />
              Run {run.number}
              <span className="process-bar__caret" aria-hidden="true">▼</span>
            </>
          }
        >
          {(close) =>
            siblings.map((r) => (
              <button
                key={r.id}
                type="button"
                role="menuitem"
                className={cx("process-menu__item", r.id === run.id && "process-menu__item--on")}
                onClick={() => {
                  close();
                  dispatch({ type: "navigate", route: { view: "run", runId: r.id } });
                }}
              >
                Run {r.number}
                <span className="process-menu__meta">
                  {r.status === "done" ? r.result : r.status} · v{r.version}
                </span>
              </button>
            ))
          }
        </BarMenu>
        <span className="process-bar__sub">{sub}</span>
        <span className="process-bar__rule" />
        <span className="process-bar__pinned">Uses v{run.version} · view only</span>
      </div>
      <div className="process-bar__fill" />
      {run.status === "waiting" && (
        <div className="process-status process-status--waiting">
          <span className="process-dot process-dot--warn" />
          Waiting for you<span className="process-status__time">{agoShort(run.since, now)}</span>
        </div>
      )}
      {run.status === "running" && (
        <div className="process-status process-status--running">
          <span className="process-dot process-dot--accent" />
          Running<span className="process-status__time">{agoShort(run.startedAt, now)}</span>
        </div>
      )}
      {run.status === "done" && (
        <div className="process-status process-status--done">
          <span className="process-dot process-dot--ok" />
          {run.result}
        </div>
      )}
      {run.status === "stopped" && (
        <div className="process-status process-status--stopped">
          <span className="process-dot process-dot--danger" />
          {run.result}
        </div>
      )}
      <button type="button" className="process-btn process-btn--glass" onClick={() => dispatch({ type: "navigate", route: { view: "editor", processId: run.processId } })}>
        Edit process
      </button>
      {live ? (
        <button type="button" className="process-btn process-btn--glass process-btn--danger" onClick={() => dispatch({ type: "stop", runId: run.id, now: Date.now() })}>
          Stop run
        </button>
      ) : (
        <button type="button" className="process-btn process-btn--glass" onClick={() => dispatch({ type: "run", processId: run.processId, now: Date.now() })}>
          Run again
        </button>
      )}
    </div>
  );
}
