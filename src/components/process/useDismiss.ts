import { useEffect, type RefObject } from "react";

/** Closes a popover on a click outside `root` or on Escape. */
export function useDismiss(open: boolean, root: RefObject<HTMLElement | null>, close: () => void) {
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) close();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    document.addEventListener("mousedown", onDown);
    root.current?.addEventListener("keydown", onKey);
    const el = root.current;
    return () => {
      document.removeEventListener("mousedown", onDown);
      el?.removeEventListener("keydown", onKey);
    };
  }, [open, root, close]);
}
