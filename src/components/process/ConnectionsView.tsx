import { useState } from "react";
import { CONNECTION_COLORS, CONNECTION_GROUPS } from "../../process/connections";
import type { Connection } from "../../process/types";
import { cx } from "../../lib/cx";
import { ProcessIcon } from "./ProcessIcon";
import { useProcess } from "./store";
import { TeamConnectionsView } from "./TeamConnections";
import { useKeptScroll } from "./useKeptScroll";

export const GROUP_NAME = { work: "Project management", local: "On this computer", systems: "Systems and data" };

/**
 * Connections (board 05): what processes start from and what their steps
 * can reach, grouped by kind. Process files name a connection and never hold
 * its secrets. None can be set up yet: the cards show what is coming and say
 * so. With a team selected, this shows the team's connections instead
 * (board 11).
 */
export function ConnectionsView() {
  const { team } = useProcess();
  return team ? <TeamConnectionsView /> : <PersonalConnections />;
}

/** A card that can't be set up yet: its status, and no button. */
export const NOT_YET: CardView = { on: false, status: "Not set up", used: "", action: null };

/** Your own connections, as board 05 draws them. */
function PersonalConnections() {
  const { state, dispatch } = useProcess();
  const [query, setQuery] = useState("");
  const scroll = useKeptScroll<HTMLDivElement>("connections");
  const q = query.trim().toLowerCase();

  return (
    <div className="process-connections">
      <div className="process-home__main process-connections__main" ref={scroll.ref} onScroll={scroll.onScroll}>
        <div className="process-home__head process-connections__head">
          <div className="process-home__titles">
            <button type="button" className="process-backlink" onClick={() => dispatch({ type: "navigate", route: { view: "home" } })}>
              <span aria-hidden="true">‹</span>Processes
            </button>
            <h1 className="process-home__title">Connections</h1>
            <p className="process-home__sub">Set up once, use in any process. Secrets stay in your system’s credential store, never in process files.</p>
          </div>
          <input
            className="process-input process-connections__search"
            type="search"
            placeholder="Search connections"
            aria-label="Search connections"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <ComingNote />
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
                  <ConnectionCard key={c.id} connection={c} picked={false} onOpen={() => {}} view={NOT_YET} />
                ))}
              </div>
            </section>
          );
        })}
      </div>
    </div>
  );
}

/** Says plainly that connections can't be set up yet. */
export function ComingNote() {
  return (
    <div className="process-coming" role="note">
      <ProcessIcon name="plug" size={16} />
      <span>
        Connections can’t be set up yet. These are the kinds processes will be able to start from and use; until then
        processes start by hand.
      </span>
    </div>
  );
}

export function Tile({ connection, size = 34 }: { connection: Connection; size?: number }) {
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

/** What a team's Connections changes on a card: its status, the line under it and the button. */
export interface CardView {
  on: boolean;
  status: string;
  used: string;
  /** The button, or null for none. */
  action: string | null;
}

export function ConnectionCard({ connection, picked, onOpen, view }: { connection: Connection; picked: boolean; onOpen(): void; view?: CardView }) {
  const c = view ? { ...connection, on: view.on, status: view.status, used: view.used } : connection;
  const action = view ? view.action : c.on ? "Manage" : "Set up";
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
        {action && (
          <button type="button" className={cx("process-btn process-btn--small", !c.on && "process-btn--primary")} onClick={onOpen} aria-pressed={picked}>
            {action}
          </button>
        )}
      </div>
    </div>
  );
}
