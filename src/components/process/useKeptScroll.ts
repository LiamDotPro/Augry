import { createContext, useCallback, useContext, useLayoutEffect, useRef } from "react";

/**
 * Scroll positions that outlive the element: the Process tab swaps whole
 * views (home, a run, the editor), so each scroller saves its place under a
 * key and gets it back when it mounts again.
 */
export const ScrollMemory = createContext<Map<string, { left: number; top: number }>>(new Map());

export function useKeptScroll<T extends HTMLElement>(key: string) {
  const memory = useContext(ScrollMemory);
  const ref = useRef<T>(null);

  useLayoutEffect(() => {
    const el = ref.current;
    const saved = memory.get(key);
    if (el && saved) {
      el.scrollLeft = saved.left;
      el.scrollTop = saved.top;
    }
  }, [key, memory]);

  const onScroll = useCallback(() => {
    const el = ref.current;
    if (el) memory.set(key, { left: el.scrollLeft, top: el.scrollTop });
  }, [key, memory]);

  return { ref, onScroll };
}
