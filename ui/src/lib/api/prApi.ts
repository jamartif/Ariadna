import type {
  NewPullRequest,
  PrAction,
  PrCommentInput,
  PrCompleteOptions,
  PrDiff,
  PrFilter,
  PrReviewThread,
  PrSupport,
  PrVote,
  PullRequestDetail,
  PullRequestPage
} from './prTypes';

/**
 * Comandos de pull request (`specs/rust/ipc.md`, «Pull requests»). Todos rechazan con `ApiError`
 * (`AUTH` con el permiso que falta en `detail`, `NOT_FOUND`, `POLICY`, `CONFLICT`,
 * `RATE_LIMITED`, `NETWORK`…). El token NO pasa por la UI: Rust lo obtiene con
 * `git credential fill` en cada llamada y lo borra.
 */
export interface PullRequestApi {
  /** PR-01: ¿admite la pestaña PR dentro de la app? Sin red. */
  pullRequestSupport(tabId: string): Promise<PrSupport>;
  /** PR-02. Páginas desde 1. */
  listPullRequests(tabId: string, filter: PrFilter, page: number): Promise<PullRequestPage>;
  /** PR-03. */
  getPullRequest(tabId: string, id: number): Promise<PullRequestDetail>;
  /** PR-04. */
  createPullRequest(tabId: string, draft: NewPullRequest): Promise<PullRequestDetail>;
  /** PR-04: plantilla de descripción del repo, o `null`. */
  pullRequestTemplate(tabId: string): Promise<string | null>;
  /** PR-06: archivos cambiados y su diff (mismo `FileDiff` que el visor). */
  pullRequestDiff(tabId: string, id: number): Promise<PrDiff>;
  pullRequestThreads(tabId: string, id: number): Promise<PrReviewThread[]>;
  addPullRequestComment(tabId: string, id: number, input: PrCommentInput): Promise<PrReviewThread>;
  resolvePullRequestThread(tabId: string, id: number, threadId: string, resolved: boolean): Promise<PrReviewThread>;
  /** PR-07. El voto de rechazo (`reject`, `waitForAuthor`) abre confirmación nativa en Rust; `done: false` si se cancela. */
  votePullRequest(tabId: string, id: number, vote: PrVote, comment?: string | null): Promise<PrAction>;
  /** PR-08. Confirmación nativa; las políticas del proveedor mandan (`POLICY` si no se cumplen). */
  completePullRequest(tabId: string, id: number, options: PrCompleteOptions): Promise<PrAction>;
  /** PR-08. Abandona (Azure DevOps) o cierra (GitHub), con confirmación nativa. */
  abandonPullRequest(tabId: string, id: number): Promise<PrAction>;
  reopenPullRequest(tabId: string, id: number): Promise<PrAction>;
}
