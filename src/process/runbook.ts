/**
 * The runbook: the same process read top to bottom, stage by stage. Loops and
 * steps that run together become indented groups. Port of the list view in
 * Components.dc.html (use="lanes", view="list").
 */

import { buildGraph } from "./graph";
import { isRepeat, lineOf, list, startKindInfo, startTitle, surfaceMeta, type LaneTone } from "./model";
import type { Process } from "./types";

export type RunbookRow =
  | { kind: "stage"; key: string; n: number; name: string; note: string }
  | { kind: "mark"; key: string; indent: number; icon: string; role: "start" | "loop" | "together" | "end"; name: string; line: string }
  | {
      kind: "step";
      key: string;
      id: string;
      indent: number;
      name: string;
      line: string;
      surface: string;
      icon: string;
      tone: LaneTone;
      round: boolean;
      decision: boolean;
    };

const INDENT = 24;

export function runbookRows(process: Process): RunbookRow[] {
  const graph = buildGraph(process);
  const { steps, byId, col, loops } = graph;
  const inLoops = (id: string) => loops.filter((l) => l.steps.includes(id));
  const start = startKindInfo(process.start.on);
  const rows: RunbookRow[] = [
    { kind: "mark", key: "start", indent: 0, icon: start.icon, role: "start", name: "Starts from", line: startTitle(process.start) },
  ];

  let open: string[] = [];
  process.stages.forEach((g, gi) => {
    const ss = steps.filter((s) => s.stage === g.id).sort((a, b) => col[a.id]! - col[b.id]!);
    rows.push({ kind: "stage", key: `stage-${g.id}`, n: gi + 1, name: g.name, note: `${ss.length} ${ss.length === 1 ? "step" : "steps"}` });
    for (const s of ss) {
      const ls = inLoops(s.id);
      ls.forEach((l, li) => {
        if (open.includes(l.id)) return;
        open.push(l.id);
        const count = l.kind === "repeat" ? `max ${l.max}` : "";
        rows.push({ kind: "mark", key: `loop-${l.id}-${s.id}`, indent: INDENT * li, icon: "loop", role: "loop", name: l.label, line: count });
      });
      open = open.filter((id) => ls.some((l) => l.id === id));
      const indent = INDENT * ls.length;
      const same = ss.filter((x) => col[x.id] === col[s.id]);
      if (same.length > 1 && same[0] === s) {
        rows.push({ kind: "mark", key: `together-${s.id}`, indent, icon: "branch", role: "together", name: "At the same time", line: "" });
      }
      const meta = surfaceMeta(process.surfaces.find((x) => x.id === s.on));
      const decision = !!s.decide;
      const names = (ids: string[]) => ids.map((t) => byId[t]?.name ?? t).join(", ");
      const line = decision
        ? `yes → ${names(list(s.yes))} · no → ${isRepeat(s.no) ? "repeat" : names(Array.isArray(s.no) ? s.no : [])}`
        : lineOf(s);
      rows.push({
        kind: "step", key: `step-${s.id}`, id: s.id, indent: indent + (same.length > 1 ? INDENT : 0), name: s.name, line,
        surface: meta.name, icon: decision ? "decide" : meta.icon, tone: meta.tone, round: meta.round, decision,
      });
    }
  });

  for (const e of process.ends ?? []) {
    rows.push({ kind: "mark", key: `end-${e.name}`, indent: 0, icon: "end", role: "end", name: e.name, line: `after ${byId[e.after]?.name ?? e.after}` });
  }
  return rows;
}
