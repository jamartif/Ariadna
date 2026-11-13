import type {
  PrCheckStatus,
  PrFileStatus,
  PrFilter,
  PrMergeStatus,
  PrState,
  PrThreadAnchor,
  PrThreadStatus,
  PrVote,
  PullRequestSummary
} from '$lib/api/prTypes';
import { toApiError, type ApiError, type RemoteProviderId, type Snapshot } from '$lib/api/types';
import type { IconName } from '$lib/components/icons';
import type { MessageKey } from '$lib/i18n';

/** Tono visual: siempre va acompañado de icono y texto (el color nunca es el único canal). */
export type Tone = 'success' | 'danger' | 'warning' | 'info' | 'muted' | 'accent';

export const FILTERS: readonly { id: PrFilter; label: MessageKey }[] = [
  { id: 'open', label: 'pr.filter.open' },
  { id: 'mine', label: 'pr.filter.mine' },
  { id: 'reviewing', label: 'pr.filter.reviewing' },
  { id: 'closed', label: 'pr.filter.closed' }
];

export const STATE_LABEL: Record<PrState, MessageKey> = {
  active: 'pr.state.active',
  completed: 'pr.state.completed',
  abandoned: 'pr.state.abandoned'
};
export const STATE_ICON: Record<PrState, IconName> = { active: 'git-branch', completed: 'git-merge', abandoned: 'ban' };
export const STATE_TONE: Record<PrState, Tone> = { active: 'success', completed: 'accent', abandoned: 'muted' };

/** Etiqueta del voto según el proveedor: en GitHub `reject` es «solicitar cambios». */
export function voteLabel(vote: PrVote, provider: RemoteProviderId | undefined): MessageKey {
  if (provider === 'github') {
    if (vote === 'reject') return 'pr.vote.githubReject';
    if (vote === 'approve') return 'pr.vote.githubApprove';
  }
  return VOTE_LABEL[vote];
}

const VOTE_LABEL: Record<PrVote, MessageKey> = {
  approve: 'pr.vote.approve',
  approveWithSuggestions: 'pr.vote.approveWithSuggestions',
  noVote: 'pr.vote.noVote',
  waitForAuthor: 'pr.vote.waitForAuthor',
  reject: 'pr.vote.reject',
  comment: 'pr.vote.comment'
};

export const VOTE_ICON: Record<PrVote, IconName> = {
  approve: 'circle-check',
  approveWithSuggestions: 'circle-check',
  noVote: 'minus',
  waitForAuthor: 'clock',
  reject: 'ban',
  comment: 'info'
};
export const VOTE_TONE: Record<PrVote, Tone> = {
  approve: 'success',
  approveWithSuggestions: 'success',
  noVote: 'muted',
  waitForAuthor: 'warning',
  reject: 'danger',
  comment: 'info'
};

/** Texto de ayuda de cada voto en el formulario de revisión. */
export const VOTE_HINT: Record<PrVote, MessageKey> = {
  approve: 'pr.review.hint.approve',
  approveWithSuggestions: 'pr.review.hint.approveWithSuggestions',
  noVote: 'pr.review.hint.noVote',
  waitForAuthor: 'pr.review.hint.waitForAuthor',
  reject: 'pr.review.hint.reject',
  comment: 'pr.review.hint.comment'
};

export const CHECK_LABEL: Record<PrCheckStatus, MessageKey> = {
  passing: 'pr.check.passing',
  failing: 'pr.check.failing',
  pending: 'pr.check.pending',
  notApplicable: 'pr.check.notApplicable',
  broken: 'pr.check.broken'
};
export const CHECK_ICON: Record<PrCheckStatus, IconName> = {
  passing: 'circle-check',
  failing: 'circle-alert',
  pending: 'clock',
  notApplicable: 'minus',
  broken: 'triangle-alert'
};
export const CHECK_TONE: Record<PrCheckStatus, Tone> = {
  passing: 'success',
  failing: 'danger',
  pending: 'warning',
  notApplicable: 'muted',
  broken: 'danger'
};

export const MERGE_LABEL: Record<PrMergeStatus, MessageKey> = {
  clean: 'pr.merge.clean',
  conflicts: 'pr.merge.conflicts',
  unknown: 'pr.merge.unknown'
};

export const FILE_STATUS_LABEL: Record<PrFileStatus, MessageKey> = {
  added: 'pr.file.added',
  modified: 'pr.file.modified',
  deleted: 'pr.file.deleted',
  renamed: 'pr.file.renamed',
  copied: 'pr.file.copied',
  other: 'pr.file.changed'
};
export const FILE_STATUS_LETTER: Record<PrFileStatus, string> = { added: 'A', modified: 'M', deleted: 'D', renamed: 'R', copied: 'C', other: '·' };

export const THREAD_STATUS_LABEL: Record<PrThreadStatus, MessageKey> = {
  active: 'pr.thread.active',
  resolved: 'pr.thread.resolved',
  closed: 'pr.thread.closed'
};

/** Iniciales para el avatar (nunca se cargan imágenes de usuario). */
export function initials(name: string): string {
  const parts = name.trim().split(/[\s._@-]+/).filter(Boolean);
  const a = parts[0]?.[0] ?? '?';
  const b = parts.length > 1 ? (parts[parts.length - 1]?.[0] ?? '') : '';
  return (a + b).toUpperCase();
}

/** ¿Encaja la PR con la búsqueda? (título, #id, autor, ramas y revisores). */
export function matchesQuery(pr: PullRequestSummary, query: string): boolean {
  const q = query.trim().toLowerCase().replace(/^#/, '');
  if (q === '') return true;
  const hay = [
    String(pr.id),
    pr.title,
    pr.author.displayName,
    pr.author.login,
    pr.sourceBranch,
    pr.targetBranch,
    ...pr.reviewers.map((r) => r.user.displayName)
  ];
  return hay.some((h) => h.toLowerCase().includes(q));
}

/** Clave de línea del diff: `old:12` / `new:12`. */
export const lineKey = (side: 'old' | 'new', line: number): string => `${side}:${line}`;
export const anchorKey = (a: PrThreadAnchor): string => lineKey(a.side, a.line);

/** Separa una lista escrita a mano (comas, espacios o saltos de línea), sin vacíos ni repetidos. */
export function parseList(text: string): string[] {
  return [...new Set(text.split(/[\s,;]+/).map((s) => s.trim()).filter(Boolean))];
}

export function parseIds(text: string): { ids: number[]; invalid: string[] } {
  const ids: number[] = [];
  const invalid: string[] = [];
  for (const item of parseList(text.replace(/#/g, ''))) {
    if (/^\d{1,9}$/.test(item)) ids.push(Number(item));
    else invalid.push(item);
  }
  return { ids: [...new Set(ids)], invalid };
}

/** Rama de destino por defecto: la de `origin/HEAD`-like (`main`, `master`, `develop`…), sin el remoto. */
export function defaultBase(snapshot: Snapshot | null, remote: string | null, head: string | null): string {
  const prefix = `${remote ?? 'origin'}/`;
  const remotes = (snapshot?.remoteBranches ?? []).map((b) => b.name).filter((n) => n.startsWith(prefix)).map((n) => n.slice(prefix.length));
  for (const c of ['main', 'master', 'develop', 'trunk']) if (remotes.includes(c) && c !== head) return c;
  return remotes.find((r) => r !== head && r !== 'HEAD') ?? (head === 'main' ? 'master' : 'main');
}

/** Ramas posibles como destino: las del remoto (sin prefijo) y las locales, sin la de origen. */
export function baseOptions(snapshot: Snapshot | null, remote: string | null, head: string | null): string[] {
  const prefix = `${remote ?? 'origin'}/`;
  const names = new Set<string>();
  for (const b of snapshot?.remoteBranches ?? []) if (b.name.startsWith(prefix) && b.name !== `${prefix}HEAD`) names.add(b.name.slice(prefix.length));
  for (const b of snapshot?.branches ?? []) names.add(b.name);
  if (head) names.delete(head);
  return [...names].sort((a, b) => a.localeCompare(b));
}

export type Publication = 'published' | 'unpublished' | 'ahead' | 'unknown';

/**
 * ¿La rama actual está en el remoto? `unpublished` = sin rama remota; `ahead` = existe pero hay
 * commits sin subir (la PR no los incluiría).
 */
export function publication(snapshot: Snapshot | null, remote: string | null): Publication {
  const cur = snapshot?.branches.find((b) => b.isHead);
  if (!snapshot || !cur) return 'unknown';
  const prefix = `${remote ?? 'origin'}/`;
  const exists = snapshot.remoteBranches.some((b) => b.name === `${prefix}${cur.name}`);
  if (!cur.upstream && !exists) return 'unpublished';
  if (cur.upstream && !snapshot.remoteBranches.some((b) => b.name === cur.upstream) && !exists) return 'unpublished';
  return cur.ahead > 0 ? 'ahead' : 'published';
}

// ---- Errores --------------------------------------------------------------------------

export type PrErrorKind = 'auth' | 'notFound' | 'policy' | 'rateLimited' | 'network' | 'conflict' | 'other';

export function errorKind(e: unknown): PrErrorKind {
  switch (toApiError(e).code) {
    case 'AUTH':
      return 'auth';
    case 'NOT_FOUND':
      return 'notFound';
    case 'POLICY':
      return 'policy';
    case 'RATE_LIMITED':
      return 'rateLimited';
    case 'NETWORK':
      return 'network';
    case 'CONFLICT':
      return 'conflict';
    default:
      return 'other';
  }
}

/** Segundos de espera de un `RATE_LIMITED` (`detail`); entre 1 y 120, 5 si Rust no lo dice. */
export function retrySeconds(e: ApiError): number {
  const n = Number.parseInt(e.detail ?? '', 10);
  if (!Number.isFinite(n) || n <= 0) return 5;
  return Math.min(120, n);
}
