import type {
  AiCommitSuggestion,
  AiGenerateOptions,
  AiSettingsPatch,
  AiState,
  AmendDraft,
  ApiEvents,
  AppState,
  Branch,
  Commit,
  CommitResult,
  ConflictFile,
  ContextPreview,
  CloneAuth,
  CredentialTest,
  DiffHunk,
  DiffScope,
  DoneResult,
  FileDiff,
  GitPath,
  MergeFastForward,
  MergeResult,
  OpenResult,
  RecentRepository,
  Snapshot
} from './types';

export type Unlisten = () => void;

/**
 * Interfaz única entre la UI y el backend. La implementan `tauriApi` (invoke/listen)
 * y `mockApi` (en memoria). Un método por comando de `specs/rust/ipc.md`.
 * Todos rechazan con `ApiError`.
 */
export interface Api {
  // Aplicación
  appState(): Promise<AppState>;
  pickFolder(): Promise<string | null>;
  openRepository(path: string): Promise<OpenResult>;
  closeTab(tabId: string): Promise<{ closed: boolean }>;
  activateTab(tabId: string): Promise<void>;
  reorderTabs(tabIds: string[]): Promise<void>;
  toggleFavorite(path: string): Promise<RecentRepository[]>;
  forgetRecent(path: string): Promise<RecentRepository[]>;
  cancelOperation(tabId: string, operationId: number): Promise<boolean>;
  /** Abre el navegador del sistema (solo https y hosts de la lista de Rust). */
  openExternalUrl(url: string): Promise<void>;
  /**
   * Clona y abre una pestaña. Emite `clone-progress`; cancelable con `cancelOperation('', id)`.
   * `auth` es un PAT de **un solo uso**: Rust lo pasa a git por `GIT_ASKPASS` temporal y no lo guarda.
   */
  cloneRepository(url: string, destination: string, branch: string | null, auth?: CloneAuth | null): Promise<OpenResult>;
  /** Entrega el secreto a `git credential approve`. El secreto NUNCA vuelve a la UI. */
  storeCredential(url: string, username: string, secret: string): Promise<void>;
  testCredential(url: string): Promise<CredentialTest>;
  forgetCredential(url: string, username: string): Promise<void>;

  // Pestaña
  getSnapshot(tabId: string): Promise<Snapshot>;
  refresh(tabId: string): Promise<Snapshot | null>;
  stage(tabId: string, paths: GitPath[]): Promise<void>;
  unstage(tabId: string, paths: GitPath[]): Promise<void>;
  stageHunk(tabId: string, path: GitPath, hunk: DiffHunk): Promise<void>;
  unstageHunk(tabId: string, path: GitPath, hunk: DiffHunk): Promise<void>;
  discard(tabId: string, paths: GitPath[]): Promise<DoneResult>;
  commit(tabId: string, message: string, allowDetached: boolean): Promise<CommitResult>;
  amend(tabId: string, message: string): Promise<string>;
  amendDraft(tabId: string): Promise<AmendDraft>;
  diff(tabId: string, scope: DiffScope, paths: GitPath[]): Promise<FileDiff[]>;
  log(tabId: string, limit: number, query: string | null): Promise<Commit[]>;
  /** Página del historial: `limit` commits tras saltar `skip` (con `query`, busca en todo el historial). */
  logPage(tabId: string, skip: number, limit: number, query: string | null): Promise<Commit[]>;
  createBranch(tabId: string, name: string, startPoint: string | null): Promise<void>;
  renameBranch(tabId: string, oldName: string, newName: string): Promise<void>;
  checkout(tabId: string, reference: string): Promise<DoneResult>;
  /** Confirmación nativa desde Rust: `{ done: false }` si el usuario cancela. */
  deleteBranch(tabId: string, name: string, force: boolean): Promise<DoneResult>;
  merge(tabId: string, branch: string, fastForward: MergeFastForward): Promise<MergeResult>;
  /** Fusiona solo si no hay conflictos; si los hay aborta y rechaza con `MERGE_CONFLICTS`. */
  mergeClean(tabId: string, branch: string, fastForward: MergeFastForward): Promise<MergeResult>;
  continueIntegration(tabId: string): Promise<void>;
  abortIntegration(tabId: string): Promise<DoneResult>;
  getConflicts(tabId: string): Promise<ConflictFile[]>;
  writeConflictResolution(tabId: string, path: GitPath, contents: number[]): Promise<DoneResult>;
  markResolved(tabId: string, path: GitPath): Promise<void>;
  fetch(tabId: string, remote: string): Promise<void>;
  pull(tabId: string, remote: string, refspec: string | null, rebase: boolean): Promise<MergeResult>;
  push(tabId: string, remote: string, refspec: string | null, setUpstream: boolean): Promise<void>;

  // IA (solo propone; lee Rust el diff preparado, la UI no envía ningún diff)
  aiState(tabId: string): Promise<AiState>;
  aiUpdateSettings(patch: AiSettingsPatch): Promise<AiState>;
  aiPreviewContext(tabId: string): Promise<ContextPreview>;
  aiGenerateCommitMessage(tabId: string, options: AiGenerateOptions): Promise<AiCommitSuggestion>;
  aiCancel(tabId: string): Promise<boolean>;

  // Eventos Rust → UI
  listen<K extends keyof ApiEvents>(event: K, handler: (payload: ApiEvents[K]) => void): Promise<Unlisten>;
}

export type { Branch };
