import { recordOf } from "../../process/state";
import { working } from "../../process/versions";
import { Missing } from "./ProcessEditor";
import { RunBar } from "./ProcessBar";
import { ProcessLanes } from "./ProcessLanes";
import { useProcess } from "./store";

/**
 * A run (board 02): its own view, never inside the editor. It draws the
 * version the run started with, what each row is doing, where it is up to and
 * how far each loop got. View only; Edit process opens the editor.
 */
export function RunView({ runId }: { runId: string }) {
  const { state } = useProcess();
  const run = state.runs.find((r) => r.id === runId);
  if (!run) return <Missing />;
  const record = recordOf(state, run.processId);
  const name = record ? working(record).name : run.process.name;

  return (
    <div className="process-runview">
      <div className="process-canvas process-canvas--run">
        <ProcessLanes process={run.process} run={run} colW={196} scrollKey={`run-${run.id}`} />
        <div className="process-canvas__bar">
          <RunBar run={run} name={name} />
        </div>
      </div>
    </div>
  );
}
