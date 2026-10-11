/**
 * Lane geometry, ported from Components.dc.html (use="lanes"). Rows are the
 * process's surfaces, columns come from buildGraph, decisions are diamonds on
 * the decider's row in a narrow column, stages run along the top, loops along
 * the bottom under the columns they repeat, and the start point sits top-left.
 *
 * Everything here is numbers and state names; the component picks colours
 * from the theme, so Light and Black keep working.
 */

import { buildGraph, firstSteps, type Graph } from "./graph";
import { isRepeat, lineOf, surfaceMeta, type LaneTone, type SurfaceMeta } from "./model";
import type { Process, Run, StepState, Surface } from "./types";

export type BlockState = "idle" | "selected" | StepState;
export type EdgeState = "idle" | "done" | "active" | "pending" | "skipped";
export type StageState = "idle" | "done" | "waiting" | "running" | "pending";
/** How a version compare marks a step. */
export type BlockMark = "added" | "changed" | "removed";

export interface LaneRow {
  id: string;
  y: number;
  surface: Surface;
  meta: SurfaceMeta;
  /** Every other row is lightly washed. */
  alt: boolean;
}

export interface StageBand {
  id: string;
  n: number;
  name: string;
  x: number;
  w: number;
  /** A dashed rule before every stage but the first. */
  sep: boolean;
  sepH: number;
  state: StageState;
  note: string;
}

export interface Block {
  id: string;
  kind: "step" | "decision";
  x: number;
  y: number;
  w: number;
  name: string;
  line: string;
  tone: LaneTone;
  state: BlockState;
  status: string;
  /** Decisions: "?" when someone is asked, "=" for a rule. */
  glyph: string;
  /** Decisions: where the name sits above the diamond. */
  lx: number;
  ly: number;
  mark?: BlockMark;
}

export interface Edge {
  d: string;
  arrow: string;
  state: EdgeState;
  /** The dashed stub under a repeat decision. */
  loopBack?: boolean;
}

export interface EdgeLabel {
  x: number;
  y: number;
  text: string;
  branch: "yes" | "no";
}

export interface LoopBand {
  id: string;
  x: number;
  w: number;
  y: number;
  cx: number;
  ly: number;
  label: string;
  count: string;
  active: boolean;
}

export interface EndMark {
  x: number;
  y: number;
  lx: number;
  ly: number;
  name: string;
  dim: boolean;
}

export interface LaneLayout {
  graph: Graph;
  width: number;
  height: number;
  headW: number;
  laneH: number;
  rulerH: number;
  lanesBottom: number;
  lanes: LaneRow[];
  stages: StageBand[];
  blocks: Block[];
  edges: Edge[];
  labels: EdgeLabel[];
  starts: { x: number; y: number }[];
  ends: EndMark[];
  loops: LoopBand[];
  /** Run mode: the "now" line, at the step running or waiting. */
  play: { x: number; h: number; waiting: boolean } | null;
  /** Run mode: where the waiting card goes, under the step that waits. */
  wait: { x: number; y: number; stepId: string } | null;
}

export interface LayoutOptions {
  colW?: number;
  run?: Run | null;
  selected?: string | null;
  marks?: Record<string, BlockMark>;
}

export const RULER_H = 64;
export const LANE_H = 112;
const DEC_W = 76;
const PAD = 40;
const ENDS_W = 130;

/** Rounds the corners of a polyline into an SVG path. */
export function roundPath(points: [number, number][], r: number): string {
  let d = `M${points[0]![0]} ${points[0]![1]}`;
  for (let i = 1; i < points.length - 1; i++) {
    const [px, py] = points[i - 1]!;
    const [cx, cy] = points[i]!;
    const [nx, ny] = points[i + 1]!;
    const l1 = Math.hypot(cx - px, cy - py);
    const l2 = Math.hypot(nx - cx, ny - cy);
    if (!l1 || !l2) continue;
    const rr = Math.min(r, l1 / 2, l2 / 2);
    d += ` L${cx - ((cx - px) / l1) * rr} ${cy - ((cy - py) / l1) * rr} Q${cx} ${cy} ${cx + ((nx - cx) / l2) * rr} ${cy + ((ny - cy) / l2) * rr}`;
  }
  const last = points[points.length - 1]!;
  return d + ` L${last[0]} ${last[1]}`;
}

const arrow = (x: number, y: number) => `M${x - 6} ${y - 4} L${x} ${y} L${x - 6} ${y + 4}`;

export function layoutLanes(process: Process, options: LayoutOptions = {}): LaneLayout {
  const graph = buildGraph(process);
  const { steps, byId, preds, links, col, nCols } = graph;
  const run = options.run ?? null;
  const sel = options.selected ?? null;
  const colW = options.colW ?? 156;
  const headW = run ? 300 : 240;

  const stState = (id: string): BlockState =>
    run ? (run.steps[id]?.state ?? "pending") : id === sel ? "selected" : "idle";
  const note = (id: string) => (run ? (run.steps[id]?.note ?? "") : "");

  // A column of nothing but decisions is narrow.
  const isDecCol = (c: number) => {
    const inCol = steps.filter((s) => col[s.id] === c);
    return inCol.length > 0 && inCol.every((s) => s.decide);
  };
  const cw = Array.from({ length: nCols }, (_, c) => (isDecCol(c) ? DEC_W : colW));
  const cx0: number[] = [];
  let acc = headW + PAD;
  for (let c = 0; c < nCols; c++) {
    cx0.push(acc);
    acc += cw[c]!;
  }

  const rowIndex: Record<string, number> = {};
  process.surfaces.forEach((s, i) => (rowIndex[s.id] = i));
  const laneY = (i: number) => RULER_H + i * LANE_H;
  // A step on a row that does not exist goes on the first row rather than vanishing.
  const cyOf = (id: string) => laneY(rowIndex[byId[id]!.on] ?? 0) + LANE_H / 2;
  const box = (id: string) => {
    const c = col[id]!;
    if (byId[id]!.decide) {
      const cx = cx0[c]! + DEC_W / 2;
      return { l: cx - 24, r: cx + 24, cx };
    }
    return { l: cx0[c]! + 10, r: cx0[c]! + cw[c]! - 10, cx: cx0[c]! + cw[c]! / 2 };
  };

  const lanesBottom = RULER_H + Math.max(1, process.surfaces.length) * LANE_H;
  const width = cx0[nCols - 1]! + cw[nCols - 1]! + ENDS_W;
  const height = lanesBottom + graph.loops.length * 34 + 70;

  const blocks: Block[] = steps.map((s) => {
    const meta = surfaceMeta(process.surfaces.find((x) => x.id === s.on));
    const st = stState(s.id);
    const c = col[s.id]!;
    const y = cyOf(s.id);
    const mark = options.marks?.[s.id];
    if (s.decide) {
      const cx = cx0[c]! + DEC_W / 2;
      return {
        id: s.id, kind: "decision", x: cx - 17, y: y - 17, w: 34, name: s.name, line: lineOf(s), tone: meta.tone,
        state: st, status: "", glyph: s.decide === "ask" ? "?" : "=", lx: cx - 60, ly: y - 44, mark,
      };
    }
    const n = note(s.id);
    const status = !run || st === "pending" ? "" : st === "done" ? "✓ " + n : n.replace("waiting ", "");
    return {
      id: s.id, kind: "step", x: cx0[c]! + 10, y: y - 32, w: cw[c]! - 20, name: s.name, line: lineOf(s), tone: meta.tone,
      state: st, status, glyph: "", lx: 0, ly: 0, mark,
    };
  });

  const edgeState = (f: string, t: string): EdgeState => {
    if (!run) return "idle";
    const a = stState(f);
    const b = stState(t);
    if (a === "skipped" || b === "skipped") return "skipped";
    if (b === "running" || b === "waiting") return "active";
    if ((a === "done" || a === "stopped") && b !== "pending") return "done";
    return "pending";
  };

  const edges: Edge[] = [];
  const labels: EdgeLabel[] = [];
  for (const k of links) {
    const f = byId[k.from]!;
    const yf = cyOf(k.from);
    const yt = cyOf(k.to);
    const bf = box(k.from);
    const bt = box(k.to);
    const x2 = bt.l - 2;
    const mx = cx0[col[k.to]!]!;
    let pts: [number, number][];
    if (f.decide && k.branch === "no") {
      const vy = yt < yf ? yf - 24 : yf + 24;
      pts = [[bf.cx, vy], [bf.cx, yt], [x2, yt]];
    } else {
      pts = [[bf.r, yf], [mx, yf], [mx, yt], [x2, yt]];
    }
    edges.push({ d: roundPath(pts, 10), arrow: arrow(x2, yt), state: edgeState(k.from, k.to) });
    if (k.branch) labels.push({ x: x2 - 22, y: yt - 13, text: k.branch, branch: k.branch });
  }
  for (const s of steps) {
    if (!s.decide || !isRepeat(s.no) || !byId[s.no.repeatFrom]) continue;
    const b = box(s.id);
    const y = cyOf(s.id);
    edges.push({ d: `M${b.cx} ${y + 24} L${b.cx} ${y + 44}`, arrow: "", state: "idle", loopBack: true });
    labels.push({ x: b.cx, y: y + 54, text: "no ↺", branch: "no" });
  }

  const firsts = firstSteps(graph);
  const starts = firsts.map((s) => ({ x: headW + 16, y: cyOf(s.id) }));
  for (const s of firsts) {
    const y = cyOf(s.id);
    const l = box(s.id).l - 2;
    edges.push({ d: `M${headW + 22} ${y} L${l} ${y}`, arrow: arrow(l, y), state: run ? "done" : "idle" });
  }

  const ends: EndMark[] = (process.ends ?? [])
    .filter((e) => byId[e.after])
    .map((e) => {
      const b = box(e.after);
      const y = cyOf(e.after);
      const st = stState(e.after);
      const reached = !run || st === "done";
      edges.push({ d: `M${b.r} ${y} L${b.r + 30} ${y}`, arrow: "", state: reached ? "idle" : "pending" });
      return { x: b.r + 36, y, lx: b.r + 48, ly: y - 8, name: e.name, dim: st === "skipped" };
    });

  const loops: LoopBand[] = graph.loops.map((l, i) => {
    const x = cx0[l.from]! + 8;
    const w = cx0[l.to]! + cw[l.to]! - 8 - x;
    const y = lanesBottom + 8 + i * 34;
    const count = run ? (run.loops[l.id] ?? "") : l.kind === "repeat" ? `max ${l.max}` : "";
    return { id: l.id, x, w, y, cx: x + w / 2, ly: y + 12, label: l.label, count, active: !!run && /of/.test(count) };
  });

  const lanes: LaneRow[] = process.surfaces.map((surface, i) => ({
    id: surface.id,
    y: laneY(i),
    surface,
    meta: surfaceMeta(surface),
    alt: i % 2 === 1,
  }));

  const stages: StageBand[] = [];
  process.stages.forEach((g, gi) => {
    const inStage = steps.filter((s) => s.stage === g.id);
    if (!inStage.length) return;
    const cs = inStage.map((s) => col[s.id]!);
    const a = Math.min(...cs);
    const b = Math.max(...cs);
    const states = inStage.map((s) => stState(s.id));
    const state: StageState = !run
      ? "idle"
      : states.every((x) => x === "done" || x === "skipped")
        ? "done"
        : states.some((x) => x === "waiting")
          ? "waiting"
          : states.some((x) => x === "running")
            ? "running"
            : "pending";
    const noteText = !run
      ? `${inStage.length} ${inStage.length === 1 ? "step" : "steps"}`
      : { done: "✓ done", waiting: "waiting for you", running: "running", pending: "", idle: "" }[state];
    stages.push({
      id: g.id, n: gi + 1, name: g.name, x: cx0[a]!, w: cx0[b]! + cw[b]! - cx0[a]!, sep: stages.length > 0,
      sepH: lanesBottom - 12, state, note: noteText,
    });
  });

  const now = run ? steps.find((s) => ["running", "waiting"].includes(stState(s.id))) : undefined;
  const play = now
    ? { x: cx0[col[now.id]!]! + 4, h: lanesBottom - 48, waiting: stState(now.id) === "waiting" }
    : null;
  const waiting = run ? steps.find((s) => stState(s.id) === "waiting" && !s.decide) : undefined;
  const wait = waiting ? { x: box(waiting.id).l, y: cyOf(waiting.id) + 44, stepId: waiting.id } : null;

  return {
    graph, width, height, headW, laneH: LANE_H, rulerH: RULER_H, lanesBottom,
    lanes, stages, blocks, edges, labels, starts, ends, loops, play, wait,
  };
}

/** The small preview on a process card: [column, row, decision] per step. */
export function miniBlocks(process: Process): { col: number; row: number; decision: boolean; tone: LaneTone }[] {
  const graph = buildGraph(process);
  const rowIndex: Record<string, number> = {};
  process.surfaces.forEach((s, i) => (rowIndex[s.id] = i));
  return graph.steps.map((s) => ({
    col: graph.col[s.id]!,
    row: rowIndex[s.on] ?? 0,
    decision: !!s.decide,
    tone: surfaceMeta(process.surfaces.find((x) => x.id === s.on)).tone,
  }));
}
