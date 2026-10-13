import { useEffect, useMemo, useRef, useState } from "react";
import { formatProcess, parseProcess, stepAtLine, stepLineRange } from "../../process/json";
import { errorMessage } from "../../lib/appError";
import type { Selection } from "../../process/state";
import type { Process } from "../../process/types";
import { cx } from "../../lib/cx";
import { CodeTokens } from "./CodeView";
import { ProcessIcon } from "./ProcessIcon";

const LINE_H = 21;

/**
 * The JSON panel (board 08): the process file, edited directly. Valid edits
 * update the lanes as you type; invalid ones keep the last good process and
 * show the error on its line. Selecting a step anywhere highlights its lines
 * here, and moving the caret into a step selects it.
 */
export function JsonPanel({
  process,
  file,
  version,
  selection,
  onSelect,
  onChange,
  onOpenFile,
}: {
  process: Process;
  /** Where the process lives on disk, or null for a team process (it lives in the cloud). */
  file: string | null;
  version: number;
  selection: Selection;
  onSelect(selection: Selection): void;
  onChange(process: Process): void;
  /** Opens the saved file in the system's editor; absent for a team process. */
  onOpenFile?: () => Promise<void>;
}) {
  const [text, setText] = useState(() => formatProcess(process));
  const [error, setError] = useState<{ message: string; line: number | null } | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLTextAreaElement>(null);
  const lines = useMemo(() => text.split("\n"), [text]);
  const selected = selection?.kind === "step" ? selection.id : null;
  const range = selected ? stepLineRange(lines, selected) : null;
  const longest = useMemo(() => Math.max(40, ...lines.map((l) => l.length)), [lines]);

  // Edits from the lanes or the side panel rewrite the text, unless the text
  // already says the same thing (then the user's own layout is kept).
  useEffect(() => {
    const parsed = parseProcess(text);
    if (parsed.ok && formatProcess(parsed.process) === formatProcess(process)) return;
    if (!parsed.ok && error) return;
    setText(formatProcess(process));
    setError(null);
    // Only the process drives this; the text is read as it was.
  }, [process]);

  // Bring a newly selected step's lines into view.
  useEffect(() => {
    const el = scroller.current;
    if (!el || !range) return;
    const top = range[0] * LINE_H;
    const bottom = (range[1] + 1) * LINE_H;
    if (top < el.scrollTop || bottom > el.scrollTop + el.clientHeight) el.scrollTop = Math.max(0, top - 6 * LINE_H);
  }, [selected]);

  const edit = (next: string) => {
    setText(next);
    setNote(null);
    const parsed = parseProcess(next);
    if (parsed.ok) {
      setError(null);
      onChange(parsed.process);
    } else {
      setError({ message: parsed.error, line: parsed.line });
    }
  };

  const caretMoved = () => {
    const el = input.current;
    if (!el) return;
    const line = el.value.slice(0, el.selectionStart).split("\n").length - 1;
    const id = stepAtLine(lines, line, process.steps.map((s) => s.id));
    if (id && id !== selected) onSelect({ kind: "step", id });
  };

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setNote("Copied");
    } catch {
      setNote("Could not copy");
    }
  };

  return (
    <div className="process-code process-json">
      <div className="process-code__head">
        <ProcessIcon name="braces" size={16} stroke={1.6} className="process-code__headicon" />
        <span className="process-code__file">{file ?? `${process.name}.json`}</span>
        <span className="process-code__ver">v{version}</span>
        <span className="process-code__fill" />
        {error ? (
          <span className="process-code__status process-code__status--bad" role="alert">
            <span className="process-dot process-dot--danger" />
            {error.line ? `Line ${error.line}: ` : ""}
            {error.message}
          </span>
        ) : (
          <span className="process-code__status">
            <span className="process-dot process-dot--ok" />
            Valid · process@1
          </span>
        )}
      </div>
      <div className="process-code__scroll" ref={scroller}>
        <div className="process-code__inner" style={{ height: lines.length * LINE_H + 20, width: `calc(${longest + 4}ch + 66px)` }}>
          <div className="process-code__lines" aria-hidden="true">
            {lines.map((line, i) => {
              const hot = range && i >= range[0] && i <= range[1];
              const bad = error?.line === i + 1;
              return (
                <div key={i} className={cx("process-code__row", hot && "process-code__row--hot", bad && "process-code__row--bad")}>
                  <span className="process-code__mark" />
                  <span className="process-code__n">{i + 1}</span>
                  <span className="process-code__text">
                    <CodeTokens line={line} />
                  </span>
                </div>
              );
            })}
          </div>
          <textarea
            ref={input}
            className="process-code__input"
            value={text}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            wrap="off"
            aria-label={`${file ?? process.name}, process JSON`}
            onChange={(e) => edit(e.target.value)}
            onClick={caretMoved}
            onKeyUp={(e) => (e.key.startsWith("Arrow") || e.key === "PageUp" || e.key === "PageDown") && caretMoved()}
            onKeyDown={(e) => {
              // Tab indents instead of leaving the editor; Escape lets it go.
              if (e.key === "Tab" && !e.shiftKey) {
                e.preventDefault();
                const el = e.currentTarget;
                const at = el.selectionStart;
                const next = el.value.slice(0, at) + "  " + el.value.slice(el.selectionEnd);
                edit(next);
                requestAnimationFrame(() => el.setSelectionRange(at + 2, at + 2));
              } else if (e.key === "Escape") {
                e.currentTarget.blur();
              }
            }}
          />
        </div>
      </div>
      <div className="process-code__foot">
        <button type="button" className="process-btn process-btn--outline" onClick={() => edit(formatProcess(parseProcess(text).ok ? JSON.parse(text) : process))}>
          Format
        </button>
        <button type="button" className="process-btn process-btn--outline" onClick={copy}>
          Copy
        </button>
        {onOpenFile && (
          <button
            type="button"
            className="process-btn process-btn--outline"
            title="Opens the saved version in the app your computer uses for JSON"
            onClick={() =>
              void onOpenFile()
                .then(() => setNote("Opened the saved version. Changes made there show here as a draft the next time Augry starts."))
                .catch((error) => setNote(`Couldn’t open it: ${errorMessage(error)}`))
            }
          >
            Open in editor
          </button>
        )}
        <span className="process-code__fill" />
        <span className="process-code__hint" role="status">
          {note ?? "Edits update the lanes as you type"}
        </span>
      </div>
    </div>
  );
}
