import { useEffect, useRef, type ReactNode } from "react";
import { cx } from "../../lib/cx";
import { initialOf } from "../Account";

/**
 * The frame of every team dialog (App Shell board 08): the scrim, the dialog
 * box and its keyboard. A click on the scrim or Escape closes, Enter runs the
 * primary action (`onSubmit`), and the first field (or else the primary
 * button) takes focus on open. Keys are caught on the window in the capture
 * phase so the terminal under the dialog never sees them.
 */
export function TeamDialog({
  label,
  width,
  className,
  onClose,
  onSubmit,
  children,
}: {
  /** The dialog's accessible name. */
  label: string;
  width: number;
  className?: string;
  onClose(): void;
  /** Left out when Enter belongs to the fields (team settings). */
  onSubmit?: () => void;
  children: ReactNode;
}) {
  const box = useRef<HTMLDivElement>(null);
  const pressedOnScrim = useRef(false);
  const handlers = useRef({ onClose, onSubmit });
  handlers.current = { onClose, onSubmit };

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        handlers.current.onClose();
        return;
      }
      if (event.key !== "Enter" || event.isComposing || event.shiftKey || event.altKey || event.metaKey || event.ctrlKey) {
        return;
      }
      // A focused button or menu handles Enter itself.
      const target = event.target instanceof HTMLElement ? event.target : null;
      if (target?.closest("button, select, textarea")) return;
      const submit = handlers.current.onSubmit;
      if (!submit) return;
      event.preventDefault();
      event.stopPropagation();
      submit();
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, []);

  useEffect(() => {
    const root = box.current;
    const first =
      root?.querySelector<HTMLElement>("input:not(:disabled), select:not(:disabled)") ??
      root?.querySelector<HTMLElement>(".cloud-btn--primary:not(:disabled)") ??
      root;
    first?.focus();
  }, []);

  return (
    <div
      className="cloud-scrim"
      onMouseDown={(event) => {
        pressedOnScrim.current = event.target === event.currentTarget;
      }}
      onClick={(event) => {
        // Only a click that started and ended on the scrim, not a drag out of a field.
        if (pressedOnScrim.current && event.target === event.currentTarget) handlers.current.onClose();
        pressedOnScrim.current = false;
      }}
    >
      <div
        ref={box}
        role="dialog"
        aria-modal="true"
        aria-label={label}
        tabIndex={-1}
        className={cx("cloud-dialog", "team-dialog", className)}
        style={{ ["--cloud-dialog-width" as string]: `${width}px` }}
      >
        {children}
      </div>
    </div>
  );
}

/** The usual head: a 32px tile, the title and ×. */
export function TeamDialogHead({ tile, title, onClose }: { tile: ReactNode; title: ReactNode; onClose(): void }) {
  return (
    <div className="cloud-dialog__head">
      {tile}
      <h2 className="cloud-dialog__title">{title}</h2>
      <button type="button" className="cloud-dialog__close" aria-label="Close" onClick={onClose}>
        ×
      </button>
    </div>
  );
}

/** The person outline in a cyan tile (team-join, team-join-signed-out). */
export function PersonTile() {
  return (
    <span className="cloud-dialog__tile team-dialog__tile--cyan" aria-hidden="true">
      <svg width="17" height="17" viewBox="0 0 24 24" className="account-icon">
        <path d="M15 8.5a3 3 0 1 1-6 0 3 3 0 0 1 6 0zM5.5 19a6.5 6.5 0 0 1 13 0" className="team-dialog__stroke" />
      </svg>
    </span>
  );
}

/** A plus in a cyan tile (team-create). */
export function PlusTile() {
  return (
    <span className="cloud-dialog__tile team-dialog__tile--cyan" aria-hidden="true">
      <svg width="17" height="17" viewBox="0 0 24 24" className="account-icon">
        <path d="M12 5v14M5 12h14" className="team-dialog__stroke" />
      </svg>
    </span>
  );
}

/** The team's initial in a 32px violet tile, for heads. */
export function TeamHeadTile({ name }: { name: string }) {
  return (
    <span className="cloud-dialog__tile team-dialog__tile--team" aria-hidden="true">
      {initialOf(name)}
    </span>
  );
}

/** A red dot and the error, under a field. */
export function TeamFieldError({ id, children }: { id?: string; children: ReactNode }) {
  return (
    <div id={id} className="team-dialog__error" role="alert">
      <span className="team-dialog__error-dot" aria-hidden="true" />
      {children}
    </div>
  );
}
