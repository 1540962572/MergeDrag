/**
 * Mirror of the Rust `merge-core` types (serde output).
 * Keep in sync with src-tauri/crates/merge-core/src/lib.rs:
 *   Decision:  { "kind": "...", "payload": ... }
 *   Hunk:      { "kind": "Clean" | "Conflict", ... }
 *   MergeDocument: { "fileLabel", "hunks" }
 */

export type Decision =
  | { kind: "Unresolved" }
  | { kind: "TakeLocal" }
  | { kind: "TakeRemote" }
  | { kind: "TakeBoth"; payload: { localFirst: boolean } }
  | { kind: "Ignore" }
  | { kind: "Manual"; payload: string };

export type Hunk =
  | { kind: "Clean"; text: string }
  | {
      kind: "Conflict";
      id: number;
      local: string;
      remote: string;
      base: string | null;
      decision: Decision;
    };

export type Side = "local" | "remote" | "base";

/** A conflicted file in the left-hand list (mirror of ConflictFileDto). */
export interface ConflictFile {
  path: string;
  isBinary: boolean;
  unresolvedCount: number;
}

/** Mirror of Rust MergeDocument. */
export interface MergeDocument {
  fileLabel: string;
  hunks: Hunk[];
}

/** Mirror of the SessionSnapshot returned by open_session / load_sample. */
export interface SessionSnapshot {
  repoRoot: string | null;
  files: ConflictFile[];
  current: MergeDocument | null;
  message: string | null;
}

/** Mirror of the save_merged result. */
export interface SaveResult {
  added: boolean;
  warning: string | null;
}

// ---- workspace (mirror of git-bridge::WorkspaceStatus / commands DTOs) -----

export interface UpstreamInfo {
  name: string;
  remoteUrl: string | null;
  ahead: number;
  behind: number;
}

/** Mirror of git-bridge::WorkspaceStatus. */
export interface WorkspaceStatus {
  root: string;
  branch: string | null;
  upstream: UpstreamInfo | null;
  merging: boolean;
  unmergedCount: number;
  dirty: boolean;
  summary: string;
}

export interface RecentWorkspace {
  path: string;
  lastOpened: number;
}

export interface WorkspaceSnapshot {
  status: WorkspaceStatus | null;
  recent: RecentWorkspace[];
  activePath: string | null;
  message: string | null;
}

export interface PullOutcome {
  message: string;
  status: WorkspaceStatus | null;
  conflicted: boolean;
}

/** Mirror of git-bridge::BranchInfo. */
export interface BranchInfo {
  name: string;
  current: boolean;
}

/** Mirror of git-bridge::StashEntry. */
export interface StashEntry {
  index: number;
  message: string;
}

/** Render a document to plain text (what `apply()` produces in Rust). */
export function applyHunks(hunks: Hunk[]): string {
  return hunks
    .map((hunk) => {
      if (hunk.kind === "Clean") {
        return hunk.text;
      }
      switch (hunk.decision.kind) {
        case "TakeLocal":
          return hunk.local;
        case "TakeRemote":
          return hunk.remote;
        case "TakeBoth":
          return hunk.decision.payload.localFirst
            ? `${hunk.local}${hunk.remote}`
            : `${hunk.remote}${hunk.local}`;
        case "Ignore":
          return "";
        case "Manual":
          return hunk.decision.payload;
        case "Unresolved":
          return `<<<<<<< MergeDrag:ours\n${hunk.local}=======\n${hunk.remote}>>>>>>> MergeDrag:theirs\n`;
      }
    })
    .join("");
}

/** Text for one pane of the three-pane view. */
export function paneText(hunks: Hunk[], pane: "local" | "remote" | "result"): string {
  if (pane === "result") {
    return applyHunks(hunks);
  }
  return hunks
    .map((hunk) => {
      if (hunk.kind === "Clean") {
        return hunk.text;
      }
      return pane === "local" ? hunk.local : hunk.remote;
    })
    .join("");
}

export function unresolvedCount(hunks: Hunk[]): number {
  return hunks.filter(
    (hunk) => hunk.kind === "Conflict" && hunk.decision.kind === "Unresolved",
  ).length;
}

/** Total number of conflict hunks. */
export function conflictCount(hunks: Hunk[]): number {
  return hunks.filter((hunk) => hunk.kind === "Conflict").length;
}