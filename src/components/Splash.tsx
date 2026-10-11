import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";

/**
 * The startup splash (Splash Animation.dc.html). The trio logo and the name
 * fade up over a soft glow, hold with a shimmering bar while the session
 * restores, then the three logo panes fly flat into the first three grid
 * cells, the other cells trace in, and the name flies up into the title bar.
 *
 * The real app is laid out underneath the whole time with its content hidden
 * (data-splash on .app, splash.css). The logo panes land on the measured rects
 * of the real cells, and each cell is revealed as its pane lands, so the
 * handoff ends on the live app rather than on a picture of one.
 */

type Ease = (t: number) => number;

const easeOutExpo: Ease = (t) => (t >= 1 ? 1 : 1 - 2 ** (-10 * t));
const easeInOutCubic: Ease = (t) => (t < 0.5 ? 4 * t ** 3 : 1 - (-2 * t + 2) ** 3 / 2);
const easeInOutExpo: Ease = (t) =>
  t <= 0 ? 0 : t >= 1 ? 1 : t < 0.5 ? 2 ** (20 * t - 10) / 2 : (2 - 2 ** (-20 * t + 10)) / 2;

/** Eased progress through [start, end] at time t: 0 before, 1 after. */
function tween(t: number, start: number, end: number, ease: Ease): number {
  if (t <= start) return 0;
  if (t >= end) return 1;
  return ease((t - start) / (end - start));
}

// The canvas's three motion curves.
const enter = (t: number, start: number, end: number) => tween(t, start, end, easeOutExpo);
const draw = (t: number, start: number, end: number) => tween(t, start, end, easeInOutCubic);
const snap = (t: number, start: number, end: number) => tween(t, start, end, easeInOutExpo);

const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
const clamp = (v: number, min: number, max: number) => Math.max(min, Math.min(max, v));

// Timeline, in seconds. The canvas runs Intro 1 s, Loading 2 s, Handoff 1.5 s
// and App 1.2 s. Only the handoff and what follows always play in full: the
// handoff starts as soon as launch is done and the logo has assembled, so a
// quick launch skips the Loading hold, and a slow one holds there as long as
// it takes.
const LOADING = 1;
/** The logo and the name are up by now; no handoff starts before it. */
const EARLIEST_HANDOFF = 0.9;
/** The bar sweeps once a second while Loading holds. */
const SWEEP = 1;
const HANDOFF_LEN = 1.5;
const APP_LEN = 1.2;
/** When the real chrome and any non-grid view slide in, after the handoff starts. */
const CHROME_IN = 0.8;
/** Gives the restored panes a moment to lay out before the logo flies into them. */
const LAYOUT_SLACK = 0.15;
/** A launch that never finishes still hands over, so the app is never stuck behind the splash. */
const MAX_HOLD = 10;

// The canvas is 1920 × 1080; the logo and type scale with the window.
const DESIGN_W = 1920;
const DESIGN_H = 1080;
const LOGO_W = 230;
const LOGO_H = 350;
/** The logo sits this far above the middle of the canvas. */
const LOGO_LIFT = 90;

const BACKDROP =
  "radial-gradient(1200px 800px at 12% 8%, rgba(96,72,178,.5), transparent 62%), radial-gradient(1000px 760px at 88% 92%, rgba(38,128,160,.42), transparent 62%), radial-gradient(900px 600px at 60% 35%, rgba(160,84,130,.2), transparent 60%), linear-gradient(180deg,#0d1020,#090b14)";
const ACCENT_RGB = "141,220,255";

/** Logo panes, back to front. `cell` is the grid slot the pane becomes. */
const PANES = [
  {
    key: "back",
    cell: 2,
    z: -240,
    y: -26,
    stagger: 0.16,
    rgb: "206,190,255",
    fill: "linear-gradient(160deg,rgba(139,108,255,.66),rgba(139,108,255,.12))",
    glow: "rgba(139,108,255,.55)",
  },
  {
    key: "mid",
    cell: 1,
    z: -120,
    y: -13,
    stagger: 0.08,
    rgb: "150,230,255",
    fill: "linear-gradient(160deg,rgba(63,200,255,.58),rgba(63,200,255,.1))",
    glow: "rgba(63,200,255,.5)",
  },
  {
    key: "front",
    cell: 0,
    z: 0,
    y: 0,
    stagger: 0,
    rgb: "238,250,255",
    fill: "linear-gradient(155deg, rgba(255,255,255,.36) 0%, rgba(255,255,255,.09) 36%, rgba(255,255,255,.03) 62%, rgba(141,220,255,.22) 100%)",
    glow: "rgba(141,220,255,.55)",
  },
] as const;

type LogoPaneSpec = (typeof PANES)[number];

interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

interface Cell {
  el: HTMLElement;
  box: Box;
  radius: number;
  focused: boolean;
}

interface Title {
  box: Box;
  fontSize: number;
  color: [number, number, number, number];
}

interface Layout {
  cells: Cell[];
  /** The gaps between cells, where the grid lines draw. */
  lines: { box: Box; vertical: boolean }[];
  title: Title | null;
}

const NO_LAYOUT: Layout = { cells: [], lines: [], title: null };

function toBox(rect: DOMRect): Box {
  return { left: rect.left, top: rect.top, width: rect.width, height: rect.height };
}

function parseColor(value: string): Title["color"] {
  const parts = value.match(/[\d.]+/g)?.map(Number) ?? [];
  if (!value.startsWith("rgb") || parts.length < 3) return [238, 241, 246, 0.85];
  return [parts[0]!, parts[1]!, parts[2]!, parts[3] ?? 1];
}

/** Where the live app's panes, dividers and title sit right now. */
function measureLayout(): Layout {
  const page = document.querySelector(".view--active .grid__page--active");
  const cells: Cell[] = page
    ? Array.from(page.querySelectorAll<HTMLElement>(":scope > .grid__cell"), (el) => {
        const pane = el.querySelector<HTMLElement>(".pane");
        return {
          el,
          box: toBox(el.getBoundingClientRect()),
          radius: pane ? parseFloat(getComputedStyle(pane).borderTopLeftRadius) || 0 : 14,
          focused: pane?.classList.contains("pane--focused") ?? false,
        };
      })
    : [];
  const lines = page
    ? Array.from(page.querySelectorAll<HTMLElement>(":scope > .grid__divider"), (el) => ({
        box: toBox(el.getBoundingClientRect()),
        vertical: el.classList.contains("grid__divider--col"),
      }))
    : [];
  const titleEl = document.querySelector<HTMLElement>(".chrome__title");
  let title: Title | null = null;
  if (titleEl) {
    const style = getComputedStyle(titleEl);
    title = {
      box: toBox(titleEl.getBoundingClientRect()),
      fontSize: parseFloat(style.fontSize) || 12.5,
      color: parseColor(style.color),
    };
  }
  return { cells, lines, title };
}

/** When a cell after the first three starts tracing in. */
const traceAt = (index: number, handoff: number) => handoff + 0.75 + (index - PANES.length) * 0.09;

/** When the real cell at `index` fades in under the splash. */
function revealAt(index: number, handoff: number, reduced: boolean): number {
  const pane = PANES.find((p) => p.cell === index);
  if (pane) {
    const lands = handoff + 0.25 + pane.stagger;
    return reduced ? lands + 0.2 : lands + 0.8;
  }
  const at = traceAt(index, handoff);
  return reduced ? at : at + 0.35;
}

/** Everything one frame is drawn from. */
interface Frame {
  T: number;
  handoff: number;
  vw: number;
  vh: number;
  layout: Layout;
}

interface Stage extends Frame {
  /** Scale from the 1920 × 1080 canvas to this window. */
  s: number;
  cx: number;
  cy: number;
  lw: number;
  lh: number;
  reduced: boolean;
}

export function Splash({
  booted,
  restoring,
  onReveal,
  onDone,
}: {
  booted: boolean;
  /** Saved panes launch is restoring; 0 skips the "Restoring" line. */
  restoring: number;
  /** The handoff is far enough along for the real chrome to slide in. */
  onReveal: () => void;
  onDone: () => void;
}) {
  const reduced = useMemo(() => window.matchMedia("(prefers-reduced-motion: reduce)").matches, []);
  const [frame, setFrame] = useState<Frame>(() => ({
    T: 0,
    handoff: Infinity,
    vw: window.innerWidth,
    vh: window.innerHeight,
    layout: NO_LAYOUT,
  }));
  const clock = useRef(0);
  const bootedAt = useRef<number | null>(null);
  const callbacks = useRef({ onReveal, onDone });
  useEffect(() => {
    callbacks.current = { onReveal, onDone };
  }, [onReveal, onDone]);

  useEffect(() => {
    if (booted && bootedAt.current === null) bootedAt.current = clock.current;
  }, [booted]);

  useEffect(() => {
    let raf = 0;
    let start: number | null = null;
    let handoff = Infinity;
    let revealed = false;
    const tick = (now: number) => {
      start ??= now;
      const T = (now - start) / 1000;
      clock.current = T;
      if (handoff === Infinity) {
        const due = bootedAt.current === null ? (T >= MAX_HOLD ? T : null) : bootedAt.current + LAYOUT_SLACK;
        if (due !== null) {
          handoff = Math.max(EARLIEST_HANDOFF, due);
        }
      }
      if (T >= handoff + HANDOFF_LEN + APP_LEN) {
        callbacks.current.onDone();
        return;
      }
      // The panes are measured every frame of the handoff, so the logo lands
      // where they are even if the window or the layout moves under it.
      const layout = T >= handoff - 0.1 ? measureLayout() : NO_LAYOUT;
      layout.cells.forEach((cell, index) => {
        if (T >= revealAt(index, handoff, reduced) && cell.el.dataset.splashIn === undefined) {
          cell.el.dataset.splashIn = "";
        }
      });
      if (!revealed && T >= handoff + CHROME_IN) {
        revealed = true;
        callbacks.current.onReveal();
      }
      setFrame({ T, handoff, vw: window.innerWidth, vh: window.innerHeight, layout });
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [reduced]);

  const { T, handoff: H, vw, vh } = frame;
  const s = Math.max(0.5, Math.min(vw / DESIGN_W, vh / DESIGN_H));
  const lh = LOGO_H * s;
  const st: Stage = { ...frame, s, cx: vw / 2, cy: vh / 2 - LOGO_LIFT * s, lw: LOGO_W * s, lh, reduced };

  // The hold breathes: one slow pulse of the floor glow every two sweeps.
  const glowIn = enter(T, 0, 1);
  const holding = T >= LOADING && T < H;
  const pulse = reduced || !holding ? 0 : ((1 - Math.cos(((T - LOADING) / 2) * Math.PI * 2)) / 2) * 0.1;
  const glowOut = 1 - enter(T, H, H + 0.7);
  const { cx, cy } = st;

  return (
    <div
      className="splash"
      aria-hidden="true"
      data-tauri-drag-region={T < H ? "" : undefined}
      style={{ pointerEvents: T < H ? "auto" : "none" }}
    >
      {/* The splash is always dark; over a light theme it fades out to the app's own backdrop. */}
      <div className="splash__layer" style={{ background: BACKDROP, opacity: glowOut }} />
      <div
        className="splash__layer"
        style={{
          left: cx - 520 * s,
          top: cy - 420 * s,
          width: 1040 * s,
          height: 840 * s,
          borderRadius: "50%",
          background: "radial-gradient(closest-side, rgba(110,90,220,.22), rgba(63,200,255,.08) 55%, transparent 100%)",
          opacity: glowIn * glowOut,
        }}
      />
      <div
        className="splash__layer"
        style={{
          left: cx - 300 * s,
          top: cy + lh / 2 - 30 * s,
          width: 600 * s,
          height: 120 * s,
          borderRadius: "50%",
          background: `radial-gradient(closest-side, rgba(${ACCENT_RGB},.55), transparent)`,
          filter: `blur(${18 * s}px)`,
          opacity: (glowIn + pulse) * glowOut,
        }}
      />
      <GridLines st={st} />
      {st.layout.cells.slice(PANES.length).map((cell, offset) => (
        <TracedCell key={offset} st={st} cell={cell} index={offset + PANES.length} />
      ))}
      {PANES.map((pane, order) => (
        <LogoPane key={pane.key} st={st} pane={pane} order={order} />
      ))}
      <Wordmark st={st} />
      <LoadBar st={st} />
      <Status st={st} restoring={restoring} />
    </div>
  );
}

function LogoPane({ st, pane, order }: { st: Stage; pane: LogoPaneSpec; order: number }) {
  const { T, handoff: H, s, reduced } = st;
  const cell = st.layout.cells[pane.cell];
  const start = 0.1 + order * 0.1;
  const fadeIn = enter(T, start, start + 0.7);
  const rise = reduced ? 0 : (1 - fadeIn) * 14 * s;

  // A pane with a cell to land in flies there; one without (fewer panes than
  // the logo has, the notes tab, reduced motion) fades out where it stands.
  const flies = Boolean(cell) && !reduced;
  const lift = H + 0.25 + pane.stagger;
  const lands = lift + 0.8;
  const q = flies ? snap(T, lift, lands) : 0;
  const asPane = flies ? enter(T, lift + 0.55, lift + 1.0) : 0;
  const handedOver = flies ? enter(T, lands + 0.15, lands + 0.6) : 0;
  const flash = flies && T >= lands ? 1 - enter(T, lands, lands + 0.7) : 0;
  const logoOpacity = flies ? 1 - asPane : 1 - enter(T, H, H + 0.5);

  const k = 1 - q;
  const logo: Box = { left: st.cx - st.lw / 2, top: st.cy - st.lh / 2, width: st.lw, height: st.lh };
  const to = cell?.box ?? logo;
  const box: Box = {
    left: lerp(logo.left, to.left, q),
    top: lerp(logo.top, to.top, q),
    width: lerp(logo.width, to.width, q),
    height: lerp(logo.height, to.height, q),
  };
  const radius = lerp(34 * s, cell?.radius ?? 14, q);
  const focused = cell?.focused ?? false;
  const flashRgb = focused ? ACCENT_RGB : pane.rgb;
  const transform =
    `translate(${-27 * s * k}px,${11 * s * k}px) perspective(${1000 * s}px) ` +
    `rotateY(${-38 * k}deg) rotateX(${7 * k}deg) translate3d(0,${(pane.y * s + rise) * k}px,${pane.z * s * k}px)`;
  const edge = Math.max(2, 3 * s);

  return (
    <div className="splash__pane" style={{ ...box, borderRadius: radius, transform, opacity: fadeIn }}>
      {logoOpacity > 0 && (
        <div
          className="splash__fill"
          style={{
            borderRadius: radius,
            background: pane.fill,
            border: `${edge}px solid rgba(${pane.rgb},.75)`,
            boxShadow:
              `0 0 ${70 * s}px ${pane.glow}, inset 0 ${4 * s}px 0 rgba(255,255,255,.35)` +
              (pane.key === "front" ? `, inset ${6 * s}px 0 0 rgba(235,250,255,.9)` : ""),
            opacity: logoOpacity,
          }}
        />
      )}
      {asPane * (1 - handedOver) > 0 && (
        // A bare pane in the real one's frame; the real pane fades in under it.
        <div
          className="splash__fill"
          style={{
            borderRadius: radius,
            background: "rgba(8,10,18,.62)",
            border: focused ? "2px solid #8ddcff" : "1px solid rgba(255,255,255,.13)",
            boxShadow: (focused ? `0 0 0 4px rgba(${ACCENT_RGB},.28), ` : "") + "0 20px 50px rgba(0,0,0,.42)",
            opacity: asPane * (1 - handedOver),
          }}
        />
      )}
      {flash > 0 && (
        <div
          className="splash__fill"
          style={{ borderRadius: radius, boxShadow: `0 0 ${50 * flash}px rgba(${flashRgb},${0.7 * flash})` }}
        />
      )}
    </div>
  );
}

/** An edge light that draws itself round a rounded rect, a bright comet at its head. */
function Trace({ deg, radius, opacity }: { deg: number; radius: number; opacity: number }) {
  if (opacity <= 0.001 || deg <= 0.5) return null;
  const rgb = "160,225,255";
  const alpha = 0.85;
  const from = 215;
  const color = `rgba(${rgb},${alpha})`;
  const tailAlpha = alpha * lerp(0.2, 1, clamp((deg - 290) / 70, 0, 1));
  const trail = `conic-gradient(from ${from}deg, rgba(${rgb},${tailAlpha}) 0deg, ${color} ${deg}deg, transparent ${deg}deg 360deg)`;
  const head =
    deg < 359.5
      ? `conic-gradient(from ${from}deg, transparent 0deg ${Math.max(0, deg - 46)}deg, rgba(255,255,255,.98) ${deg}deg, transparent ${deg + 0.01}deg 360deg), `
      : "";
  const mask = "linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0)";
  return (
    <div className="splash__fill" style={{ borderRadius: radius, filter: `drop-shadow(0 0 10px ${color})`, opacity }}>
      <div
        className="splash__fill"
        style={{
          borderRadius: radius,
          padding: 1.5,
          background: head + trail,
          WebkitMask: mask,
          WebkitMaskComposite: "xor",
          mask,
          maskComposite: "exclude",
        }}
      />
    </div>
  );
}

/** A cell past the first three: its edge traces in while the real pane fades up inside. */
function TracedCell({ st, cell, index }: { st: Stage; cell: Cell; index: number }) {
  if (st.reduced) return null;
  const at = traceAt(index, st.handoff);
  const deg = draw(st.T, at, at + 0.55) * 360;
  const opacity = 1 - enter(st.T, at + 0.55, at + 1.0);
  return (
    <div className="splash__pane" style={cell.box}>
      <Trace deg={deg} radius={Math.max(cell.radius, 4)} opacity={opacity} />
    </div>
  );
}

/** Light runs along the gaps between cells as the handoff starts. */
function GridLines({ st }: { st: Stage }) {
  const { T, handoff: H } = st;
  if (st.reduced) return null;
  const d = draw(T, H, H + 0.55);
  const out = 1 - enter(T, H + 1.0, H + 1.5);
  if (d <= 0 || out <= 0) return null;
  return (
    <>
      {st.layout.lines.map(({ box, vertical }, index) => {
        const style: CSSProperties = vertical
          ? { left: box.left + box.width / 2 - 0.5, top: box.top, width: 1, height: box.height, transform: `scaleY(${d})` }
          : { left: box.left, top: box.top + box.height / 2 - 0.5, width: box.width, height: 1, transform: `scaleX(${d})` };
        return <div key={index} className="splash__line" style={{ ...style, opacity: out }} />;
      })}
    </>
  );
}

/** The name under the logo, which flies up to become the title bar's. */
function Wordmark({ st }: { st: Stage }) {
  const { T, handoff: H, s } = st;
  const title = st.layout.title;
  const size = 34 * s;
  const shown = enter(T, 0.35, 1.0);
  // Reduced motion swaps it over with fades rather than flying it.
  const q = !title ? 0 : st.reduced ? (T >= H + 0.6 ? 1 : 0) : snap(T, H + 0.15, H + 0.95);
  const away = !title
    ? 1 - enter(T, H + 0.15, H + 0.6)
    : st.reduced
      ? T < H + 0.6
        ? 1 - enter(T, H, H + 0.5)
        : enter(T, H + CHROME_IN, H + 1.3)
      : 1;
  const [r, g, b, a] = title?.color ?? [243, 246, 251, 1];
  const scale = lerp(1, (title?.fontSize ?? size) / size, q);
  const centerY = lerp(st.cy + st.lh / 2 + 76 * s + size / 2, title ? title.box.top + title.box.height / 2 : 0, q);
  const left = lerp(st.cx, title?.box.left ?? 0, q);
  const color = `rgba(${lerp(243, r, q)},${lerp(246, g, q)},${lerp(251, b, q)},${lerp(1, a, q)})`;
  return (
    <div
      className="splash__wordmark"
      style={{
        left,
        top: centerY - size / 2,
        height: size,
        fontSize: size,
        lineHeight: `${size}px`,
        letterSpacing: `${lerp(-0.01, 0.04, q)}em`,
        color,
        opacity: shown * away,
        transform: `translateX(${lerp(-50, 0, q)}%) scale(${scale})`,
      }}
    >
      Augry
    </div>
  );
}

function LoadBar({ st }: { st: Stage }) {
  const { T, handoff: H, s, reduced } = st;
  const shown = enter(T, 0.6, 1.0) * (1 - enter(T, H + 0.25, H + 0.5));
  if (shown <= 0) return null;
  const pass = T >= LOADING && T < H ? ((T - LOADING) % SWEEP) / SWEEP : 0;
  const fill = draw(T, H - 0.05, H + 0.25);
  const width = 34 + 66 * fill;
  const left = (reduced ? 33 : -34 + pass * 134) * (1 - fill);
  return (
    <div
      className="splash__bar"
      style={{ left: st.cx - 110 * s, top: st.cy + st.lh / 2 + 136 * s, width: 220 * s, opacity: shown }}
    >
      <div className="splash__bar-fill" style={{ left: `${left}%`, width: `${width}%`, opacity: reduced ? 0.7 : 1 }} />
    </div>
  );
}

function Status({ st, restoring }: { st: Stage; restoring: number }) {
  const { T, handoff: H, s } = st;
  const items = [{ at: LOADING, text: "Starting shells…" }];
  if (restoring > 0) {
    items.push({ at: LOADING + 1.2, text: `Restoring ${restoring} pane${restoring === 1 ? "" : "s"}…` });
  }
  const current = items.filter((item) => T >= item.at).length - 1;
  if (current < 0) return null;
  const item = items[current]!;
  // A quick launch hands over before the hold, and its first line, begin.
  if (item.at >= H) return null;
  const next = items[current + 1];
  const opacity =
    enter(T, item.at, item.at + 0.25) *
    (next ? 1 - enter(T, next.at - 0.2, next.at) : 1 - enter(T, H, H + 0.3));
  if (opacity <= 0) return null;
  return (
    <div
      className="splash__status"
      style={{ top: st.cy + st.lh / 2 + 154 * s, fontSize: Math.max(12, 15 * s), opacity }}
    >
      {item.text}
    </div>
  );
}
