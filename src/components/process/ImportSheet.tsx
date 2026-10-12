import { useState } from "react";
import { parseProcess } from "../../process/json";
import { Sheet } from "./Sheet";
import { useProcess } from "./store";

/**
 * Import JSON: paste a process file. It is checked against augry/process@1
 * and, if it is usable, starts a new process at version 1. With a team
 * selected it becomes the team's.
 */
export function ImportSheet() {
  const { dispatch, team } = useProcess();
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const close = () => dispatch({ type: "import-open", open: false });
  const submit = () => {
    const result = parseProcess(text);
    if (!result.ok) {
      setError(result.line ? `Line ${result.line}: ${result.error}` : result.error);
      return;
    }
    if (team) {
      close();
      void team.create({ ...result.process, from: "import" }, "editor");
    } else dispatch({ type: "import", process: result.process, now: Date.now() });
  };

  return (
    <Sheet label="Import JSON" onClose={close} onSubmit={submit}>
      <div className="process-sheet__head">
        <div className="process-sheet__titles">
          <h2 className="process-sheet__title">Import JSON</h2>
          <p className="process-sheet__sub">Paste a process file. It starts a new process at version 1 and records where it came from.</p>
        </div>
        <button type="button" className="process-x" aria-label="Close" onClick={close}>
          ×
        </button>
      </div>
      <div className="process-sheet__body">
        <textarea
          className="process-input process-import"
          spellCheck={false}
          aria-label="Process JSON"
          placeholder='{ "$schema": "augry/process@1", … }'
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            setError(null);
          }}
        />
        {error && (
          <div className="process-error" role="alert">
            {error}
          </div>
        )}
      </div>
      <div className="process-sheet__foot">
        <div className="process-sheet__fill" />
        <button type="button" className="process-btn process-btn--quiet" onClick={close}>
          Cancel
        </button>
        <button type="button" className="process-btn process-btn--primary" onClick={submit} disabled={!text.trim()}>
          Import
          <kbd className="process-kbd">Ctrl+Enter</kbd>
        </button>
      </div>
    </Sheet>
  );
}
