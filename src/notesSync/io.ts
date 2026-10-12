/** The engine's IO in the app: the notes commands (src-tauri notes) and the cloud API. */
import * as api from "../cloud/api";
import { call } from "../ipc/client";
import type { NoteNode } from "../ipc/types";
import type { LocalFile, NotesIO, SyncApi } from "./engine";

function files(nodes: NoteNode[], out: LocalFile[] = []): LocalFile[] {
  for (const node of nodes) {
    if (node.kind === "file") out.push({ relPath: node.relPath, mtimeMs: node.mtimeMs, size: node.size });
    else if (node.children) files(node.children, out);
  }
  return out;
}

export const notesIO: NotesIO = {
  list: async () => files(await call("notes_tree", {})),
  read: async (relPath) => (await call("notes_read", { relPath })).content,
  write: (relPath, content) => call("notes_write", { relPath, content }),
  remove: (relPath) => call("notes_delete", { relPath }),
};

export const syncApi: SyncApi = {
  getPersonalSpace: api.getPersonalSpace,
  getTeamSpace: api.getTeamSpace,
  listChanges: (spaceId, cursor) => api.listChanges(spaceId, cursor),
  getDocuments: api.getDocuments,
  putDocument: api.putDocument,
  deleteDocument: api.deleteDocument,
};
