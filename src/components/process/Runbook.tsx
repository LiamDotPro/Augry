import { useEffect, useMemo } from "react";
import { runbookRows } from "../../process/runbook";
import type { Selection } from "../../process/state";
import type { Process } from "../../process/types";
import { cx } from "../../lib/cx";
import { ProcessIcon } from "./ProcessIcon";
import { useKeptScroll } from "./useKeptScroll";

/**
 * The runbook (board 07; Components use="lanes" view="list"): the same
 * process top to bottom. Loops and steps that run together are indented.
 */
export function Runbook({
  process,
  selection,
  onSelect,
  top = 76,
  scrollKey,
}: {
  process: Process;
  selection: Selection;
  onSelect(selection: Selection): void;
  top?: number;
  scrollKey: string;
}) {
  const rows = useMemo(() => runbookRows(process), [process]);
  const scroll = useKeptScroll<HTMLDivElement>(scrollKey);
  const selected = selection?.kind === "step" ? selection.id : null;

  useEffect(() => {
    if (!selected) return;
    scroll.ref.current?.querySelector<HTMLElement>(`[data-step="${CSS.escape(selected)}"]`)?.scrollIntoView({ block: "nearest" });
  }, [selected, scroll.ref]);

  return (
    <div className="process-runbook" ref={scroll.ref} onScroll={scroll.onScroll} style={{ paddingTop: top }}>
      <div className="process-runbook__list">
        {rows.map((r) => {
          if (r.kind === "stage") {
            const on = selection?.kind === "stage" && selection.id === r.key.slice("stage-".length);
            return (
              <button
                key={r.key}
                type="button"
                className={cx("process-runbook__stage", on && "process-runbook__stage--on")}
                onClick={() => onSelect({ kind: "stage", id: r.key.slice("stage-".length) })}
              >
                <span className="process-stage__n">{r.n}</span>
                <span className="process-runbook__stagename">{r.name}</span>
                <span className="process-runbook__note">{r.note}</span>
              </button>
            );
          }
          if (r.kind === "mark") {
            const content = (
              <>
                <ProcessIcon name={r.icon} size={15} stroke={1.8} />
                <span className="process-runbook__markname">{r.name}</span>
                <span className="process-runbook__markline">{r.line}</span>
              </>
            );
            return r.role === "start" ? (
              <button
                key={r.key}
                type="button"
                className={cx("process-runbook__mark", "process-runbook__mark--start", selection?.kind === "start" && "process-runbook__mark--on")}
                style={{ paddingLeft: r.indent }}
                onClick={() => onSelect({ kind: "start" })}
              >
                {content}
              </button>
            ) : (
              <div key={r.key} className={cx("process-runbook__mark", `process-runbook__mark--${r.role}`)} style={{ paddingLeft: r.indent }}>
                {content}
              </div>
            );
          }
          const on = r.id === selected;
          return (
            <div key={r.key} className="process-runbook__steprow" style={{ paddingLeft: r.indent }}>
              <button
                type="button"
                data-step={r.id}
                aria-pressed={on}
                className={cx("process-runbook__step", `process-tone--${r.tone}`, r.decision && "process-runbook__step--decision", on && "process-runbook__step--on")}
                onClick={() => onSelect({ kind: "step", id: r.id })}
              >
                <span className={cx("process-runbook__icon", r.round && "process-runbook__icon--round")}>
                  <ProcessIcon name={r.icon} size={17} />
                </span>
                <span className="process-runbook__name">{r.name}</span>
                <span className="process-runbook__line">{r.line}</span>
                <span className="process-runbook__pill">{r.surface}</span>
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
