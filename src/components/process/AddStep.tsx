import { useCallback, useRef } from "react";
import { STEP_KINDS, type StepKind } from "../../process/edit";
import { ProcessIcon } from "./ProcessIcon";
import { useDismiss } from "./useDismiss";

/**
 * Add step (bottom right of the editor, Tab): a palette of step kinds. The
 * new step goes after the selected one, or at the end.
 */
export function AddStep({ open, onOpen, onClose, onAdd, after }: { open: boolean; onOpen(): void; onClose(): void; onAdd(kind: StepKind): void; after: string | null }) {
  const root = useRef<HTMLDivElement>(null);
  const close = useCallback(() => onClose(), [onClose]);
  useDismiss(open, root, close);
  return (
    <div className="process-add" ref={root}>
      {open && (
        <div className="process-add__palette" role="menu" aria-label="Add a step">
          <div className="process-add__title">{after ? `Add after ${after}` : "Add at the end"}</div>
          {STEP_KINDS.map((k, i) => (
            <button
              key={k.kind}
              type="button"
              role="menuitem"
              className={`process-add__item process-tone--${k.surface === "you" ? "you" : k.kind === "decision" ? "shell" : k.surface}`}
              autoFocus={i === 0}
              onClick={() => onAdd(k.kind)}
            >
              <span className="process-add__icon">
                <ProcessIcon name={k.icon} size={16} />
              </span>
              <span className="process-add__body">
                <span className="process-add__name">{k.name}</span>
                <span className="process-add__detail">{k.detail}</span>
              </span>
            </button>
          ))}
        </div>
      )}
      <button type="button" className="process-add__button" aria-expanded={open} onClick={open ? onClose : onOpen}>
        <span className="process-btn__plus" aria-hidden="true">+</span>
        Add step
        <kbd className="process-kbd process-kbd--dim">Tab</kbd>
      </button>
    </div>
  );
}
