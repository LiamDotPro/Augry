import { useEffect } from "react";
import { cx } from "../../lib/cx";
import { ProcessIcon } from "./ProcessIcon";
import { useProcess } from "./store";

/** How long the notice stays when nobody touches it. */
const NOTICE_MS = 12000;

/**
 * "Crash report started" (board 04, "folder-run"): a watched folder got a
 * file, so its process started by itself.
 */
export function FolderNotice() {
  const { state, dispatch } = useProcess();
  const notice = state.notice!;

  useEffect(() => {
    const timer = window.setTimeout(() => dispatch({ type: "dismiss-notice" }), NOTICE_MS);
    return () => window.clearTimeout(timer);
  }, [notice.id, dispatch]);

  return (
    <div className={cx("process-notice", state.route.view === "home" && "process-notice--home")} role="status">
      <div className="process-notice__icon">
        <ProcessIcon name="folder" size={18} />
      </div>
      <div className="process-notice__body">
        <div className="process-notice__title">{notice.title}</div>
        <div className="process-notice__text">
          <code>{notice.file}</code> arrived in {notice.folder}
        </div>
        <div className="process-notice__buttons">
          <button
            type="button"
            className="process-btn process-btn--small process-btn--primary"
            onClick={() => {
              dispatch({ type: "navigate", route: { view: "run", runId: notice.runId } });
              dispatch({ type: "dismiss-notice" });
            }}
          >
            Open run
          </button>
          <button
            type="button"
            className="process-btn process-btn--small process-btn--danger"
            onClick={() => {
              dispatch({ type: "stop", runId: notice.runId, now: Date.now() });
              dispatch({ type: "dismiss-notice" });
            }}
          >
            Stop
          </button>
        </div>
      </div>
      <button type="button" className="process-x process-notice__close" aria-label="Dismiss" onClick={() => dispatch({ type: "dismiss-notice" })}>
        ×
      </button>
    </div>
  );
}
