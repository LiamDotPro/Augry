import { useEffect, useRef, type ReactNode } from "react";

/**
 * A modal sheet over the Process tab (the Copy and run window). Escape and
 * the backdrop close it, Ctrl+Enter submits, and focus starts inside it and
 * goes back where it was when it closes.
 */
export function Sheet({
  label,
  onClose,
  onSubmit,
  children,
  wide,
}: {
  label: string;
  onClose(): void;
  onSubmit?(): void;
  children: ReactNode;
  wide?: boolean;
}) {
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    // The first field, else the first button (the close button comes first in the markup).
    const el = root.current;
    const first =
      el?.querySelector<HTMLElement>("[data-autofocus]") ?? el?.querySelector<HTMLElement>("input, textarea") ?? el?.querySelector<HTMLElement>("button");
    first?.focus();
    return () => before?.focus?.();
  }, []);

  return (
    <div className="process-sheet__backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div
        ref={root}
        className={wide ? "process-sheet process-sheet--wide" : "process-sheet"}
        role="dialog"
        aria-modal="true"
        aria-label={label}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            onClose();
          } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && onSubmit) {
            e.preventDefault();
            onSubmit();
          } else if (e.key === "Tab") {
            // Keep Tab inside the sheet.
            const items = [...(root.current?.querySelectorAll<HTMLElement>("button, input, textarea, select") ?? [])].filter((x) => !x.hasAttribute("disabled"));
            if (!items.length) return;
            const i = items.indexOf(document.activeElement as HTMLElement);
            if (e.shiftKey && i <= 0) {
              e.preventDefault();
              items[items.length - 1]!.focus();
            } else if (!e.shiftKey && i === items.length - 1) {
              e.preventDefault();
              items[0]!.focus();
            }
          }
        }}
      >
        {children}
      </div>
    </div>
  );
}
