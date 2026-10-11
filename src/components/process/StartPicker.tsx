import { useRef } from "react";
import { START_KINDS } from "../../process/model";
import type { StartKind } from "../../process/types";
import { cx } from "../../lib/cx";
import { ProcessIcon } from "./ProcessIcon";
import { useDismiss } from "./useDismiss";

/**
 * "Starts from" (board 04, "start"): what starts a run, chosen from the
 * top-left corner of the editor.
 */
export function StartPicker({ current, onPick, onClose }: { current: StartKind; onPick(kind: StartKind): void; onClose(): void }) {
  const root = useRef<HTMLDivElement>(null);
  useDismiss(true, root, onClose);
  return (
    <div className="process-picker" ref={root} role="listbox" aria-label="Starts from">
      <div className="process-picker__title">Starts from</div>
      {START_KINDS.map((k) => (
        <button
          key={k.kind}
          type="button"
          role="option"
          aria-selected={k.kind === current}
          className={cx("process-picker__item", k.kind === current && "process-picker__item--on")}
          onClick={() => onPick(k.kind)}
        >
          <span className="process-picker__icon">
            <ProcessIcon name={k.icon} size={17} />
          </span>
          <span className="process-picker__body">
            <span className="process-picker__name">{k.name}</span>
            <span className="process-picker__detail">{k.detail}</span>
          </span>
        </button>
      ))}
    </div>
  );
}
