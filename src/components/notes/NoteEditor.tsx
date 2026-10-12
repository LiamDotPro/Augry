import { useMemo, useState, type KeyboardEvent } from "react";
import { cx } from "../../lib/cx";
import { classifyLines, type LineKind } from "../../lib/markdown";
import type { LocalConflict } from "../../notesSync/engine";
import { useNotesSync } from "../../notesSync/NotesSyncProvider";
import { conflictLead, savedStatus } from "../../notesSync/status";
import { useAppActions, useAppState } from "../../state/AppProvider";
import type { SaveState } from "../../state/reducer";

const STATUS_TEXT: Record<SaveState, string> = {
  clean: "Up to date",
  dirty: "Unsaved · saving…",
  saving: "Saving…",
  saved: "Saved",
  error: "Save failed",
};

/**
 * Markdown editor. A transparent textarea sits exactly on top of a highlighted
 * copy of the same text, so the line numbers stay aligned even when a long
 * paragraph wraps, and every real editing behaviour still comes from the
 * textarea itself.
 */
export function NoteEditor() {
  const { notes } = useAppState();
  const actions = useAppActions();
  const sync = useNotesSync();

  const lines = useMemo(() => notes.content.split("\n"), [notes.content]);
  const kinds = useMemo(() => classifyLines(notes.content), [notes.content]);

  if (!notes.openPath) {
    return (
      <div className="notes__panel">
        <div className="panel-head">Editor</div>
        <p className="editor__placeholder">
          Pick a note on the left, or create one with + File.
        </p>
      </div>
    );
  }

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Tab") {
      event.preventDefault();
      insertAtCursor(event.currentTarget, "  ", actions.editNote);
      return;
    }
    if (event.key === "s" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void actions.saveNote();
    }
  };

  // While sync is on: the team's name before a team note's path, and whether it reached the server.
  const where = sync.active ? sync.rootOf(notes.openPath) : null;
  const syncConflict = sync.active ? sync.conflictFor(notes.openPath) : null;
  const settled = notes.saveState === "clean" || notes.saveState === "saved";
  const synced =
    where && settled && (where.root.kind === "team" || sync.personal)
      ? savedStatus({
          conflict: syncConflict !== null,
          refused: sync.snapshot.refused.includes(notes.openPath),
          offline: sync.offline,
          waiting: sync.snapshot.waiting.includes(notes.openPath),
        })
      : null;

  return (
    <div className="notes__panel">
      <div className="panel-head">
        <span className="editor__path ellipsis" title={notes.openPath}>
          {where?.root.kind === "team" ? (
            <>
              <span className="notes-sync-path__team">{where.root.label}</span> / {where.path}
            </>
          ) : (
            notes.openPath
          )}
        </span>
        <span className="spacer" />
        {synced ? (
          <span className={cx("editor__status", `notes-sync-status--${synced.tone}`)}>
            <span className="editor__status-dot" aria-hidden="true" />
            {synced.text}
          </span>
        ) : (
          <span className={cx("editor__status", `editor__status--${notes.saveState}`)}>
            <span className="editor__status-dot" aria-hidden="true" />
            {STATUS_TEXT[notes.saveState]}
          </span>
        )}
      </div>

      {syncConflict && <SyncConflictBanner conflict={syncConflict} />}

      {notes.conflict && (
        <div className="conflict" role="alert">
          <span className="conflict__text">This file changed on disk while you were editing.</span>
          <button
            type="button"
            className="conflict__primary"
            onClick={() => void actions.reloadNote()}
          >
            Reload
          </button>
          <button
            type="button"
            className="conflict__secondary"
            onClick={() => void actions.keepMine()}
          >
            Keep mine
          </button>
        </div>
      )}

      <div className="editor__scroll">
        <div className="editor__stack">
          <div className="editor__lines" aria-hidden="true">
            {lines.map((line, index) => (
              <div className="editor__line" key={index}>
                <span className="editor__gutter">{index + 1}</span>
                <span className={cx("editor__text", textClass(kinds[index]))}>
                  {line === "" ? " " : line}
                </span>
              </div>
            ))}
          </div>
          <textarea
            className="editor__input"
            aria-label={`Editing ${notes.openPath}`}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            value={notes.content}
            onChange={(event) => actions.editNote(event.target.value)}
            onKeyDown={onKeyDown}
            onBlur={() => void actions.saveNote()}
          />
        </div>
      </div>
    </div>
  );
}

/**
 * notes-sync-conflict: someone saved this note while you were editing it.
 * Yours stays where it is, theirs is kept next to it; shown on either one.
 */
function SyncConflictBanner({ conflict }: { conflict: LocalConflict }) {
  const sync = useNotesSync();
  const [busy, setBusy] = useState(false);
  const copyName = conflict.copyPath.slice(conflict.copyPath.lastIndexOf("/") + 1);

  const act = (run: (conflict: LocalConflict) => Promise<void>) => async () => {
    setBusy(true);
    try {
      await run(conflict);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="conflict notes-sync-conflict" role="alert">
      <span className="conflict__text">
        {conflictLead(conflict.theirName)} <span className="notes-sync-conflict__file">{copyName}</span>.
      </span>
      <button type="button" className="conflict__secondary" disabled={busy} onClick={act(sync.compare)}>
        Compare
      </button>
      <button type="button" className="conflict__secondary" disabled={busy} onClick={act(sync.useTheirs)}>
        Use theirs
      </button>
      <button type="button" className="conflict__primary" disabled={busy} onClick={act(sync.keepMine)}>
        Keep mine
      </button>
    </div>
  );
}

function textClass(kind: LineKind | undefined): string | false {
  switch (kind) {
    case "heading":
      return "editor__text--heading";
    case "code":
      return "editor__text--code";
    case "quote":
      return "editor__text--quote";
    case "list":
      return "editor__text--list";
    default:
      return false;
  }
}

function insertAtCursor(
  textarea: HTMLTextAreaElement,
  text: string,
  onChange: (value: string) => void,
): void {
  const { selectionStart, selectionEnd, value } = textarea;
  const next = value.slice(0, selectionStart) + text + value.slice(selectionEnd);
  onChange(next);
  // React re-renders from state, so restore the caret after the commit.
  requestAnimationFrame(() => {
    textarea.selectionStart = textarea.selectionEnd = selectionStart + text.length;
  });
}
