import { useCallback, useState, type KeyboardEvent } from "react";
import { addSurface, insertStep, removeStep, updateStart, type StepKind } from "../../process/edit";
import { call } from "../../ipc/client";
import { defaultStart, processFile } from "../../process/model";
import { editorUi, recordOf, teamOf, type Selection } from "../../process/state";
import type { Process } from "../../process/types";
import { changeSentence, latest, summarizeChanges, working } from "../../process/versions";
import { cx } from "../../lib/cx";
import { AddStep } from "./AddStep";
import { Inspector } from "./Inspector";
import { JsonPanel } from "./JsonPanel";
import { EditBar } from "./ProcessBar";
import { ProcessLanes } from "./ProcessLanes";
import { Runbook } from "./Runbook";
import { StartPicker } from "./StartPicker";
import { useProcess } from "./store";

/**
 * Editing a process (boards 03 to 08): opened with Edit. The dotted grid
 * and the Editing label mark this view. Select a step, decision, stage or the
 * start point to change it in the side panel; Add step, Lanes / Runbook and
 * JSON live here too. Edits go into an autosaved draft; Ctrl+S or Run saves
 * the draft as a version.
 */
export function ProcessEditor({ processId }: { processId: string }) {
  const { state, dispatch, saveProcess, runProcess, offline } = useProcess();
  const record = recordOf(state, processId);
  const ui = editorUi(state, processId);
  const [picker, setPicker] = useState(false);
  const [palette, setPalette] = useState(false);
  const closePalette = useCallback(() => setPalette(false), []);
  if (!record) return <Missing />;

  const process = working(record);
  const saved = latest(record);
  const changes = record.draft ? summarizeChanges(saved.process, record.draft) : [];
  const selection = ui.selection;
  const selectedStep = selection?.kind === "step" ? process.steps.find((s) => s.id === selection.id) : undefined;

  const edit = (next: Process) => dispatch({ type: "edit", processId, process: next });
  const select = (next: Selection) => {
    dispatch({ type: "select", processId, selection: next });
    setPicker(next?.kind === "start");
  };
  const add = (kind: StepKind) => {
    const result = insertStep(process, kind, selectedStep?.id ?? null);
    edit(result.process);
    dispatch({ type: "select", processId, selection: { kind: "step", id: result.id } });
    setPalette(false);
  };
  const save = () => {
    // A team process can't save while offline; its draft waits.
    if (record.draft && !offline(processId)) saveProcess(processId, changeSentence(changes));
  };

  const onKeyDown = (e: KeyboardEvent) => {
    const typing = (e.target as HTMLElement).closest("input, textarea");
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      save();
    } else if (typing) {
      return;
    } else if (e.key === "Escape" && selection) {
      select(null);
    } else if ((e.key === "Delete" || e.key === "Backspace") && selectedStep && (e.target as HTMLElement).dataset.step === selectedStep.id) {
      edit(removeStep(process, selectedStep.id));
      select(null);
    }
  };

  // Tab on the canvas itself (not a control inside it) opens Add step.
  const onCanvasKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Tab" && !e.shiftKey && e.target === e.currentTarget) {
      e.preventDefault();
      setPalette(true);
    }
  };

  const side = ui.json ? "json" : selection ? "panel" : null;
  const top = ui.layout === "lanes" ? 68 : 76;

  return (
    <div className={cx("process-editor", side && `process-editor--${side}`)} onKeyDown={onKeyDown}>
      <div className="process-canvas" tabIndex={0} onKeyDown={onCanvasKey} aria-label={`${process.name}, editing. Press Tab to add a step.`}>
        {ui.layout === "lanes" ? (
          <ProcessLanes
            process={process}
            selection={selection}
            onSelect={select}
            onAddRow={(kind) => edit(addSurface(process, kind))}
            colW={side ? 156 : 196}
            top={top}
            scrollKey={`editor-lanes-${processId}`}
          />
        ) : (
          <Runbook process={process} selection={selection} onSelect={select} top={top} scrollKey={`editor-runbook-${processId}`} />
        )}
        <div className="process-canvas__bar">
          <EditBar
            processId={processId}
            name={process.name}
            version={saved.version}
            changes={changes.length}
            layout={ui.layout}
            json={ui.json}
            onLayout={(layout) => dispatch({ type: "editor-ui", processId, patch: { layout } })}
            onJson={() => dispatch({ type: "editor-ui", processId, patch: { json: !ui.json } })}
            onRun={() => runProcess(processId)}
          />
        </div>
        {picker && selection?.kind === "start" && (
          <StartPicker
            current={process.start.on}
            onPick={(kind) => {
              if (kind !== process.start.on) edit(updateStart(process, defaultStart(kind, process.start)));
              setPicker(false);
            }}
            onClose={() => setPicker(false)}
          />
        )}
        <AddStep open={palette} onOpen={() => setPalette(true)} onClose={closePalette} onAdd={add} after={selectedStep?.name ?? null} />
      </div>
      {side === "json" && (
        <JsonPanel
          process={process}
          file={teamOf(state, processId) ? null : processFile(processId)}
          version={saved.version}
          selection={selection}
          onSelect={(s) => dispatch({ type: "select", processId, selection: s })}
          onChange={edit}
          onOpenFile={teamOf(state, processId) ? undefined : () => call("process_open_file", { id: processId })}
        />
      )}
      {side === "panel" && selection && <Inspector process={process} selection={selection} onChange={edit} onSelect={select} />}
    </div>
  );
}

export function Missing() {
  const { dispatch } = useProcess();
  return (
    <div className="process-missing">
      <p>This process is no longer here.</p>
      <button type="button" className="process-btn" onClick={() => dispatch({ type: "navigate", route: { view: "home" } })}>
        Back to Processes
      </button>
    </div>
  );
}
