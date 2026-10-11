import { useMemo } from "react";
import { miniBlocks } from "../../process/layout";
import { surfaceMeta } from "../../process/model";
import type { Process } from "../../process/types";
import { cx } from "../../lib/cx";

/**
 * The small lanes preview on process cards and templates: a dot per row, a
 * block per step, decisions as small diamonds. `big` is the process card,
 * otherwise the template thumbnail (the design's MINI sizes).
 */
export function MiniLanes({ process, big }: { process: Process; big?: boolean }) {
  const [u, rh, x0, ds] = big ? [46, 29, 30, 9] : [12, 16, 16, 5];
  const blocks = useMemo(() => miniBlocks(process), [process]);
  const rows = process.surfaces.slice(0, 5);
  return (
    <div className={cx("process-mini", big && "process-mini--big")} aria-hidden="true">
      {rows.map((surface, r) => {
        const y = 2 + r * rh + rh / 2;
        return (
          <div key={surface.id} className={`process-tone--${surfaceMeta(surface).tone}`}>
            <div className="process-mini__rule" style={{ top: y }} />
            <div
              className={cx("process-mini__dot", r === 0 && "process-mini__dot--round")}
              style={{ left: x0 / 2 - ds / 2, top: y - ds / 2, width: ds, height: ds }}
            />
          </div>
        );
      })}
      {blocks
        .filter((b) => b.row < 5)
        .map((b, i) => {
          const cy = 2 + b.row * rh + rh / 2;
          const style = b.decision
            ? { left: x0 + b.col * u, top: cy - rh * 0.28, width: rh * 0.56, height: rh * 0.56, transform: `translateX(${u * 0.2}px) rotate(45deg)` }
            : { left: x0 + b.col * u, top: cy - rh * 0.34, width: u - Math.max(4, u * 0.18), height: rh * 0.68 };
          return (
            <div
              key={i}
              className={cx("process-mini__block", `process-tone--${b.tone}`, b.decision && "process-mini__block--decision")}
              style={style}
            />
          );
        })}
    </div>
  );
}
