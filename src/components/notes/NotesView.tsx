import { cx } from "../../lib/cx";
import { shortcutLabel } from "../../lib/hotkeys";
import { NotesSyncProvider, useNotesSync } from "../../notesSync/NotesSyncProvider";
import { useAppActions, useAppState, useHotkeyScheme } from "../../state/AppProvider";
import { NoteEditor } from "./NoteEditor";
import { NotePreview } from "./NotePreview";
import { NoteTree } from "./NoteTree";

/**
 * Tree, editor and preview. The tree folds away to the left and the preview
 * to the right, each into a slim rail that brings it back; the editor in the
 * middle always stays and takes whatever width is freed.
 *
 * The tab is always mounted, so notes sync runs from here in the background
 * whichever tab is showing.
 */
export function NotesView() {
  return (
    <NotesSyncProvider>
      <NotesPanels />
    </NotesSyncProvider>
  );
}

function NotesPanels() {
  const { notes } = useAppState();
  const actions = useAppActions();
  const scheme = useHotkeyScheme();
  const sync = useNotesSync();

  return (
    <div
      className={cx(
        "notes",
        sync.active && "notes--synced",
        notes.treeCollapsed && "notes--no-tree",
        notes.previewCollapsed && "notes--no-preview",
      )}
    >
      {notes.treeCollapsed ? (
        <PanelRail
          side="left"
          label="Notes"
          hint={shortcutLabel(scheme, "notes-tree")}
          onExpand={() => actions.collapseNotesPanel("tree", false)}
        />
      ) : (
        <NoteTree />
      )}

      <NoteEditor />

      {notes.previewCollapsed ? (
        <PanelRail
          side="right"
          label="Preview"
          hint={shortcutLabel(scheme, "notes-preview")}
          onExpand={() => actions.collapseNotesPanel("preview", false)}
        />
      ) : (
        <NotePreview />
      )}
    </div>
  );
}

interface PanelRailProps {
  side: "left" | "right";
  label: string;
  hint: string;
  onExpand: () => void;
}

/** A collapsed panel: one narrow strip with its name and an expand chevron. */
function PanelRail({ side, label, hint, onExpand }: PanelRailProps) {
  return (
    <button
      type="button"
      className={cx("rail", `rail--${side}`)}
      title={`Show ${label.toLowerCase()} (${hint})`}
      aria-label={`Show ${label.toLowerCase()}`}
      onClick={onExpand}
    >
      <span className="rail__chevron" aria-hidden="true">
        {side === "left" ? "›" : "‹"}
      </span>
      <span className="rail__label">{label}</span>
    </button>
  );
}
