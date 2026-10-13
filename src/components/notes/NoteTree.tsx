import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import type { NoteNode } from "../../ipc/types";
import { cx } from "../../lib/cx";
import { useNotesSync, type RowMark, type SyncRoot } from "../../notesSync/NotesSyncProvider";
import { useAppActions, useAppState } from "../../state/AppProvider";
import { CollapseButton } from "./CollapseButton";

const INDENT_PX = 16;
const BASE_PAD_PX = 8;

export function NoteTree() {
  const { notes } = useAppState();
  const actions = useAppActions();
  const sync = useNotesSync();
  const [renaming, setRenaming] = useState<string | null>(null);
  // Sync roots fold on their own; the folders inside them use the notes state.
  const [foldedRoots, setFoldedRoots] = useState<string[]>([]);

  // While sync is on, new notes go to the open note's space, so a team folder can get them too.
  const where = sync.active && notes.openPath ? sync.rootOf(notes.openPath) : null;
  const parentForNew = where && where.root.folder ? where.root.folder : null;

  const level = (nodes: NoteNode[], depth: number) => (
    <TreeLevel
      nodes={nodes}
      depth={depth}
      renaming={renaming}
      onRenaming={setRenaming}
      expanded={notes.expanded}
      openPath={notes.openPath}
      onOpen={(relPath) => void actions.openNote(relPath)}
      onToggle={actions.toggleFolder}
      onRename={(relPath, name) => void actions.renameNode(relPath, name)}
      onDelete={(node) => void actions.deleteNode(node.relPath, node.kind)}
      markOf={sync.active ? sync.markOf : undefined}
    />
  );

  const toggleRoot = (key: string) =>
    setFoldedRoots((current) => (current.includes(key) ? current.filter((k) => k !== key) : [...current, key]));

  return (
    <div className="notes__panel">
      <div className="tree__head">
        <CollapseButton
          side="left"
          label="Hide the notes list"
          onClick={() => actions.collapseNotesPanel("tree", true)}
        />
        <div className="tree__title">Notes</div>
        <button
          type="button"
          className="tree__add"
          title="New file"
          onClick={() => void actions.createNode(parentForNew, "file")}
        >
          + File
        </button>
        <button
          type="button"
          className="tree__add"
          title="New folder"
          onClick={() => void actions.createNode(parentForNew, "folder")}
        >
          + Folder
        </button>
      </div>

      <div className={cx("tree__body", sync.active && "tree--synced")} role="tree" aria-label="Notes">
        {sync.active ? (
          sync.roots.map((root) => (
            <SyncRootRow
              key={root.key}
              root={root}
              open={!foldedRoots.includes(root.key)}
              onToggle={() => toggleRoot(root.key)}
            >
              {level(childrenOf(root, notes.tree, sync.roots), 1)}
            </SyncRootRow>
          ))
        ) : notes.tree.length === 0 ? (
          <p className="tree__empty">
            {notes.root
              ? "No notes yet. Create the first one with + File."
              : "No notes folder configured yet."}
          </p>
        ) : (
          level(notes.tree, 0)
        )}
      </div>

      {sync.footer && (
        <div className="notes-sync-foot" role="status">
          <div className="notes-sync-foot__line">
            <span className={cx("cloud-dot", FOOT_DOT[sync.footer.tone])} aria-hidden="true" />
            {sync.footer.line}
          </div>
          {sync.footer.text && <div className="notes-sync-foot__text">{sync.footer.text}</div>}
        </div>
      )}
    </div>
  );
}

const FOOT_DOT = {
  ok: "cloud-dot--ok",
  warn: "cloud-dot--warn",
  ring: "cloud-dot--ring",
  idle: "notes-sync-foot__dot--idle",
} as const;

/** What a sync root holds: a team's folder, or everything else for Personal. */
function childrenOf(root: SyncRoot, tree: NoteNode[], roots: SyncRoot[]): NoteNode[] {
  if (root.kind === "team") {
    return tree.find((node) => node.kind === "folder" && node.relPath === root.folder)?.children ?? [];
  }
  const teamFolders = new Set(roots.filter((r) => r.kind === "team").map((r) => r.folder));
  return tree.filter((node) => !(node.kind === "folder" && teamFolders.has(node.relPath)));
}

/** A top-level root while sync is on (Notes board 02): Personal or a team, with its mark. */
function SyncRootRow({
  root,
  open,
  onToggle,
  children,
}: {
  root: SyncRoot;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <div role="treeitem" aria-expanded={open}>
      <div className="tree__row tree__row--folder tree__row--root" style={{ paddingLeft: BASE_PAD_PX }}>
        <span className="tree__glyph" aria-hidden="true">
          {open ? "▾" : "▸"}
        </span>
        <button type="button" className="tree__name ellipsis" style={{ textAlign: "left" }} onClick={onToggle}>
          {root.label}
        </button>
        {root.mark && <span className="tree__mark">{root.mark}</span>}
      </div>
      {open && children}
    </div>
  );
}

interface TreeLevelProps {
  nodes: NoteNode[];
  depth: number;
  expanded: string[];
  openPath: string | null;
  renaming: string | null;
  onRenaming: (relPath: string | null) => void;
  onOpen: (relPath: string) => void;
  onToggle: (relPath: string) => void;
  onRename: (relPath: string, name: string) => void;
  onDelete: (node: NoteNode) => void;
  /** Sync's mark at the end of a row: "waiting", "conflict", "not synced". */
  markOf?: (node: NoteNode, selected: boolean) => RowMark | null;
}

function TreeLevel(props: TreeLevelProps) {
  const { nodes, depth, expanded, openPath, renaming } = props;

  return (
    <>
      {nodes.map((node) => {
        const isFolder = node.kind === "folder";
        const isOpen = expanded.includes(node.relPath);
        const selected = openPath === node.relPath;
        const mark = props.markOf?.(node, selected) ?? null;

        return (
          <div key={node.relPath} role="treeitem" aria-expanded={isFolder ? isOpen : undefined}>
            <div
              className={cx(
                "tree__row",
                isFolder && "tree__row--folder",
                selected && "tree__row--selected",
              )}
              style={{ paddingLeft: BASE_PAD_PX + depth * INDENT_PX }}
            >
              <span className="tree__glyph" aria-hidden="true">
                {isFolder ? (isOpen ? "▾" : "▸") : ""}
              </span>

              {renaming === node.relPath ? (
                <RenameInput
                  initial={node.name}
                  onCommit={(name) => {
                    props.onRenaming(null);
                    props.onRename(node.relPath, name);
                  }}
                  onCancel={() => props.onRenaming(null)}
                />
              ) : (
                <button
                  type="button"
                  className="tree__name ellipsis"
                  style={{ textAlign: "left" }}
                  onClick={() =>
                    isFolder ? props.onToggle(node.relPath) : props.onOpen(node.relPath)
                  }
                >
                  {node.name}
                </button>
              )}

              {mark && (
                <span className={cx("tree__mark", `tree__mark--${mark.tone}`)} title={mark.reason}>
                  {mark.text}
                </span>
              )}

              <button
                type="button"
                className="tree__action"
                onClick={() => props.onRenaming(node.relPath)}
              >
                Rename
              </button>
              <button
                type="button"
                className="tree__action tree__action--danger"
                onClick={() => props.onDelete(node)}
              >
                Delete
              </button>
            </div>

            {isFolder && isOpen && node.children && node.children.length > 0 && (
              <TreeLevel {...props} nodes={node.children} depth={depth + 1} />
            )}
          </div>
        );
      })}
    </>
  );
}

function RenameInput({
  initial,
  onCommit,
  onCancel,
}: {
  initial: string;
  onCommit: (name: string) => void;
  onCancel: () => void;
}) {
  const [value, setValue] = useState(initial);
  const input = useRef<HTMLInputElement>(null);
  // Escape closes the input, which also fires blur; only one of them may win.
  const settled = useRef(false);

  const finish = (commit: boolean) => {
    if (settled.current) return;
    settled.current = true;
    if (commit) onCommit(value);
    else onCancel();
  };

  useEffect(() => {
    const node = input.current;
    if (!node) return;
    node.focus();
    const dot = initial.lastIndexOf(".");
    node.setSelectionRange(0, dot > 0 ? dot : initial.length);
  }, [initial]);

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    event.stopPropagation();
    if (event.key === "Enter") finish(true);
    if (event.key === "Escape") finish(false);
  };

  return (
    <input
      ref={input}
      className="tree__rename"
      value={value}
      aria-label="New name"
      onChange={(event) => setValue(event.target.value)}
      onKeyDown={onKeyDown}
      onBlur={() => finish(true)}
    />
  );
}
