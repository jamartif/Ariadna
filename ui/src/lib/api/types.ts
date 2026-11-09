/**
 * Tipos de la interfaz, espejo de `crates/ariadna-git/src/models.rs` y `session.rs`
 * serializados con serde en camelCase (contrato: specs/rust/ipc.md).
 *
 * Reglas de serde que se respetan aquí:
 *  - `ChangeStatus` viaja como la letra de porcelain (`"M"`, `"."`, `"?"`…).
 *  - Enums con datos son «externally tagged»: `{ "commit": "abc" }`,
 *    `{ "between": { "base", "head" } }`, `{ "fastForward": { "new_head" } }`.
 *    `rename_all` en un enum renombra solo las variantes, por eso los campos de
 *    `MergeResult` conservan snake_case (`new_head`).
 *  - `GitPath` es `string` (UTF-8) o `number[]` (bytes no UTF-8).
 */

export type GitPath = string | number[];

export type ChangeStatus = '.' | 'M' | 'A' | 'D' | 'R' | 'C' | 'U' | 'T' | '?' | '!' | '';

export type ConflictKind =
  | 'bothModified'
  | 'bothAdded'
  | 'bothDeleted'
  | 'addedByUs'
  | 'addedByThem'
  | 'deletedByUs'
  | 'deletedByThem'
  | 'unknown';

export interface StatusEntry {
  path: GitPath;
  originalPath: GitPath | null;
  indexStatus: ChangeStatus;
  workTreeStatus: ChangeStatus;
  isConflicted: boolean;
  isUntracked: boolean;
  isIgnored: boolean;
  isSubmodule: boolean;
  conflictKind: ConflictKind | null;
}

export interface GitStatus {
  branchName: string | null;
  headOid: string | null;
  isDetached: boolean;
  isUnborn: boolean;
  upstream: string | null;
  ahead: number;
  behind: number;
  entries: StatusEntry[];
}

export interface Commit {
  sha: string;
  parents: string[];
  subject: string;
  body: string;
  authorName: string;
  authorEmail: string;
  authorDateIso: string;
  refs: string[];
}

export interface Branch {
  name: string;
  oid: string;
  isHead: boolean;
  upstream: string | null;
  ahead: number;
  behind: number;
}

export interface Remote {
  name: string;
  fetchUrl: string;
  pushUrl: string;
}

export type MergeFastForward = 'allow' | 'only' | 'never';

export type MergeResult =
  | 'upToDate'
  | { fastForward: { new_head: string } }
  | { merged: { new_head: string } }
  | { conflicts: { paths: GitPath[] } };

export type IntegrationState = 'none' | 'merging' | 'rebasing' | 'cherryPicking';

export type DiffScope =
  | 'unstaged'
  | 'staged'
  | { commit: string }
  | { between: { base: string; head: string } };

export type DiffLineKind = 'context' | 'addition' | 'deletion' | 'meta';

export interface DiffLine {
  id: string;
  kind: DiffLineKind;
  text: string;
  oldLineNumber: number | null;
  newLineNumber: number | null;
}

export interface DiffHunk {
  id: string;
  oldStart: number;
  oldCount: number;
  newStart: number;
  newCount: number;
  heading: string;
  lines: DiffLine[];
}

export interface FileDiff {
  path: GitPath;
  oldPath: GitPath | null;
  isBinary: boolean;
  isNewFile: boolean;
  isDeletedFile: boolean;
  hunks: DiffHunk[];
}

export type ConflictChoice = 'ours' | 'base' | 'theirs';

export interface ConflictSide {
  oid: string | null;
  content: string | null;
  isBinary: boolean;
}

export interface ConflictHunk {
  id: number;
  ours: string;
  base: string | null;
  theirs: string;
}

export interface ConflictFile {
  path: GitPath;
  kind: ConflictKind;
  ours: ConflictSide;
  base: ConflictSide;
  theirs: ConflictSide;
  workingTree: string | null;
  hunks: ConflictHunk[];
}

export interface RecentRepository {
  isFavorite: boolean;
  lastBranch?: string | null;
  /** ISO 8601 (`crate::recents::iso8601`). */
  lastOpened: string;
  name: string;
  path: string;
}

/** `session.rs`: estado consistente del repo en una revisión. */
export interface Snapshot {
  revision: number;
  status: GitStatus;
  branches: Branch[];
  remoteBranches: Branch[];
  remotes: Remote[];
  integration: IntegrationState;
  conflicts: ConflictFile[];
}

// ---- Tipos del IPC (specs/rust/ipc.md) ------------------------------------

export interface TabInfo {
  tabId: string;
  path: string;
  name: string;
  branch: string | null;
  detached: boolean;
  changeCount: number;
  conflictCount: number;
  busy: string | null;
  missing: boolean;
}

export interface GitInfo {
  path: string;
  version: string;
}

export interface AppState {
  tabs: TabInfo[];
  activeTabId: string | null;
  recents: RecentRepository[];
  git: GitInfo | null;
  gitProblem: string | null;
}

export interface OpenResult {
  tab: TabInfo;
  alreadyOpen: boolean;
}

export interface AmendDraft {
  message: string;
  canAmend: boolean;
  reason: string | null;
}

export interface CommitResult {
  done: boolean;
  sha: string | null;
}

export interface DoneResult {
  done: boolean;
}

// ---- Errores -----------------------------------------------------------------

export const API_ERROR_CODES = [
  'NOT_A_REPOSITORY',
  'GIT_NOT_FOUND',
  'COMMAND_FAILED',
  'NOTHING_TO_COMMIT',
  'EMPTY_COMMIT_MESSAGE',
  'DETACHED_HEAD_COMMIT',
  'CANNOT_AMEND',
  'LOCAL_CHANGES_BLOCK_CHECKOUT',
  'DIVERGED_LOCAL_BRANCH',
  'INVALID_REFERENCE',
  'HUNK_NOT_FOUND',
  'NO_INTEGRATION',
  'CONFIRMATION_REQUIRED',
  'PATH_ESCAPES_REPOSITORY',
  'TIMEOUT',
  'CANCELLED',
  'PARSE',
  'LOCKED',
  'DUBIOUS_OWNERSHIP',
  'AUTH',
  'IO',
  'REPO_MISSING'
] as const;

export type ApiErrorCode = (typeof API_ERROR_CODES)[number];

/** Forma serializada del error (lo que Rust rechaza en la promesa). */
export interface ApiErrorShape {
  code: ApiErrorCode | 'UNKNOWN';
  message: string;
  detail?: string | null;
}

export class ApiError extends Error implements ApiErrorShape {
  readonly code: ApiErrorCode | 'UNKNOWN';
  readonly detail: string | null;

  constructor(shape: ApiErrorShape) {
    super(shape.message);
    this.name = 'ApiError';
    this.code = shape.code;
    this.detail = shape.detail ?? null;
  }
}

function isCode(v: unknown): v is ApiErrorCode {
  return typeof v === 'string' && (API_ERROR_CODES as readonly string[]).includes(v);
}

/**
 * Convierte lo que rechaza `invoke` (objeto `ApiError`, cadena o `Error`) en
 * un `ApiError`. La UI nunca interpreta `message`: solo lo muestra.
 */
export function toApiError(e: unknown): ApiError {
  if (e instanceof ApiError) return e;
  if (typeof e === 'object' && e !== null) {
    const o = e as Record<string, unknown>;
    if (typeof o.message === 'string' && typeof o.code === 'string') {
      return new ApiError({
        code: isCode(o.code) ? o.code : 'UNKNOWN',
        message: o.message,
        detail: typeof o.detail === 'string' ? o.detail : null
      });
    }
    if (typeof o.message === 'string') {
      return new ApiError({ code: 'UNKNOWN', message: o.message });
    }
  }
  if (typeof e === 'string') return new ApiError({ code: 'UNKNOWN', message: e });
  return new ApiError({ code: 'UNKNOWN', message: String(e) });
}

// ---- Eventos ------------------------------------------------------------------

export type SessionLifecycle = 'opening' | 'active' | 'closing' | 'closed';

/**
 * `SessionEvent` serializado por `src-tauri` (fixtures/session-event.json): etiquetado
 * interno con `type`, `operationId` numérico (u64) y `ok`/`error` al terminar.
 */
export type SessionEventKind =
  | { type: 'stateChanged'; state: SessionLifecycle }
  | { type: 'operationStarted'; operationId: number; label: string }
  | { type: 'operationRetrying'; operationId: number; label: string; attempt: number; delayMs: number }
  | { type: 'operationFinished'; operationId: number; label: string; ok: boolean; error: ApiErrorShape | null }
  | { type: 'snapshotPublished'; revision: number }
  | { type: 'refreshFailed'; error: ApiErrorShape };

export interface SessionEvent {
  sessionId: string;
  revision: number;
  kind: SessionEventKind;
}

export interface ApiEvents {
  snapshot: { tabId: string; snapshot: Snapshot };
  'tab-updated': TabInfo;
  'session-event': SessionEvent;
  'tabs-changed': AppState;
}
