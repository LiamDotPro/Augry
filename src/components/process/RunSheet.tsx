import { useState } from "react";
import { TEMPLATES } from "../../process/examples";
import { ISSUES } from "../../process/mock";
import { processFile, uniqueId } from "../../process/model";
import { stopsForYou, surfaceSummary } from "../../process/runs";
import { cx } from "../../lib/cx";
import { ProcessIcon } from "./ProcessIcon";
import { Sheet } from "./Sheet";
import { useProcess } from "./store";

const NUMBERS = ["", "once", "twice", "three times", "four times"];

/**
 * Copy and run (board 01, "run-sheet"): pick what the run is for, check what
 * will happen, then run. Copying saves the template to the repo as version 1.
 */
export function RunSheet({ templateId }: { templateId: string }) {
  const { state, dispatch } = useProcess();
  const template = TEMPLATES.find((t) => t.id === templateId);
  const [issue, setIssue] = useState(ISSUES[0]!.n);
  const [search, setSearch] = useState("");
  const [purpose, setPurpose] = useState("");
  const [worktree, setWorktree] = useState(true);
  if (!template) return null;

  const id = uniqueId(template.process.id, state.records.map((r) => r.id));
  const ticket = template.start === "ticket";
  const manual = template.start === "manual";
  const chips = surfaceSummary(template.process);
  const stops = stopsForYou(template.process);
  const issues = ISSUES.filter((i) => !search || `${i.n} ${i.title}`.toLowerCase().includes(search.toLowerCase()));
  const branch = ticket ? `feat/issue-${issue.slice(1)}` : `run/${id}`;
  const close = () => dispatch({ type: "sheet", templateId: null });
  const go = (run: boolean) =>
    dispatch({
      type: "copy-template",
      templateId,
      run,
      trigger: ticket ? `issue ${issue}` : manual ? "manual" : undefined,
      detail: ticket ? issue : manual && purpose.trim() ? purpose.trim() : undefined,
      now: Date.now(),
    });

  return (
    <Sheet label={`Copy and run ${template.name}`} onClose={close} onSubmit={() => go(true)}>
      <div className="process-sheet__head">
        <div className="process-sheet__titles">
          <span className="process-sheet__kicker">Template</span>
          <h2 className="process-sheet__title">Copy and run {template.name}</h2>
          <p className="process-sheet__sub">
            Saved to <code className="process-sheet__path">{processFile(id)}</code> as version 1. You can edit it later.
          </p>
        </div>
        <button type="button" className="process-x" aria-label="Close" onClick={close}>
          ×
        </button>
      </div>

      <div className="process-sheet__body">
        {ticket && (
          <div className="process-sheet__group">
            <div className="process-label" id="sheet-issue">
              Which issue
            </div>
            <input
              className="process-input"
              type="search"
              placeholder="Search open issues labelled ready"
              aria-label="Search open issues labelled ready"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
            <div className="process-issues" role="radiogroup" aria-labelledby="sheet-issue">
              {issues.map((i) => (
                <button
                  key={i.n}
                  type="button"
                  role="radio"
                  aria-checked={issue === i.n}
                  className={cx("process-issue", issue === i.n && "process-issue--on")}
                  onClick={() => setIssue(i.n)}
                >
                  <span className="process-radio" />
                  <span className="process-issue__n">{i.n}</span>
                  <span className="process-issue__title">{i.title}</span>
                  <span className="process-issue__tag">ready</span>
                </button>
              ))}
              {!issues.length && <div className="process-issue process-issue--none">No open issue matches.</div>}
            </div>
          </div>
        )}
        {manual && (
          <label className="process-sheet__group">
            <span className="process-label">What is it for</span>
            <input className="process-input" placeholder="Say what this run should do" value={purpose} onChange={(e) => setPurpose(e.target.value)} />
          </label>
        )}
        {!ticket && !manual && (
          <p className="process-sheet__note">
            It also starts by itself from now on. This first run starts straight away with the latest {template.chip.toLowerCase()} values.
          </p>
        )}

        <div className="process-sheet__pair">
          <div className="process-sheet__group">
            <div className="process-label">Repository</div>
            <div className="process-select">
              <span className="process-select__value process-select__value--mono">augry</span>
              <span className="process-select__caret" aria-hidden="true">▼</span>
            </div>
          </div>
          <div className="process-sheet__group">
            <div className="process-label">Works in</div>
            <button type="button" className="process-select" onClick={() => setWorktree((v) => !v)} title="Switch between a new worktree and the repo root">
              <span className="process-select__value">{worktree ? "New worktree" : "Repo root"}</span>
              <span className="process-select__detail">{worktree ? branch : "current branch"}</span>
              <span className="process-select__caret" aria-hidden="true">▼</span>
            </button>
          </div>
        </div>

        <div className="process-sheet__what">
          <div className="process-label">What happens</div>
          <div className="process-sheet__chips">
            {chips.map((c) => (
              <span key={c.id} className={cx("process-surf", `process-tone--${c.tone}`)}>
                <ProcessIcon name={c.icon} size={15} />
                <strong>{c.name}</strong>
                <span className="process-surf__count">{c.count}</span>
              </span>
            ))}
          </div>
          {stops.length > 0 && (
            <div className="process-sheet__stops">
              <span className="process-dot process-dot--warn" />
              Stops for you {NUMBERS[stops.length] ?? `${stops.length} times`}: {stops.join(", ")}
            </div>
          )}
        </div>
      </div>

      <div className="process-sheet__foot">
        <button type="button" className="process-btn" onClick={() => go(false)}>
          Edit before running
        </button>
        <div className="process-sheet__fill" />
        <button type="button" className="process-btn process-btn--quiet" onClick={close}>
          Cancel
        </button>
        <button type="button" className="process-btn process-btn--primary" onClick={() => go(true)}>
          <span className="process-btn__play" aria-hidden="true">▶</span>
          Copy and run
          <kbd className="process-kbd">Ctrl+Enter</kbd>
        </button>
      </div>
    </Sheet>
  );
}
