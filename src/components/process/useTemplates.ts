import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../../cloud/api";
import { processFromV1 } from "../../process/v1";
import type { Process } from "../../process/types";
import type { Template } from "../../cloud/types";

/** A server template, with its definition drawn as a process (null when it can't be). */
export interface LoadedTemplate extends Template {
  process: Process | null;
}

export type TemplatesState =
  | { status: "loading" }
  | { status: "ready"; templates: LoadedTemplate[] }
  /** Couldn't reach the server, or it answered with an error. */
  | { status: "failed"; offline: boolean };

export interface TemplatesApi {
  state: TemplatesState;
  retry(): void;
}

/**
 * The templates the server offers (GET /v1/templates), each with its
 * definition so the card can draw its lanes. They need no account. Loaded
 * once while the Process tab is mounted; Retry after a failure loads again.
 */
export function useTemplates(): TemplatesApi {
  const [state, setState] = useState<TemplatesState>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);
  const live = useRef(true);

  useEffect(() => {
    live.current = true;
    setState({ status: "loading" });
    void (async () => {
      try {
        const list = await api.listTemplates();
        const templates = await Promise.all(
          list.map(async (template): Promise<LoadedTemplate> => {
            try {
              const full = await api.getTemplate(template.slug);
              const process = processFromV1(full.definition, { id: template.slug, from: `template:${template.slug}@${template.version}` });
              return { ...template, process };
            } catch {
              // One template that won't load still leaves the others.
              return { ...template, process: null };
            }
          }),
        );
        if (live.current) setState({ status: "ready", templates });
      } catch (error) {
        if (live.current) setState({ status: "failed", offline: api.isCloudError(error) && error.offline });
      }
    })();
    return () => {
      live.current = false;
    };
  }, [attempt]);

  const retry = useCallback(() => setAttempt((n) => n + 1), []);
  return { state, retry };
}
