import { useEffect, useMemo } from "react";
import { layoutLanes, type Block, type BlockMark } from "../../process/layout";
import { startKindInfo, startTitle, surfaceDetail } from "../../process/model";
import { agoShort, clock, laneStatus, paneFor, waitActions, waitSummary } from "../../process/runs";
import type { Selection } from "../../process/state";
import type { Process, Run } from "../../process/types";
import { cx } from "../../lib/cx";
import { ProcessIcon } from "./ProcessIcon";
import { useProcess } from "./store";
import { useKeptScroll } from "./useKeptScroll";

/**
 * The lanes (Components.dc.html use="lanes", view="lanes"): rows are what
 * does the work on this computer, columns are the order. While editing, steps,
 * decisions, stages and the start point are buttons that select; in a run the
 * same drawing shows each step's state, the "now" line and the waiting card.
 * The geometry comes from src/process/layout.ts.
 */
export function ProcessLanes({
  process,
  run,
  selection,
  onSelect,
  onAddRow,
  colW,
  top = 68,
  scrollKey,
  marks,
}: {
  process: Process;
  run?: Run | null;
  selection?: Selection;
  onSelect?(selection: Selection): void;
  onAddRow?(kind: "terminal" | "browser"): void;
  colW: number;
  /** Room left for the bar floating over the top. */
  top?: number;
  scrollKey: string;
  marks?: Record<string, BlockMark>;
}) {
  const selected = selection?.kind === "step" ? selection.id : null;
  const layout = useMemo(() => layoutLanes(process, { colW, run, selected, marks }), [process, colW, run, selected, marks]);
  const scroll = useKeptScroll<HTMLDivElement>(scrollKey);
  const editing = !run && !!onSelect;
  const lanes = run ? laneStatus(run) : null;
  const start = startKindInfo(process.start.on);

  // Keep a newly selected step in view.
  useEffect(() => {
    if (!selected) return;
    scroll.ref.current?.querySelector<HTMLElement>(`[data-step="${CSS.escape(selected)}"]`)?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [selected, scroll.ref]);

  const select = (next: Selection) => onSelect?.(next);

  return (
    <div className={cx("process-lanes", run && "process-lanes--run")} ref={scroll.ref} onScroll={scroll.onScroll}>
      <div className="process-lanes__canvas" style={{ width: layout.width, height: layout.height + top, paddingTop: top }}>
        <div className="process-lanes__plane" style={{ width: layout.width, height: layout.height }}>
          {layout.lanes.map((lane) => (
            <div key={lane.id} className={cx("process-lanes__row", lane.alt && "process-lanes__row--alt")} style={{ top: lane.y, height: layout.laneH }} />
          ))}

          {layout.stages.map((stage) => {
            const on = selection?.kind === "stage" && selection.id === stage.id;
            const Head = editing ? "button" : "div";
            return (
              <div key={stage.id}>
                {stage.sep && <div className="process-lanes__sep" style={{ left: stage.x, height: stage.sepH }} />}
                <Head
                  {...(editing ? { type: "button" as const, onClick: () => select({ kind: "stage", id: stage.id }), "aria-pressed": on } : {})}
                  className={cx("process-stage", `process-stage--${stage.state}`, on && "process-stage--selected")}
                  style={{ left: stage.x, width: stage.w }}
                >
                  <span className="process-stage__top">
                    <span className="process-stage__n">{stage.n}</span>
                    <span className="process-stage__name">{stage.name}</span>
                    <span className="process-stage__note">{stage.note}</span>
                  </span>
                  <span className="process-stage__line" />
                </Head>
              </div>
            );
          })}

          <svg className="process-lanes__edges" width={layout.width} height={layout.height} aria-hidden="true">
            {layout.edges.map((e, i) => (
              <g key={i} className={cx("process-edge", `process-edge--${e.state}`, e.loopBack && "process-edge--back")}>
                <path d={e.d} />
                {e.arrow && <path d={e.arrow} className="process-edge__arrow" />}
              </g>
            ))}
          </svg>

          {layout.starts.map((s, i) => (
            <div key={i} className="process-lanes__startdot" style={{ left: s.x, top: s.y }} />
          ))}

          {layout.loops.map((loop) => (
            <div key={loop.id} className={cx("process-loop", loop.active && "process-loop--active")}>
              <div className="process-loop__band" style={{ left: loop.x, top: loop.y, width: loop.w }} />
              <div className="process-loop__label" style={{ left: loop.cx, top: loop.ly }}>
                <ProcessIcon name="loop" size={13} stroke={1.8} />
                <span>{loop.label}</span>
                {loop.count && <span className="process-loop__count">{loop.count}</span>}
              </div>
            </div>
          ))}

          {layout.blocks.map((b) => (
            <LaneBlock key={b.id} block={b} editing={editing} onSelect={() => select({ kind: "step", id: b.id })} />
          ))}

          {layout.labels.map((l, i) => (
            <div key={i} className={cx("process-branch", `process-branch--${l.branch}`)} style={{ left: l.x, top: l.y }}>
              {l.text}
            </div>
          ))}

          {layout.ends.map((e, i) => (
            <div key={i} className={cx("process-end", e.dim && "process-end--dim")}>
              <div className="process-end__dot" style={{ left: e.x, top: e.y }} />
              <div className="process-end__name" style={{ left: e.lx, top: e.ly }}>
                {e.name}
              </div>
            </div>
          ))}

          {layout.play && (
            <>
              <div className={cx("process-now", layout.play.waiting && "process-now--waiting")} style={{ left: layout.play.x, top: 56, height: layout.play.h }} />
              <div className={cx("process-now__tag", layout.play.waiting && "process-now--waiting")} style={{ left: layout.play.x, top: 50 }}>
                now
              </div>
            </>
          )}
          {run && layout.wait && <WaitingCard run={run} stepId={layout.wait.stepId} x={layout.wait.x} y={layout.wait.y} />}

          {/* Row heads stay put while the lanes scroll sideways. */}
          <div className="process-lanes__heads" style={{ width: layout.headW, height: layout.height }}>
            {editing ? (
              <button
                type="button"
                className={cx("process-ruler", selection?.kind === "start" && "process-ruler--selected")}
                style={{ height: layout.rulerH }}
                aria-pressed={selection?.kind === "start"}
                onClick={() => select({ kind: "start" })}
              >
                <RulerContent icon={start.icon} title={startTitle(process.start)} />
              </button>
            ) : (
              <div className="process-ruler" style={{ height: layout.rulerH }}>
                <RulerContent icon={start.icon} title={run ? `${run.detail ?? run.trigger} · ${clock(run.startedAt)}` : startTitle(process.start)} />
              </div>
            )}
            {layout.lanes.map((lane) => {
              const live = lanes?.[lane.id];
              return (
                <div key={lane.id} className={cx("process-head", `process-tone--${lane.meta.tone}`)} style={{ top: lane.y, height: layout.laneH }}>
                  <div className={cx("process-head__icon", lane.meta.round && "process-head__icon--round")}>
                    <ProcessIcon name={lane.meta.icon} size={20} />
                  </div>
                  <div className="process-head__body">
                    <div className="process-head__top">
                      <span className="process-head__name">{lane.meta.name}</span>
                      {live?.pane && <OpenPane pane={live.pane} />}
                    </div>
                    <div className={cx("process-head__sub", live && `process-head__sub--${live.tone}`)}>
                      {live && <span className={cx("process-dot", "process-dot--small", `process-dot--${live.tone}`)} />}
                      <span className="process-head__subtext">{live ? live.text : surfaceDetail(lane.surface)}</span>
                    </div>
                  </div>
                </div>
              );
            })}
            {editing && onAddRow && (
              <div className="process-lanes__addrow" style={{ top: layout.height - 44 }}>
                <button type="button" className="process-addrow" onClick={() => onAddRow("terminal")}>
                  <span aria-hidden="true">+</span>Terminal
                </button>
                <button type="button" className="process-addrow" onClick={() => onAddRow("browser")}>
                  <span aria-hidden="true">+</span>Browser
                </button>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function RulerContent({ icon, title }: { icon: string; title: string }) {
  return (
    <>
      <span className="process-ruler__icon">
        <ProcessIcon name={icon} size={17} />
      </span>
      <span className="process-ruler__body">
        <span className="process-ruler__kicker">Starts from</span>
        <span className="process-ruler__title">{title}</span>
      </span>
    </>
  );
}

function OpenPane({ pane }: { pane: number }) {
  const { onOpenPane } = useProcess();
  return (
    <button type="button" className="process-head__open" title={`Open pane ${pane} in Terminals`} onClick={() => onOpenPane(pane)}>
      Open
    </button>
  );
}

function LaneBlock({ block: b, editing, onSelect }: { block: Block; editing: boolean; onSelect(): void }) {
  const classes = cx(`process-tone--${b.tone}`, `process-block--${b.state}`, b.mark && `process-block--${b.mark}`);
  const label = `${b.name}${b.status ? `, ${b.status}` : ""}`;
  if (b.kind === "decision") {
    const Diamond = editing ? "button" : "div";
    return (
      <>
        <div className={cx("process-decision__name", `process-block--${b.state}`)} style={{ left: b.lx, top: b.ly }}>
          {b.name}
        </div>
        <Diamond
          {...(editing ? { type: "button" as const, onClick: onSelect, "aria-pressed": b.state === "selected", "aria-label": label } : {})}
          data-step={b.id}
          className={cx("process-decision", classes)}
          style={{ left: b.x, top: b.y }}
          title={b.line}
        >
          <span className="process-decision__shape" />
          <span className="process-decision__glyph">{b.glyph}</span>
        </Diamond>
      </>
    );
  }
  const Box = editing ? "button" : "div";
  return (
    <Box
      {...(editing ? { type: "button" as const, onClick: onSelect, "aria-pressed": b.state === "selected" } : {})}
      data-step={b.id}
      className={cx("process-block", classes)}
      style={{ left: b.x, top: b.y, width: b.w }}
      title={b.line}
    >
      <span className="process-block__top">
        <span className="process-block__name">{b.name}</span>
        {b.status && <span className="process-block__status">{b.status}</span>}
      </span>
      <span className="process-block__line">{b.line}</span>
    </Box>
  );
}

/** "Waiting for you" (part of use="lanes"): under the step that waits. */
function WaitingCard({ run, stepId, x, y }: { run: Run; stepId: string; x: number; y: number }) {
  const { dispatch, now, onOpenPane } = useProcess();
  const actions = waitActions(run, stepId);
  const pane = paneFor(run, stepId);
  return (
    <div className="process-wait" style={{ left: x, top: y }} role="region" aria-label="Waiting for you">
      <div className="process-wait__top">
        <span className="process-dot process-dot--warn process-wait__dot" />
        <span className="process-wait__title">Waiting for you</span>
        <span className="process-wait__time">{agoShort(run.since, now)}</span>
      </div>
      <p className="process-wait__text">{waitSummary(run, stepId)}</p>
      <div className="process-wait__buttons">
        <button
          type="button"
          className="process-btn process-btn--small process-btn--warn"
          title={pane ? `Open pane ${pane} in Terminals` : undefined}
          onClick={() => pane && onOpenPane(pane)}
        >
          {actions.primary}
        </button>
        <button type="button" className="process-btn process-btn--small process-btn--soft" onClick={() => dispatch({ type: "approve", runId: run.id, stepId, now: Date.now() })}>
          {actions.approve}
        </button>
        <button type="button" className="process-btn process-btn--small process-btn--soft" onClick={() => dispatch({ type: actions.backAction, runId: run.id, stepId, now: Date.now() })}>
          {actions.back}
        </button>
      </div>
    </div>
  );
}
