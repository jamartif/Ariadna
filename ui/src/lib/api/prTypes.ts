/**
 * Tipos de pull request del contrato IPC (`specs/rust/ipc.md`, «Pull requests»). Son los de
 * `crates/ariadna-hosting` serializados en camelCase; los fixtures reales de Rust
 * (`fixtures-pr/`) se comprueban contra ellos en `pr.contract.test.ts`.
 *
 * Regla: el token no existe en la UI. Ninguno de estos tipos lo lleva; Rust lo obtiene con
 * `git credential fill` en cada llamada.
 */

import type { FileDiff, RemoteProviderId } from './types';

export interface PrUser {
  /** Id del proveedor (GUID en Azure DevOps, numérico en GitHub). */
  id: string;
  displayName: string;
  /** `login` (GitHub) o `uniqueName` (Azure DevOps). */
  login: string;
}

export type PrState = 'active' | 'completed' | 'abandoned';
export type PrFilter = 'open' | 'mine' | 'reviewing' | 'closed';

/** Voto de un revisor. Azure DevOps: los cinco primeros; GitHub: `approve`, `reject` (solicitar cambios) y `comment`. */
export type PrVote = 'approve' | 'approveWithSuggestions' | 'noVote' | 'waitForAuthor' | 'reject' | 'comment';

export interface PrReviewer {
  user: PrUser;
  vote: PrVote;
  required: boolean;
}

export type PrMergeStatus = 'clean' | 'conflicts' | 'unknown';
export type PrCheckStatus = 'passing' | 'failing' | 'pending' | 'notApplicable' | 'broken';

export interface PrPolicyCheck {
  name: string;
  status: PrCheckStatus;
  /** `null` si el proveedor no lo dice (GitHub REST). */
  blocking: boolean | null;
  url: string | null;
}

export interface PrChecksSummary {
  passing: number;
  failing: number;
  pending: number;
}

/** Fila del listado (PR-02). */
export interface PullRequestSummary {
  id: number;
  title: string;
  author: PrUser;
  sourceBranch: string;
  targetBranch: string;
  state: PrState;
  isDraft: boolean;
  createdAt: string;
  updatedAt: string | null;
  reviewers: PrReviewer[];
  /** `null` en el listado de GitHub (se pide en el detalle). */
  checks: PrChecksSummary | null;
  mergeStatus: PrMergeStatus;
  webUrl: string;
}

export interface PrWorkItem {
  id: number;
  url: string | null;
}

export interface PrCommit {
  sha: string;
  subject: string;
  author: string;
  date: string | null;
}

export type PrFileStatus = 'added' | 'modified' | 'deleted' | 'renamed' | 'copied' | 'other';

export interface PrFileChange {
  path: string;
  oldPath: string | null;
  status: PrFileStatus;
  additions: number | null;
  deletions: number | null;
  binary: boolean;
}

/** Detalle (PR-03). `description` es Markdown: se renderiza desde el AST, **nunca** con `{@html}`. */
export interface PullRequestDetail extends PullRequestSummary {
  description: string;
  policies: PrPolicyCheck[];
  workItems: PrWorkItem[];
  commits: PrCommit[];
  files: PrFileChange[];
  sourceCommit: string | null;
  targetCommit: string | null;
  /** `refs/heads/x` (Azure DevOps) o `refs/pull/N/head` (GitHub). */
  checkoutRef: string;
  isFork: boolean;
  autoComplete: boolean;
  /** Estado de fusión en palabras del proveedor (`dirty`, `blocked`, `rejectedByPolicy`…). */
  mergeDetail: string | null;
}

export interface PullRequestPage {
  items: PullRequestSummary[];
  /** Empieza en 1. */
  page: number;
  hasMore: boolean;
}

/** Datos para crear una PR (PR-04). */
export interface NewPullRequest {
  title: string;
  description: string;
  sourceBranch: string;
  targetBranch: string;
  draft: boolean;
  /** Azure DevOps: ids o correos; GitHub: logins. */
  reviewers: string[];
  /** Solo Azure DevOps. */
  workItemIds: number[];
}

export type PrThreadStatus = 'active' | 'resolved' | 'closed';
export type PrDiffSide = 'old' | 'new';

export interface PrThreadAnchor {
  path: string;
  /** Empieza en 1. */
  line: number;
  side: PrDiffSide;
}

export interface PrThreadComment {
  id: string;
  author: PrUser;
  /** Markdown, nunca `{@html}`. */
  body: string;
  createdAt: string | null;
}

export interface PrReviewThread {
  id: string;
  status: PrThreadStatus;
  /** `null` = comentario general de la PR. */
  anchor: PrThreadAnchor | null;
  outdated: boolean;
  comments: PrThreadComment[];
  canResolve: boolean;
}

/** `add_pull_request_comment`: hilo nuevo (general si no hay `anchor`) o respuesta. */
export type PrCommentInput =
  | { kind: 'new'; body: string; anchor?: PrThreadAnchor | null }
  | { kind: 'reply'; threadId: string; body: string };

export type PrMergeStrategy = 'merge' | 'squash' | 'rebase' | 'rebaseMerge';

export interface PrCompleteOptions {
  strategy: PrMergeStrategy;
  deleteSourceBranch: boolean;
  /** Azure DevOps. */
  completeWorkItems: boolean;
  /** Azure DevOps: completar solo cuando se cumplan las políticas. */
  autoComplete: boolean;
  commitMessage: string | null;
}

/** Qué admite el proveedor, para mostrar solo lo posible. */
export interface PrCapabilities {
  provider: RemoteProviderId;
  votes: PrVote[];
  strategies: PrMergeStrategy[];
  autoComplete: boolean;
  resolveThreads: boolean;
  workItems: boolean;
  draft: boolean;
}

/** `pull_request_support` (PR-01). Sin red. */
export interface PrSupport {
  supported: boolean;
  remote: string | null;
  provider: RemoteProviderId;
  providerLabel: string;
  host: string;
  /** `host/propietario[/proyecto]/repo`. */
  repoLabel: string;
  capabilities: PrCapabilities | null;
  /** Por qué no hay soporte; entonces la UI ofrece solo «Abrir en el navegador». */
  reason: string | null;
}

/** Resultado de una acción con confirmación nativa: `done: false` si el usuario cancela. */
export interface PrAction {
  done: boolean;
  detail: PullRequestDetail | null;
}

export interface PrOmitted {
  path: string;
  reason: string;
}

/** `pull_request_diff`: `diffs` es el mismo `FileDiff` que usa el visor de diff. */
export interface PrDiff {
  files: PrFileChange[];
  diffs: FileDiff[];
  omitted: PrOmitted[];
}
