import { useEffect, useRef } from "react";

const OPENED = "augry:popover-opened";

/**
 * Keeps popovers one at a time: when this one opens it tells the others,
 * which close. Without it a pane's task popover stayed open under its own +
 * menu (both live in the header, so neither click was "outside"), and the
 * hotkey sheet could land over both.
 */
export function useSolePopover(open: boolean, close: () => void): void {
  const token = useRef<object>({});
  const closeRef = useRef(close);
  closeRef.current = close;

  useEffect(() => {
    if (!open) return;
    // Announce before listening, so this popover does not hear itself.
    window.dispatchEvent(new CustomEvent(OPENED, { detail: token.current }));
    const onOpened = (event: Event) => {
      if ((event as CustomEvent).detail !== token.current) closeRef.current();
    };
    window.addEventListener(OPENED, onOpened);
    return () => window.removeEventListener(OPENED, onOpened);
  }, [open]);
}
