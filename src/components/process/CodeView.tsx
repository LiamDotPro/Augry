import { memo } from "react";
import { tokenize } from "../../process/json";
import type { DiffLine } from "../../process/versions";
import { cx } from "../../lib/cx";

/** One line of JSON, coloured by token. */
export const CodeTokens = memo(function CodeTokens({ line }: { line: string }) {
  return (
    <>
      {tokenize(line).map((t, i) => (
        <span key={i} className={`process-tok process-tok--${t.kind}`}>
          {t.text}
        </span>
      ))}
    </>
  );
});

/**
 * A compare of two versions' JSON (Components use="lanes" view="diff"): both
 * line numbers, a sign, and folded runs of unchanged lines.
 */
export function DiffView({ lines }: { lines: DiffLine[] }) {
  return (
    <div className="process-code__lines process-code__lines--diff">
      {lines.map((l, i) =>
        l.kind === "fold" ? (
          <div key={i} className="process-code__row process-code__row--fold">
            <span className="process-code__mark" />
            <span className="process-code__n" />
            <span className="process-code__n" />
            <span className="process-code__sign" />
            <span className="process-code__text">⋯ {l.count} unchanged lines</span>
          </div>
        ) : (
          <div key={i} className={cx("process-code__row", `process-code__row--${l.kind}`)}>
            <span className="process-code__mark" />
            <span className="process-code__n">{l.old ?? ""}</span>
            <span className="process-code__n">{l.new ?? ""}</span>
            <span className="process-code__sign">{l.kind === "add" ? "+" : l.kind === "del" ? "-" : ""}</span>
            <span className="process-code__text">
              <CodeTokens line={l.text} />
            </span>
          </div>
        ),
      )}
    </div>
  );
}
