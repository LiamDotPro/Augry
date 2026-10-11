import { useEffect, useState, type ReactNode } from "react";
import { removeStep, renameStage, updateStart, updateStep } from "../../process/edit";
import { isRepeat, list, startKindInfo, surfaceMeta, type LaneTone } from "../../process/model";
import type { Selection } from "../../process/state";
import type { Process, ProcessStart, Step } from "../../process/types";
import { cx } from "../../lib/cx";
import { InspectorField, type Field } from "./InspectorField";
import { ProcessIcon } from "./ProcessIcon";
import { useProcess } from "./store";

/** Sample values a starting point passes to steps, for "Steps can use". */
const SAMPLES: Record<string, [string, string][]> = {
  ticket: [["title", "Settings: font size doesn’t apply to new panes"], ["number", "142"], ["url", "github.com/liam-dev/augry/issues/142"]],
  webhook: [["title", "TypeError in parseDate"], ["url", "localhost:5173/reports"], ["stack", "12 frames"]],
  folder: [["path", "C:\\Users\\liam\\crash-dumps\\crash-2026-10-11.log"], ["name", "crash-2026-10-11.log"], ["size", "48 KB"]],
  manual: [["text", "What you typed when you started it"]],
  schedule: [["time", "Mon 09:00"]],
  branch: [["branch", "main"], ["sha", "9f3c1a2"]],
  notes: [["path", "~/notes/inbox/idea.md"], ["text", "The note’s text"]],
};

/**
 * The editor's side panel (board 03, 04): the selected step, decision,
 * stage or starting point, with its settings as fields. Every change goes
 * through src/process/edit.ts into the draft.
 */
export function Inspector({
  process,
  selection,
  onChange,
  onSelect,
}: {
  process: Process;
  selection: NonNullable<Selection>;
  onChange(process: Process): void;
  onSelect(selection: Selection): void;
}) {
  const close = () => onSelect(null);
  if (selection.kind === "start") return <StartPanel process={process} onChange={onChange} onClose={close} />;
  if (selection.kind === "stage") {
    const stage = process.stages.find((g) => g.id === selection.id);
    if (!stage) return null;
    const steps = process.steps.filter((s) => s.stage === stage.id);
    const fields: Field[] = [
      { type: "text", label: "Name", value: stage.name, onChange: (v) => onChange(renameStage(process, stage.id, v)) },
      {
        type: "steps",
        label: "Steps",
        items: steps.map((s) => {
          const surface = process.surfaces.find((x) => x.id === s.on);
          const meta = surfaceMeta(surface);
          return {
            id: s.id,
            title: s.name,
            who: meta.name,
            family: surface?.kind === "you" ? ("human" as const) : surface?.agent ? ("agent" as const) : ("auto" as const),
            agent: meta.tone,
            onClick: () => onSelect({ kind: "step", id: s.id }),
          };
        }),
        hint: steps.length ? "Select a step to change it." : "No steps yet. Add one with Add step.",
      },
    ];
    return (
      <Panel icon="branch" tone="neutral" title={stage.name} sub={`Stage · ${steps.length} ${steps.length === 1 ? "step" : "steps"}`} onClose={close} fields={fields} />
    );
  }
  const step = process.steps.find((s) => s.id === selection.id);
  if (!step) return null;
  return <StepPanel process={process} step={step} onChange={onChange} onSelect={onSelect} />;
}

function Panel({
  icon,
  tone,
  title,
  sub,
  onClose,
  fields,
  foot,
}: {
  icon: string;
  tone: LaneTone | "start" | "neutral";
  title: string;
  sub: string;
  onClose(): void;
  fields: Field[];
  foot?: ReactNode;
}) {
  return (
    <aside className="process-panel" aria-label={`${title} settings`}>
      <div className="process-panel__head">
        <div className={cx("process-panel__tile", `process-tone--${tone}`)}>
          <ProcessIcon name={icon} size={20} />
        </div>
        <div className="process-panel__titles">
          <div className="process-panel__title">{title}</div>
          <div className="process-panel__sub">{sub}</div>
        </div>
        <button type="button" className="process-x" aria-label="Close" onClick={onClose}>
          ×
        </button>
      </div>
      <div className="process-panel__body">
        {fields.map((f) => (
          <InspectorField key={f.label} field={f} />
        ))}
      </div>
      {foot && <div className="process-panel__foot">{foot}</div>}
    </aside>
  );
}

function StepPanel({ process, step, onChange, onSelect }: { process: Process; step: Step; onChange(p: Process): void; onSelect(s: Selection): void }) {
  const { state } = useProcess();
  const [tested, setTested] = useState<string | null>(null);
  const [confirm, setConfirm] = useState(false);
  useEffect(() => {
    setTested(null);
    setConfirm(false);
  }, [step.id]);

  const surface = process.surfaces.find((s) => s.id === step.on);
  const meta = surfaceMeta(surface);
  const stage = process.stages.find((g) => g.id === step.stage);
  const set = (patch: Partial<Step>) => onChange(updateStep(process, step.id, patch));
  const name = (id: string) => process.steps.find((s) => s.id === id)?.name ?? id;
  const fields: Field[] = [{ type: "text", label: "Name", value: step.name, onChange: (v) => set({ name: v }) }];
  let kind = "Step";
  let icon = meta.icon;

  const agentSurface = (agent: string) => process.surfaces.find((s) => s.agent?.includes(agent))?.id;
  const terminalField = (): Field => ({
    type: "select",
    label: "Terminal",
    icon: "term",
    value: String(step.pane ?? "New pane"),
    detail: `in ${surface?.cwd === "worktree" ? "the worktree" : "the repo root"}`,
    options: [{ value: "New pane" }, { value: "Reuse a pane", detail: "the row’s last pane" }],
    onChange: (v) => set({ pane: v === "New pane" ? undefined : v }),
  });

  if (step.decide) {
    kind = "Decision";
    icon = "decide";
    const by = step.decide !== "ask" ? "A rule" : surface?.kind === "you" ? "You" : "An agent";
    fields.push({
      type: "segmented",
      label: "Decided by",
      options: ["A rule", "An agent", "You"],
      value: by,
      onChange: (v) => {
        if (v === "A rule") set({ decide: String(step.rule ?? "true"), question: undefined, rule: undefined });
        else {
          const on = v === "You" ? "you" : surface?.agent ? step.on : (agentSurface("claude") ?? step.on);
          set({ decide: "ask", question: step.question ?? `${step.name}`, on, rule: step.decide !== "ask" ? step.decide : step.rule });
        }
      },
    });
    if (step.decide !== "ask") {
      fields.push({ type: "text", label: "Rule", mono: true, value: step.decide, onChange: (v) => set({ decide: v }), hint: "Reads the output of earlier steps, like checks.exitCode." });
    } else {
      fields.push({ type: "text", label: "Question", value: step.question ?? "", onChange: (v) => set({ question: v }) });
    }
    const yes = list(step.yes).map(name).join(", ") || "Nothing yet";
    const no = isRepeat(step.no)
      ? { k: "no", v: `Repeat from ${name(step.no.repeatFrom)}`, detail: `max ${step.no.max}` }
      : { k: "no", v: (Array.isArray(step.no) && step.no.length ? step.no.map(name).join(", ") : "Ends this branch") };
    fields.push({ type: "table", label: "Branches", rows: [{ k: "yes", v: yes }, no] });
    if (isRepeat(step.no)) {
      const no2 = step.no;
      fields.push({
        type: "segmented",
        label: "Most tries",
        options: ["2", "3", "5"],
        value: String(no2.max),
        onChange: (v) => set({ no: { ...no2, max: Number(v) } }),
      });
      fields.push({
        type: "segmented",
        label: "After the last try",
        options: ["Ask me", "Stop the run"],
        value: step.onLimit === "stop" ? "Stop the run" : "Ask me",
        onChange: (v) => set({ onLimit: v === "Stop the run" ? "stop" : "ask" }),
      });
    }
  } else if (surface?.kind === "you") {
    const mode = step.approve !== undefined ? "Approve" : step.ask !== undefined ? "Answer" : "Review";
    kind = mode === "Answer" ? "Question for you" : mode === "Approve" ? "Approval" : "Review";
    icon = mode === "Answer" ? "question" : mode === "Approve" ? "gate" : "review";
    const current = String(step.approve ?? step.review ?? step.ask ?? "");
    fields.push({
      type: "segmented",
      label: "Asks you to",
      options: ["Approve", "Review", "Answer"],
      value: mode,
      onChange: (v) =>
        set({
          approve: v === "Approve" ? current || "plan.md" : undefined,
          review: v === "Review" ? current || "diff" : undefined,
          ask: v === "Answer" ? current || "What should happen next?" : undefined,
        }),
    });
    const key = mode === "Approve" ? "approve" : mode === "Answer" ? "ask" : "review";
    fields.push({
      type: "text",
      label: mode === "Approve" ? "What to approve" : mode === "Answer" ? "Question" : "What to review",
      mono: mode !== "Answer",
      value: current,
      onChange: (v) => set({ [key]: v }),
    });
    const choices = (step.choices as string[] | undefined) ?? ["approve", "back"];
    const toggle = (c: string) => (on: boolean) => set({ choices: on ? [...choices, c] : choices.filter((x) => x !== c) });
    fields.push({
      type: "checks",
      label: "Your choices",
      items: [
        { label: mode === "Answer" ? "Answer" : "Approve", detail: "continues", on: choices.includes("approve"), onChange: toggle("approve") },
        { label: "Send back", detail: "runs the step before again", on: choices.includes("back"), onChange: toggle("back") },
        { label: "Stop the run", on: choices.includes("stop"), onChange: toggle("stop") },
      ],
    });
    const notify = (step.notify as { badge?: boolean; sound?: boolean } | undefined) ?? { badge: true, sound: false };
    fields.push({
      type: "switches",
      label: "Let me know",
      items: [
        { label: "Badge on the Process tab", on: notify.badge !== false, onChange: (on) => set({ notify: { ...notify, badge: on } }) },
        { label: "Play a sound", on: !!notify.sound, onChange: (on) => set({ notify: { ...notify, sound: on } }) },
      ],
    });
  } else if (surface?.kind === "browser") {
    kind = "Browser step";
    fields.push({ type: "text", label: "Opens", mono: true, value: step.open ?? "", onChange: (v) => set({ open: v }) });
    fields.push({ type: "text", label: "Saves", mono: true, value: step.saves ?? "", placeholder: "screenshots/", onChange: (v) => set({ saves: v || undefined }) });
    fields.push({ type: "select", label: "Browser", icon: "globe", value: "Controlled Chrome", detail: "its own profile" });
  } else if (surface?.agent || step.prompt !== undefined) {
    kind = "Agent step";
    const agent = meta.tone === "claude" ? "Claude Code" : meta.tone === "codex" ? "Codex" : "Custom";
    fields.push({
      type: "agents",
      label: "Agent",
      options: ["Claude Code", "Codex", "Custom"],
      value: agent,
      onChange: (v) => {
        const on = v === "Claude Code" ? agentSurface("claude") : v === "Codex" ? agentSurface("codex") : process.surfaces.find((s) => s.kind === "terminal" && !s.agent)?.id;
        if (on) set({ on });
      },
    });
    fields.push({ type: "text", label: "Task", multiline: true, value: step.prompt ?? "", onChange: (v) => set({ prompt: v }), hint: "Use {{ticket.title}} and the like for what the start or earlier steps give." });
    fields.push({ type: "text", label: "Saves", mono: true, value: step.saves ?? "", placeholder: "plan.md", onChange: (v) => set({ saves: v || undefined }) });
    fields.push(terminalField());
    fields.push({
      type: "segmented",
      label: "Done when",
      options: ["Agent finishes", "Idle 30s", "I mark it"],
      value: String(step.doneWhen ?? "Agent finishes"),
      onChange: (v) => set({ doneWhen: v === "Agent finishes" ? undefined : v }),
    });
  } else {
    kind = "Shell step";
    fields.push({ type: "text", label: "Command", mono: true, value: step.run ?? "", onChange: (v) => set({ run: v }) });
    fields.push(terminalField());
    fields.push({
      type: "segmented",
      label: "Done when",
      options: ["Command exits", "I mark it"],
      value: String(step.doneWhen ?? "Command exits"),
      onChange: (v) => set({ doneWhen: v === "Command exits" ? undefined : v }),
    });
  }

  if (process.stages.length > 1) {
    fields.push({
      type: "select",
      label: "Stage",
      icon: "branch",
      value: stage?.name ?? step.stage,
      options: process.stages.map((g) => ({ value: g.name })),
      onChange: (v) => {
        const g = process.stages.find((x) => x.name === v);
        if (g) set({ stage: g.id });
      },
    });
  }

  const lastRun = state.runs.find((r) => r.processId === process.id && r.steps[step.id]?.state === "done");
  const test = () => {
    if (!lastRun) return setTested("No run has reached this step yet.");
    const note = lastRun.steps[step.id]!.note;
    setTested(`Run ${lastRun.number}: ${step.decide === "ask" ? "answered" : step.decide} → ${note === "yes" ? "yes" : note || "yes"}`);
  };

  return (
    <Panel
      icon={icon}
      tone={step.decide ? "shell" : meta.tone}
      title={step.name}
      sub={`${kind} · ${stage?.name ?? step.stage}`}
      onClose={() => onSelect(null)}
      fields={fields}
      foot={
        <>
          {tested && (
            <div className="process-panel__result" role="status">
              {tested}
            </div>
          )}
          <div className="process-panel__buttons">
            {step.decide && (
              <button type="button" className="process-btn process-btn--solid process-btn--grow" onClick={test}>
                Test with last run
              </button>
            )}
            <button
              type="button"
              className={cx("process-btn process-btn--danger", !step.decide && "process-btn--grow")}
              onClick={() => {
                if (!confirm) return setConfirm(true);
                onChange(removeStep(process, step.id));
                onSelect(null);
              }}
            >
              {confirm ? "Delete this step" : "Delete"}
            </button>
          </div>
        </>
      }
    />
  );
}

function StartPanel({ process, onChange, onClose }: { process: Process; onChange(p: Process): void; onClose(): void }) {
  const { state, dispatch } = useProcess();
  const [result, setResult] = useState<string | null>(null);
  const start = process.start;
  const info = startKindInfo(start.on);
  const as = start.as ?? info.kind;
  const set = (patch: Partial<ProcessStart>) => {
    const next = { ...start, ...patch };
    for (const [k, v] of Object.entries(patch)) if (v === undefined) delete next[k];
    onChange(updateStart(process, next));
  };
  useEffect(() => setResult(null), [start.on]);

  const uses: Field = {
    type: "table",
    label: "Steps can use",
    rows: (SAMPLES[start.on] ?? []).map(([k, v]) => ({ k: `${as}.${k}`, v, mono: true })),
  };
  const fields: Field[] = [];
  let foot: ReactNode = null;

  switch (start.on) {
    case "webhook":
      fields.push(
        { type: "select", label: "Listens on", icon: "webhook", tone: "ok", value: "POST localhost:4319", detail: String(start.path ?? "") },
        { type: "text", label: "Path", mono: true, value: String(start.path ?? ""), onChange: (v) => set({ path: v }) },
        {
          type: "segmented",
          label: "Reachable from",
          options: ["This computer", "Relay"],
          value: start.reach === "relay" ? "Relay" : "This computer",
          onChange: (v) => set({ reach: v === "Relay" ? "relay" : "local" }),
          hint: "The relay forwards Sentry or GitHub webhooks to this computer while Augry is open.",
        },
        uses,
        {
          type: "switches",
          label: "When a request arrives",
          items: [
            { label: "Start a run right away", on: !start.confirm, onChange: (on) => set({ confirm: on ? undefined : true }) },
            { label: "Ask me first", on: !!start.confirm, onChange: (on) => set({ confirm: on ? true : undefined }) },
          ],
        },
      );
      foot = (
        <button type="button" className="process-btn process-btn--solid process-btn--grow" onClick={() => setResult(`200 OK · ${as}.title = “TypeError in parseDate”. A run would start.`)}>
          Send test request
        </button>
      );
      break;
    case "folder": {
      const options = (start.options as Record<string, boolean> | undefined) ?? { settle: true, ignoreExisting: true };
      const opt = (key: string) => (on: boolean) => set({ options: { ...options, [key]: on } });
      fields.push(
        { type: "text", label: "Watch", mono: true, value: String(start.path ?? ""), onChange: (v) => set({ path: v }), hint: "A folder on this computer, like ~/crash-dumps." },
        { type: "text", label: "Files that match", mono: true, value: String(start.match ?? "*"), onChange: (v) => set({ match: v }) },
        {
          type: "segmented",
          label: "Start when a file is",
          options: ["Added", "Added or changed"],
          value: start.when === "changed" ? "Added or changed" : "Added",
          onChange: (v) => set({ when: v === "Added" ? "added" : "changed" }),
        },
        {
          type: "switches",
          label: "Options",
          items: [
            { label: "Include subfolders", on: !!options.subfolders, onChange: opt("subfolders") },
            { label: "Wait until the file stops changing", on: options.settle !== false, onChange: opt("settle") },
            { label: "Ignore files already there", on: options.ignoreExisting !== false, onChange: opt("ignoreExisting") },
          ],
        },
        {
          type: "segmented",
          label: "If several arrive",
          options: ["One run each", "One run for all"],
          value: start.batch ? "One run for all" : "One run each",
          onChange: (v) => set({ batch: v === "One run for all" ? true : undefined }),
        },
        uses,
      );
      foot = (
        <button type="button" className="process-btn process-btn--grow" onClick={() => dispatch({ type: "file-arrived", processId: process.id, now: Date.now() })}>
          <ProcessIcon name="folder" size={14} stroke={1.8} />
          Test with the newest file
        </button>
      );
      break;
    }
    case "ticket": {
      const sources = state.connections.filter((c) => c.group === "work" && c.on);
      const source = sources.find((c) => c.icon === start.source) ?? sources[0];
      fields.push(
        {
          type: "select",
          label: "From",
          icon: source?.icon ?? "ticket",
          value: source?.name ?? "GitHub Issues",
          detail: source?.status.replace("Connected · ", ""),
          options: sources.map((c) => ({ value: c.name, detail: c.status })),
          onChange: (v) => set({ source: sources.find((c) => c.name === v)?.icon }),
        },
        { type: "text", label: "Issues that match", mono: true, value: String(start.query ?? ""), onChange: (v) => set({ query: v }) },
        uses,
        {
          type: "switches",
          label: "When a run starts",
          items: [
            { label: "Comment on the issue", on: start.comment !== false, onChange: (on) => set({ comment: on ? undefined : false }) },
            { label: "Assign it to me", on: !!start.assign, onChange: (on) => set({ assign: on ? true : undefined }) },
          ],
        },
      );
      break;
    }
    case "schedule":
      fields.push(
        { type: "text", label: "When", mono: true, value: String(start.cron ?? ""), onChange: (v) => set({ cron: v }), hint: "Like weekdays 09:00 or weekly mon 09:00." },
        {
          type: "segmented",
          label: "If the computer was off",
          options: ["Skip it", "Run when it wakes"],
          value: start.catchUp ? "Run when it wakes" : "Skip it",
          onChange: (v) => set({ catchUp: v === "Run when it wakes" ? true : undefined }),
        },
        uses,
      );
      break;
    case "branch":
      fields.push(
        {
          type: "segmented",
          label: "On",
          options: ["Push", "PR opened", "PR review"],
          value: start.event === "pr-opened" ? "PR opened" : start.event === "pr-review" ? "PR review" : "Push",
          onChange: (v) => set({ event: v === "Push" ? "push" : v === "PR opened" ? "pr-opened" : "pr-review" }),
        },
        { type: "text", label: "Branch", mono: true, value: String(start.branch ?? "main"), onChange: (v) => set({ branch: v }) },
        uses,
      );
      break;
    case "notes":
      fields.push({ type: "text", label: "Folder", mono: true, value: String(start.path ?? ""), onChange: (v) => set({ path: v }) }, uses);
      break;
    case "manual":
      fields.push(
        { type: "text", label: "Asks for", value: String(start.prompt ?? "What is this run for?"), onChange: (v) => set({ prompt: v }) },
        uses,
      );
      break;
  }
  fields.push({ type: "text", label: "Values are called", mono: true, value: as, onChange: (v) => set({ as: v.replace(/[^\w]/g, "") || undefined }), hint: `Steps write {{${as}.…}} to use them.` });

  return (
    <Panel
      icon={info.icon}
      tone="start"
      title={info.name === "Folder" ? "Watch a folder" : info.name}
      sub={`Starts ${process.name}`}
      onClose={onClose}
      fields={fields}
      foot={
        foot || result ? (
          <>
            {result && (
              <div className="process-panel__result process-panel__result--ok" role="status">
                {result}
              </div>
            )}
            {foot && <div className="process-panel__buttons">{foot}</div>}
          </>
        ) : undefined
      }
    />
  );
}
