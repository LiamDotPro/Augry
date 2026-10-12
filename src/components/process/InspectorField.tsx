import { useCallback, useId, useRef, useState } from "react";
import { cx } from "../../lib/cx";
import { ProcessIcon } from "./ProcessIcon";
import { useDismiss } from "./useDismiss";

/**
 * A side panel field (Components.dc.html use="field"). The type picks the
 * control; every one is live and reports changes through its callbacks.
 */
export type Field =
  | { type: "segmented"; label: string; options: string[]; value: string; onChange(value: string): void; hint?: string; readOnly?: boolean }
  | { type: "agents"; label: string; options: string[]; value: string; onChange(value: string): void; hint?: string }
  | {
      type: "select";
      label: string;
      icon: string;
      value: string;
      detail?: string;
      tone?: "ok" | "muted";
      options?: { value: string; detail?: string }[];
      onChange?(value: string): void;
      hint?: string;
    }
  | {
      type: "text";
      label: string;
      value: string;
      onChange(value: string): void;
      mono?: boolean;
      multiline?: boolean;
      secret?: boolean;
      placeholder?: string;
      hint?: string;
      /** Shown but not changeable (a team connection only owners and admins change). */
      readOnly?: boolean;
    }
  | { type: "checks"; label: string; items: { label: string; detail?: string; on: boolean; onChange(on: boolean): void }[]; hint?: string }
  | { type: "switches"; label: string; items: { label: string; on: boolean; onChange(on: boolean): void }[]; hint?: string }
  | { type: "table"; label: string; rows: { k: string; v: string; mono?: boolean; detail?: string }[]; hint?: string }
  | {
      type: "steps";
      label: string;
      items: { id: string; title: string; who: string; family: "agent" | "human" | "auto"; agent?: string; onClick(): void }[];
      hint?: string;
    };

/** The agent tiles: brand mark and its tile wash. */
const AGENT_TILES: Record<string, { icon: string; tile: string; size: number }> = {
  "Claude Code": { icon: "claude", tile: "process-agent__tile--claude", size: 20 },
  Codex: { icon: "openai", tile: "process-agent__tile--codex", size: 19 },
  Custom: { icon: "term", tile: "process-agent__tile--custom", size: 18 },
};

/** `{{ticket.title}}` references in a task, shown as tokens under the box. */
function references(text: string): string[] {
  return [...new Set([...text.matchAll(/\{\{\s*([\w.]+)\s*\}\}/g)].map((m) => m[1]!))];
}

export function InspectorField({ field }: { field: Field }) {
  const id = useId();
  const labelled = field.type === "text" || field.type === "select";
  return (
    <div className="process-field">
      {labelled ? (
        <label className="process-label" htmlFor={id}>
          {field.label}
        </label>
      ) : (
        <div className="process-label" id={id}>
          {field.label}
        </div>
      )}
      <FieldControl field={field} id={id} />
      {field.hint && <div className="process-field__hint">{field.hint}</div>}
    </div>
  );
}

function FieldControl({ field, id }: { field: Field; id: string }) {
  switch (field.type) {
    case "segmented":
      return (
        <div className="process-seg process-seg--field" role="radiogroup" aria-labelledby={id}>
          {field.options.map((o) => (
            <button
              key={o}
              type="button"
              role="radio"
              aria-checked={o === field.value}
              className={cx("process-seg__item", o === field.value && "process-seg__item--on")}
              disabled={field.readOnly}
              onClick={() => field.onChange(o)}
            >
              {o}
            </button>
          ))}
        </div>
      );

    case "agents":
      return (
        <div className="process-agents" role="radiogroup" aria-labelledby={id}>
          {field.options.map((o) => {
            const tile = AGENT_TILES[o] ?? AGENT_TILES.Custom!;
            const on = o === field.value;
            return (
              <button key={o} type="button" role="radio" aria-checked={on} title={o} className={cx("process-agent", on && "process-agent--on")} onClick={() => field.onChange(o)}>
                <span className={cx("process-agent__tile", tile.tile)}>
                  <ProcessIcon name={tile.icon} size={tile.size} stroke={1.8} />
                </span>
                <span className="process-agent__label">{o}</span>
                {on && (
                  <span className="process-agent__check" aria-hidden="true">
                    ✓
                  </span>
                )}
              </button>
            );
          })}
        </div>
      );

    case "select":
      return <SelectControl field={field} id={id} />;

    case "text": {
      const refs = field.mono ? [] : references(field.value);
      return (
        <>
          {field.multiline ? (
            <textarea
              id={id}
              className={cx("process-input", "process-text", field.mono && "process-input--mono")}
              value={field.value}
              placeholder={field.placeholder}
              spellCheck={!field.mono}
              readOnly={field.readOnly}
              onChange={(e) => field.onChange(e.target.value)}
            />
          ) : (
            <input
              id={id}
              className={cx("process-input", "process-input--field", field.mono && "process-input--mono")}
              type={field.secret ? "password" : "text"}
              value={field.value}
              placeholder={field.placeholder}
              spellCheck={false}
              autoComplete="off"
              readOnly={field.readOnly}
              onChange={(e) => field.onChange(e.target.value)}
            />
          )}
          {refs.length > 0 && (
            <div className="process-field__refs">
              Uses
              {refs.map((r) => (
                <span key={r} className="process-token">
                  {r}
                </span>
              ))}
            </div>
          )}
        </>
      );
    }

    case "checks":
      return (
        <div className="process-list" role="group" aria-labelledby={id}>
          {field.items.map((it) => (
            <button key={it.label} type="button" role="checkbox" aria-checked={it.on} className="process-list__row process-list__row--check" onClick={() => it.onChange(!it.on)}>
              <span className={cx("process-check", it.on && "process-check--on")} aria-hidden="true">
                {it.on ? "✓" : ""}
              </span>
              <span className="process-list__body">
                <span className="process-list__title">{it.label}</span>
                {it.detail && <span className="process-list__detail">{it.detail}</span>}
              </span>
            </button>
          ))}
        </div>
      );

    case "switches":
      return (
        <div className="process-list" role="group" aria-labelledby={id}>
          {field.items.map((it) => (
            <button key={it.label} type="button" role="switch" aria-checked={it.on} className="process-list__row" onClick={() => it.onChange(!it.on)}>
              <span className="process-list__label">{it.label}</span>
              <span className={cx("process-switch", it.on && "process-switch--on")} aria-hidden="true">
                <span className="process-switch__knob" />
              </span>
            </button>
          ))}
        </div>
      );

    case "table":
      return (
        <div className="process-list" role="table" aria-labelledby={id}>
          {field.rows.map((r) => (
            <div key={r.k} className="process-list__row process-list__row--table" role="row">
              <span className="process-list__key" role="cell">
                {r.k}
              </span>
              <span className="process-list__value" role="cell">
                <span className={cx(r.mono && "process-mono")}>{r.v}</span>
                {r.detail && <span className="process-list__more">{r.detail}</span>}
              </span>
            </div>
          ))}
        </div>
      );

    case "steps":
      return (
        <div className="process-list" role="list" aria-labelledby={id}>
          {field.items.map((it, i) => (
            <button key={it.id} type="button" role="listitem" className="process-list__row process-list__row--step" onClick={it.onClick}>
              <span className="process-list__n">{i + 1}</span>
              <span className={cx("process-who-dot", `process-who-dot--${it.family}`)} />
              <span className="process-list__label">{it.title}</span>
              <span className={cx("process-who", `process-who--${it.family}`, it.agent && `process-who--${it.agent}`)}>{it.who}</span>
            </button>
          ))}
        </div>
      );
  }
}

function SelectControl({ field, id }: { field: Extract<Field, { type: "select" }>; id: string }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(open, root, close);
  const choices = field.options && field.onChange ? field.options : null;
  return (
    <div className="process-menu process-menu--block" ref={root}>
      <button
        id={id}
        type="button"
        className={cx("process-select", "process-select--field", field.tone && `process-select--${field.tone}`)}
        aria-haspopup={choices ? "listbox" : undefined}
        aria-expanded={choices ? open : undefined}
        onClick={() => choices && setOpen((v) => !v)}
      >
        <ProcessIcon name={field.icon} size={16} stroke={1.8} className="process-select__icon" />
        <span className="process-select__value">{field.value}</span>
        <span className="process-select__detail">{field.detail}</span>
        <span className="process-select__caret" aria-hidden="true">▼</span>
      </button>
      {open && choices && (
        <div className="process-menu__list process-menu__list--field" role="listbox">
          {choices.map((o) => (
            <button
              key={o.value}
              type="button"
              role="option"
              aria-selected={o.value === field.value}
              className={cx("process-menu__item", o.value === field.value && "process-menu__item--on")}
              onClick={() => {
                field.onChange?.(o.value);
                setOpen(false);
              }}
            >
              {o.value}
              {o.detail && <span className="process-menu__meta">{o.detail}</span>}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
