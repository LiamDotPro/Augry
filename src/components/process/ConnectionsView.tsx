import { useState } from "react";
import { CONNECTION_COLORS, CONNECTION_GROUPS, connectionFields } from "../../process/connections";
import type { Connection } from "../../process/types";
import { cx } from "../../lib/cx";
import { InspectorField, type Field } from "./InspectorField";
import { ProcessIcon } from "./ProcessIcon";
import { useProcess } from "./store";
import { useKeptScroll } from "./useKeptScroll";

const GROUP_NAME = { work: "Project management", local: "On this computer", systems: "Systems and data" };

/**
 * Connections (board 05): what processes start from and what their steps
 * can reach, grouped by kind. Set one up once and every process can use it;
 * process files name a connection and never hold its secrets.
 */
export function ConnectionsView() {
  const { state, dispatch } = useProcess();
  const [query, setQuery] = useState("");
  const scroll = useKeptScroll<HTMLDivElement>("connections");
  const q = query.trim().toLowerCase();
  const open = state.connections.find((c) => c.id === state.connection) ?? null;
  const pick = (id: string | null) => dispatch({ type: "connection-open", id });

  return (
    <div className={cx("process-connections", open && "process-connections--panel")}>
      <div className="process-home__main process-connections__main" ref={scroll.ref} onScroll={scroll.onScroll}>
        <div className="process-home__head process-connections__head">
          <div className="process-home__titles">
            <button type="button" className="process-backlink" onClick={() => dispatch({ type: "navigate", route: { view: "home" } })}>
              <span aria-hidden="true">‹</span>Processes
            </button>
            <h1 className="process-home__title">Connections</h1>
            <p className="process-home__sub">Set up once, use in any process. Secrets stay in Windows Credential Manager, never in process files.</p>
          </div>
          <input
            className="process-input process-connections__search"
            type="search"
            placeholder="Search connections"
            aria-label="Search connections"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <button
            type="button"
            className="process-btn"
            onClick={() => pick(state.connections.find((c) => !c.on)?.id ?? state.connections[0]!.id)}
          >
            <span className="process-btn__plus" aria-hidden="true">+</span>
            Add connection
          </button>
        </div>
        {CONNECTION_GROUPS.map((group) => {
          const items = state.connections.filter((c) => c.group === group.id && (!q || `${c.name} ${c.desc}`.toLowerCase().includes(q)));
          if (!items.length) return null;
          return (
            <section key={group.id} className="process-connections__group">
              <div className="process-home__section">
                <h2 className="process-home__h2">{group.title}</h2>
                <span className="process-home__hint">{group.hint}</span>
              </div>
              <div className="process-connections__grid">
                {items.map((c) => (
                  <ConnectionCard key={c.id} connection={c} picked={c.id === state.connection} onOpen={() => pick(c.id)} />
                ))}
              </div>
            </section>
          );
        })}
      </div>
      {open && <SetupPanel connection={open} onClose={() => pick(null)} />}
    </div>
  );
}

function Tile({ connection, size = 34 }: { connection: Connection; size?: number }) {
  const color = CONNECTION_COLORS[connection.icon];
  return (
    <span
      className={cx("process-conn__tile", !color && "process-tone--start")}
      style={color && color !== "currentColor" ? { color, background: `color-mix(in srgb, ${color} 14%, transparent)`, width: size, height: size } : { width: size, height: size }}
    >
      <ProcessIcon name={connection.icon} size={size > 34 ? 20 : 18} />
    </span>
  );
}

function ConnectionCard({ connection: c, picked, onOpen }: { connection: Connection; picked: boolean; onOpen(): void }) {
  return (
    <div className={cx("process-conn", c.on ? "process-conn--on" : "process-conn--off", picked && "process-conn--picked")}>
      <div className="process-conn__top">
        <Tile connection={c} />
        <div className="process-conn__titles">
          <div className="process-conn__name">{c.name}</div>
          <div className="process-conn__status">
            <span className={cx("process-dot", "process-dot--small", c.on ? "process-dot--ok" : "process-dot--muted")} />
            <span className="process-conn__statustext">{c.status}</span>
          </div>
        </div>
      </div>
      <div className="process-conn__desc">{c.desc}</div>
      <div className="process-conn__foot">
        <span className="process-conn__used">{c.on ? c.used : ""}</span>
        <button type="button" className={cx("process-btn process-btn--small", !c.on && "process-btn--primary")} onClick={onOpen} aria-pressed={picked}>
          {c.on ? "Manage" : "Set up"}
        </button>
      </div>
    </div>
  );
}

function SetupPanel({ connection: c, onClose }: { connection: Connection; onClose(): void }) {
  const { dispatch } = useProcess();
  // Secrets are typed here but never kept in the settings (or a process file).
  const [secrets, setSecrets] = useState<Record<string, string>>({});
  const [testing, setTesting] = useState(false);
  const set = (key: string, value: string) => dispatch({ type: "connection-set", id: c.id, key, value });

  const fields: Field[] = connectionFields(c)
    .filter((f) => f.type !== "switch")
    .map((f): Field => {
      if (f.type === "segmented") {
        return { type: "segmented", label: f.label, options: f.options!, value: c.settings[f.key] ?? f.options![0]!, onChange: (v) => set(f.key, v), hint: f.hint };
      }
      if (f.type === "secret") {
        const stored = c.on && !secrets[f.key];
        return {
          type: "text",
          label: f.label,
          mono: f.mono,
          secret: true,
          value: secrets[f.key] ?? (stored ? "••••••••••••" : ""),
          onChange: (v) => setSecrets((s) => ({ ...s, [f.key]: v.replace(/•/g, "") })),
          hint: f.hint,
        };
      }
      return { type: "text", label: f.label, mono: f.mono, multiline: f.key === "folders" || f.key === "repos", value: c.settings[f.key] ?? "", onChange: (v) => set(f.key, v), hint: f.hint };
    });
  const switches = connectionFields(c).filter((f) => f.type === "switch");
  if (switches.length) {
    fields.push({
      type: "switches",
      label: c.group === "systems" ? "Also allow" : "Options",
      items: switches.map((f) => ({ label: f.label, on: c.settings[f.key] === "on", onChange: (on) => set(f.key, on ? "on" : "off") })),
    });
  }

  const test = () => {
    setTesting(true);
    // A short wait so the result reads as a check rather than a label.
    window.setTimeout(() => {
      dispatch({ type: "connection-test", id: c.id });
      setTesting(false);
    }, 450);
  };

  return (
    <aside className="process-panel process-panel--wide" aria-label={`${c.name} settings`}>
      <div className="process-panel__head">
        <Tile connection={c} size={36} />
        <div className="process-panel__titles">
          <div className="process-panel__title">{c.name}</div>
          <div className="process-panel__sub">
            {GROUP_NAME[c.group]}
            {c.group !== "work" ? " · on this computer" : ""}
          </div>
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
      <div className="process-panel__foot">
        {(testing || c.test) && (
          <div className={cx("process-panel__result", c.test?.ok ? "process-panel__result--ok" : c.test && !testing ? "process-panel__result--bad" : undefined)} role="status">
            <span className={cx("process-dot", testing ? "process-dot--accent" : c.test?.ok ? "process-dot--ok" : "process-dot--danger")} />
            {testing ? "Testing…" : c.test!.text}
          </div>
        )}
        <div className="process-panel__buttons">
          <button type="button" className="process-btn process-btn--grow" onClick={test} disabled={testing}>
            Test connection
          </button>
          {c.on ? (
            <button type="button" className="process-btn process-btn--danger" onClick={() => dispatch({ type: "connection-remove", id: c.id })}>
              Remove
            </button>
          ) : (
            <button type="button" className="process-btn process-btn--primary" onClick={() => dispatch({ type: "connection-save", id: c.id })}>
              Save
            </button>
          )}
        </div>
      </div>
    </aside>
  );
}
