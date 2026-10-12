import { useEffect, useMemo, type KeyboardEvent } from "react";
import type { BlockMark } from "../../process/layout";
import { recordOf, teamOf, versionsUi } from "../../process/state";
import type { Process } from "../../process/types";
import { agoWords } from "../../process/runs";
import { changeSentence, latest, summarizeChanges, versionDiff, versionOf, versionTags, working } from "../../process/versions";
import { cx } from "../../lib/cx";
import { DiffView } from "./CodeView";
import { EditBar } from "./ProcessBar";
import { Missing } from "./ProcessEditor";
import { ProcessIcon } from "./ProcessIcon";
import { ProcessLanes } from "./ProcessLanes";
import { SAVE_OFFLINE, useProcess } from "./store";
import { useKeptScroll } from "./useKeptScroll";

/**
 * Versions (board 09): opened from the version chip in the bar. The draft,
 * numbered versions with the runs that used them, and a compare of any
 * version with the current one, as JSON or lanes. Restore saves the old
 * content as a new version, so history only grows. A team process's history
 * comes from the team (its versions are fetched when this opens), and saving
 * or restoring writes the team's next version.
 */
export function VersionsView({ processId }: { processId: string }) {
  const { state, dispatch, now, team, saveProcess, runProcess, restoreProcess, offline } = useProcess();
  const record = recordOf(state, processId);
  const ui = versionsUi(state, processId);
  const listScroll = useKeptScroll<HTMLDivElement>(`versions-list-${processId}`);
  const diffScroll = useKeptScroll<HTMLDivElement>(`versions-diff-${processId}`);
  const inTeam = teamOf(state, processId) !== null;
  const cantSave = offline(processId);

  // Team versions live on the server; fetch them each time Versions opens.
  const loadVersions = team?.loadVersions;
  useEffect(() => {
    if (inTeam) loadVersions?.(processId);
  }, [inTeam, processId, loadVersions]);

  const current = record ? latest(record) : null;
  const draft = record?.draft ?? null;
  // The newer side is the draft when there is one, else the current version.
  const to: { label: string; process: Process } | null = current ? (draft ? { label: "Draft", process: draft } : { label: `v${current.version}`, process: current.process }) : null;
  let fromVersion = ui.compare ?? current?.version ?? 1;
  if (record && !draft && fromVersion === current?.version && record.versions.length > 1) fromVersion = record.versions[record.versions.length - 2]!.version;
  const from = record ? versionOf(record, fromVersion) : undefined;

  const changes = useMemo(() => (from && to ? summarizeChanges(from.process, to.process) : []), [from, to?.process]);
  const diff = useMemo(() => (from && to ? versionDiff(from.process, to.process) : []), [from, to?.process]);
  const draftChanges = useMemo(() => (draft && current ? summarizeChanges(current.process, draft) : []), [draft, current]);
  const marks = useMemo(() => {
    const out: Record<string, BlockMark> = {};
    for (const c of changes) {
      if (!c.step) continue;
      if (ui.side === "to") {
        if (c.sign === "+") out[c.step] = "added";
        else if (c.sign === "~") out[c.step] = "changed";
      } else if (c.sign === "-") out[c.step] = "removed";
      else if (c.sign === "~") out[c.step] = "changed";
    }
    return out;
  }, [changes, ui.side]);

  if (!record || !current || !to) return <Missing />;
  const process = working(record);
  const nextNumber = current.version + 1;
  const sentence = changeSentence(draftChanges);
  const save = () => !cantSave && saveProcess(processId, ui.message || sentence);
  const set = (patch: Partial<typeof ui>) => dispatch({ type: "versions-ui", processId, patch });
  const canRestore = !!from && (from.version !== current.version || !!draft);

  const onKeyDown = (e: KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      if (draft) save();
    }
  };

  return (
    <div className="process-versions" onKeyDown={onKeyDown}>
      <div className="process-versions__bar">
        <EditBar
          processId={processId}
          name={process.name}
          version={current.version}
          changes={draftChanges.length}
          versionsOpen
          onRun={() => runProcess(processId)}
        />
      </div>

      <aside className="process-versions__list process-glass" aria-label="Versions">
        <div className="process-glass__head">
          <ProcessIcon name="history" size={18} className="process-versions__icon" />
          <h2 className="process-glass__title">Versions</h2>
          <span className="process-versions__id">{inTeam ? process.id : processId}</span>
        </div>
        {draft && (
          <div className="process-draft">
            <div className="process-draft__top">
              <span className="process-dot process-dot--warn" />
              <span className="process-draft__title">
                Draft · {draftChanges.length} change{draftChanges.length === 1 ? "" : "s"}
              </span>
              <span className="process-draft__auto">autosaved</span>
            </div>
            <div className="process-draft__text">{sentence}</div>
            {/* Team versions keep no message, so there is nothing to type. */}
            {!inTeam && (
              <input
                className="process-input process-input--small"
                placeholder={sentence}
                aria-label="Version message"
                value={ui.message}
                onChange={(e) => set({ message: e.target.value })}
                onKeyDown={(e) => e.key === "Enter" && save()}
              />
            )}
            <div className="process-draft__buttons">
              <button
                type="button"
                className="process-btn process-btn--small process-btn--warn"
                disabled={cantSave}
                title={cantSave ? SAVE_OFFLINE : undefined}
                onClick={save}
              >
                Save as v{nextNumber}
                <kbd className="process-kbd process-kbd--bare">Ctrl+S</kbd>
              </button>
              <button type="button" className="process-btn process-btn--small process-btn--quiet" onClick={() => dispatch({ type: "discard", processId })}>
                Discard
              </button>
            </div>
          </div>
        )}
        <div className="process-versions__rows" ref={listScroll.ref} onScroll={listScroll.onScroll}>
          {[...record.versions].reverse().map((v) => {
            const tags = versionTags(record, v, state.runs, from?.version ?? null);
            const on = v.version === from?.version;
            return (
              <button key={v.version} type="button" className={cx("process-version", on && "process-version--on")} aria-pressed={on} onClick={() => set({ compare: v.version })}>
                <span className={cx("process-version__n", v.version === current.version && "process-version__n--current")}>v{v.version}</span>
                <span className="process-version__body">
                  <span className="process-version__msg">{v.message}</span>
                  <span className="process-version__meta">
                    <span>
                      {v.byId && team ? team.nameOf(v.byId) : v.by} · {agoWords(v.at, now)}
                    </span>
                    <span className="process-mono">{v.hash}</span>
                    {tags.map((t) => (
                      <span key={t.text} className={cx("process-tag", `process-tag--${t.tone}`)}>
                        {t.text}
                      </span>
                    ))}
                  </span>
                </span>
              </button>
            );
          })}
        </div>
      </aside>

      <section className="process-versions__compare process-glass" aria-label="Compare">
        <div className="process-glass__head process-compare__head">
          <h2 className="process-glass__title">Compare</h2>
          <span className="process-compare__pair">
            <span className="process-compare__from">v{from?.version}</span>→<span className="process-compare__to">{to.label}</span>
          </span>
          <span className="process-code__fill" />
          <div className="process-seg" role="group" aria-label="Compare as">
            {(["json", "lanes"] as const).map((m) => (
              <button key={m} type="button" className={cx("process-seg__item", ui.mode === m && "process-seg__item--on")} aria-pressed={ui.mode === m} onClick={() => set({ mode: m })}>
                {m === "json" ? "JSON" : "Lanes"}
              </button>
            ))}
          </div>
          <button type="button" className="process-btn process-btn--small" onClick={() => set({ mode: "lanes", side: "from" })}>
            Open v{from?.version}
          </button>
          <button
            type="button"
            className="process-btn process-btn--small process-btn--solid"
            disabled={!canRestore || cantSave}
            title={cantSave ? SAVE_OFFLINE : draft ? "Restoring drops the draft" : undefined}
            onClick={() => from && restoreProcess(processId, from.version)}
          >
            Restore v{from?.version} as v{nextNumber}
          </button>
        </div>
        <div className="process-compare__changes">
          {changes.length ? (
            changes.map((c, i) => (
              <span key={i} className="process-change">
                <span className={cx("process-change__sign", c.sign === "+" ? "process-change__sign--add" : c.sign === "-" ? "process-change__sign--del" : "process-change__sign--mod")}>{c.sign}</span>
                {c.text}
              </span>
            ))
          ) : (
            <span className="process-compare__same">No differences.</span>
          )}
        </div>
        {ui.mode === "json" ? (
          <div className="process-code process-compare__code">
            <div className="process-code__head">
              <ProcessIcon name="braces" size={16} stroke={1.6} className="process-code__headicon" />
              <span className="process-code__file">.augry/processes/{inTeam ? process.id : processId}.json</span>
              <span className="process-code__ver">
                v{from?.version} → {to.label}
              </span>
              <span className="process-code__fill" />
              <span className="process-code__status process-code__status--warn">
                <span className="process-dot process-dot--warn" />
                {changes.filter((c) => c.step).length} steps changed
              </span>
            </div>
            <div className="process-code__scroll" ref={diffScroll.ref} onScroll={diffScroll.onScroll}>
              <DiffView lines={diff} />
            </div>
          </div>
        ) : (
          <div className="process-compare__lanes">
            <div className="process-compare__sides process-seg" role="group" aria-label="Show">
              {(["from", "to"] as const).map((s) => (
                <button key={s} type="button" className={cx("process-seg__item", ui.side === s && "process-seg__item--on")} aria-pressed={ui.side === s} onClick={() => set({ side: s })}>
                  {s === "from" ? `v${from?.version}` : to.label}
                </button>
              ))}
            </div>
            <ProcessLanes
              process={ui.side === "from" && from ? from.process : to.process}
              colW={156}
              top={56}
              marks={marks}
              scrollKey={`versions-lanes-${processId}-${ui.side}`}
            />
          </div>
        )}
      </section>
    </div>
  );
}
