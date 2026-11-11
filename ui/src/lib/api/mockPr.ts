/**
 * Pull requests del backend simulado (`mockApi`): mismos comandos, mismas formas y mismos
 * errores que Rust, con datos de ejemplo en memoria. No hay tokens: ni existen en la UI ni se
 * simulan. Escenarios (`?scenario=`): `pr-auth`, `pr-rate-limit`, `pr-network`, `pr-empty`.
 */

import { makeHunk } from './mockData';
import type { PullRequestApi } from './prApi';
import type {
  NewPullRequest,
  PrAction,
  PrCapabilities,
  PrDiff,
  PrFileChange,
  PrFilter,
  PrReviewThread,
  PrReviewer,
  PrState,
  PrSupport,
  PrUser,
  PrVote,
  PullRequestDetail,
  PullRequestPage,
  PullRequestSummary
} from './prTypes';
import { ApiError, type ApiErrorCode, type FileDiff, type RemoteProviderId } from './types';

export interface MockPrDeps {
  scenario?: string;
  /** Remotos de la pestaña (lanza `REPO_MISSING` si no existe). */
  remotesOf(tabId: string): { name: string; fetchUrl: string }[];
  delay(): Promise<void>;
  /** Doble de la confirmación nativa. */
  confirm(message: string): boolean;
  now(): Date;
}

function err(code: ApiErrorCode, message: string, detail?: string): ApiError {
  return new ApiError({ code, message, detail });
}

export function providerOf(url: string): RemoteProviderId {
  if (/dev\.azure\.com|visualstudio\.com/i.test(url)) return 'azureDevOps';
  if (/github/i.test(url)) return 'github';
  if (/gitlab/i.test(url)) return 'gitlab';
  if (/bitbucket/i.test(url)) return 'bitbucket';
  return 'other';
}

const LABEL: Record<RemoteProviderId, string> = {
  azureDevOps: 'Azure DevOps',
  github: 'GitHub',
  gitlab: 'GitLab',
  bitbucket: 'Bitbucket',
  other: 'Otro'
};

function capabilities(provider: RemoteProviderId): PrCapabilities {
  const azure = provider === 'azureDevOps';
  return {
    provider,
    votes: azure ? ['approve', 'approveWithSuggestions', 'noVote', 'waitForAuthor', 'reject'] : ['approve', 'reject', 'comment'],
    strategies: azure ? ['merge', 'squash', 'rebase', 'rebaseMerge'] : ['merge', 'squash', 'rebase'],
    autoComplete: azure,
    resolveThreads: true,
    workItems: azure,
    draft: true
  };
}

const ME: PrUser = { id: 'u-yo', displayName: 'Yo', login: 'yo@ejemplo.test' };
const user = (login: string, name: string): PrUser => ({ id: `u-${login}`, displayName: name, login });
const ANA = user('ana', 'Ana');
const BETO = user('beto', 'Beto');
const CRIS = user('cris', 'Cris');
const DANI = user('dani', 'Dani');

interface StoredPr {
  detail: PullRequestDetail;
  threads: PrReviewThread[];
  diffs: FileDiff[];
}

function fileDiff(path: string, kind: 'modified' | 'added' | 'deleted'): FileDiff {
  const id = `pr:${path}`;
  const specs =
    kind === 'added'
      ? ['+export const nuevo = 1;', '+export const otro = 2;']
      : kind === 'deleted'
        ? ['-export const viejo = 1;']
        : [' function main() {', '-  return 1;', '+  return 2;', ' }'];
  return {
    path,
    oldPath: null,
    isBinary: false,
    isNewFile: kind === 'added',
    isDeletedFile: kind === 'deleted',
    hunks: [makeHunk(`${id}#1`, kind === 'added' ? 0 : 10, kind === 'deleted' ? 0 : 10, 'main', specs)]
  };
}

function change(path: string, status: PrFileChange['status'], additions: number, deletions: number): PrFileChange {
  return { path, oldPath: null, status, additions, deletions, binary: false };
}

function seed(provider: RemoteProviderId, base: string): Map<number, StoredPr> {
  const azure = provider === 'azureDevOps';
  const rev = (user: PrUser, vote: PrVote, required = false): PrReviewer => ({ user, vote, required });
  const mk = (
    id: number,
    title: string,
    author: PrUser,
    state: PrState,
    extra: Partial<PullRequestDetail> = {}
  ): StoredPr => {
    const files = [change('src/lib/api/api.ts', 'modified', 12, 3), change('src/lib/util/nuevo.ts', 'added', 2, 0), change('src/lib/util/viejo.ts', 'deleted', 0, 1)];
    const detail: PullRequestDetail = {
      id,
      title,
      author,
      sourceBranch: `feature/pr-${id}`,
      targetBranch: 'main',
      state,
      isDraft: false,
      createdAt: `2026-11-0${(id % 9) + 1}T10:00:00Z`,
      updatedAt: null,
      reviewers: [rev(BETO, 'approve', true), rev(CRIS, 'noVote')],
      checks: { passing: 2, failing: 0, pending: 1 },
      mergeStatus: 'clean',
      webUrl: azure ? `${base}/pullrequest/${id}` : `${base}/pull/${id}`,
      description: `## Resumen\n\nCambios de la PR ${id}.\n\n- punto uno\n- punto dos\n\n<script>alert('esto es texto, no HTML')</script>`,
      policies: [
        { name: azure ? 'Build' : 'build', status: 'passing', blocking: azure ? true : null, url: null },
        { name: azure ? 'Revisores mínimos' : 'lint', status: 'passing', blocking: azure ? true : null, url: null },
        { name: azure ? 'Comentarios resueltos' : 'e2e', status: 'pending', blocking: azure ? false : null, url: null }
      ],
      workItems: azure ? [{ id: 1200 + id, url: null }] : [],
      commits: [
        { sha: `c${id}a`.padEnd(40, '0'), subject: 'feat: primera parte', author: author.displayName, date: '2026-11-01T09:00:00Z' },
        { sha: `c${id}b`.padEnd(40, '0'), subject: 'fix: ajustes de revisión', author: author.displayName, date: '2026-11-02T09:00:00Z' }
      ],
      files,
      sourceCommit: `s${id}`.padEnd(40, '0'),
      targetCommit: `t${id}`.padEnd(40, '0'),
      checkoutRef: azure ? `refs/heads/feature/pr-${id}` : `refs/pull/${id}/head`,
      isFork: false,
      autoComplete: false,
      mergeDetail: 'clean',
      ...extra
    };
    const threads: PrReviewThread[] = [
      {
        id: `${id}01`,
        status: 'active',
        anchor: { path: 'src/lib/api/api.ts', line: 12, side: 'new' },
        outdated: false,
        comments: [
          { id: '1', author: BETO, body: '¿Y si `x` es nulo?', createdAt: '2026-11-02T10:00:00Z' },
          { id: '2', author, body: 'Buen punto, lo cubro.', createdAt: '2026-11-02T11:00:00Z' }
        ],
        canResolve: true
      },
      {
        id: azure ? `${id}02` : `issue:${id}02`,
        status: 'active',
        anchor: null,
        outdated: false,
        comments: [{ id: '1', author: CRIS, body: 'Comentario general sobre la PR.', createdAt: '2026-11-03T10:00:00Z' }],
        canResolve: azure
      }
    ];
    const diffs = [fileDiff('src/lib/api/api.ts', 'modified'), fileDiff('src/lib/util/nuevo.ts', 'added'), fileDiff('src/lib/util/viejo.ts', 'deleted')];
    return { detail, threads, diffs };
  };
  const list: StoredPr[] = [
    mk(101, 'Añade validación de formularios', ANA, 'active', { reviewers: [rev(ME, 'noVote'), rev(BETO, 'approve', true)] }),
    mk(102, 'Corrige el cierre de sesión', ME, 'active', { isDraft: true, reviewers: [rev(BETO, 'waitForAuthor'), rev(CRIS, 'noVote')] }),
    mk(103, 'Actualiza dependencias', CRIS, 'completed', { updatedAt: '2026-10-30T10:00:00Z' }),
    mk(104, 'Prueba de rendimiento', DANI, 'abandoned', { updatedAt: '2026-10-29T10:00:00Z' }),
    mk(105, 'Reescribe el módulo de facturación', ANA, 'active', {
      mergeStatus: 'conflicts',
      mergeDetail: 'dirty',
      checks: { passing: 1, failing: 1, pending: 0 },
      policies: [{ name: azure ? 'Build' : 'build', status: 'failing', blocking: azure ? true : null, url: null }]
    })
  ];
  return new Map(list.map((p) => [p.detail.id, p]));
}

function summaryOf(d: PullRequestDetail): PullRequestSummary {
  return {
    id: d.id,
    title: d.title,
    author: d.author,
    sourceBranch: d.sourceBranch,
    targetBranch: d.targetBranch,
    state: d.state,
    isDraft: d.isDraft,
    createdAt: d.createdAt,
    updatedAt: d.updatedAt,
    reviewers: d.reviewers,
    checks: d.checks,
    mergeStatus: d.mergeStatus,
    webUrl: d.webUrl
  };
}

const REJECTIONS: PrVote[] = ['reject', 'waitForAuthor'];

export function createMockPullRequests(deps: MockPrDeps): PullRequestApi {
  const stores = new Map<string, { provider: RemoteProviderId; prs: Map<number, StoredPr>; next: number }>();

  function pick(tabId: string): { name: string; fetchUrl: string; provider: RemoteProviderId } | null {
    const remotes = deps.remotesOf(tabId).map((r) => ({ ...r, provider: providerOf(r.fetchUrl) }));
    const ok = (r: { provider: RemoteProviderId }) => r.provider === 'azureDevOps' || r.provider === 'github';
    return remotes.find((r) => r.name === 'origin' && ok(r)) ?? remotes.find(ok) ?? remotes.find((r) => r.name === 'origin') ?? remotes[0] ?? null;
  }

  function scenarioError(): void {
    switch (deps.scenario) {
      case 'pr-auth':
        throw err('AUTH', 'Sin permiso: el token no tiene acceso a las pull requests.', 'pull_requests=read');
      case 'pr-rate-limit':
        throw err('RATE_LIMITED', 'El proveedor limita las peticiones: espera un momento.', '30');
      case 'pr-network':
        throw err('NETWORK', 'No se pudo conectar con el proveedor. Comprueba la red.');
      default:
    }
  }

  async function store(tabId: string) {
    await deps.delay();
    const r = pick(tabId);
    if (!r || (r.provider !== 'azureDevOps' && r.provider !== 'github')) {
      throw err('POLICY', `Las pull requests de ${r ? LABEL[r.provider] : 'este repositorio'} no se gestionan dentro de Ariadna: ábrelas en el navegador.`);
    }
    scenarioError();
    let s = stores.get(tabId);
    if (!s || s.provider !== r.provider) {
      const base = r.provider === 'azureDevOps' ? 'https://dev.azure.com/org/proj/_git/app' : 'https://github.com/acme/app';
      s = { provider: r.provider, prs: deps.scenario === 'pr-empty' ? new Map() : seed(r.provider, base), next: 200 };
      stores.set(tabId, s);
    }
    return s;
  }

  function find(s: { prs: Map<number, StoredPr> }, id: number): StoredPr {
    const p = s.prs.get(id);
    if (!p) throw err('NOT_FOUND', `No se encuentra la pull request #${id}.`);
    return p;
  }

  const confirm = (title: string, d: PullRequestDetail, extra = ''): boolean =>
    deps.confirm(`${title}: #${d.id} «${d.title}» (${d.sourceBranch} → ${d.targetBranch}).${extra}`);

  const clone = <T>(v: T): T => structuredClone(v);
  const requireActive = (d: PullRequestDetail) => {
    if (d.state !== 'active') throw err('POLICY', `La PR #${d.id} ya no está abierta.`);
  };

  return {
    async pullRequestSupport(tabId): Promise<PrSupport> {
      await deps.delay();
      const r = pick(tabId);
      if (!r) {
        return { supported: false, remote: null, provider: 'other', providerLabel: LABEL.other, host: '', repoLabel: '', capabilities: null, reason: 'Este repositorio no tiene ningún remoto.' };
      }
      const supported = r.provider === 'azureDevOps' || r.provider === 'github';
      return {
        supported,
        remote: r.name,
        provider: r.provider,
        providerLabel: LABEL[r.provider],
        host: r.provider === 'azureDevOps' ? 'dev.azure.com' : r.provider === 'github' ? 'github.com' : '',
        repoLabel: r.provider === 'azureDevOps' ? 'dev.azure.com/org/proj/app' : 'github.com/acme/app',
        capabilities: supported ? capabilities(r.provider) : null,
        reason: supported ? null : `Las pull requests de ${LABEL[r.provider]} se abren en el navegador.`
      };
    },

    async listPullRequests(tabId, filter: PrFilter, page): Promise<PullRequestPage> {
      const s = await store(tabId);
      const all = [...s.prs.values()].map((p) => p.detail).sort((a, b) => b.id - a.id);
      const hit = all.filter((d) => {
        switch (filter) {
          case 'open':
            return d.state === 'active';
          case 'mine':
            return d.state === 'active' && d.author.id === ME.id;
          case 'reviewing':
            return d.state === 'active' && d.reviewers.some((r) => r.user.id === ME.id);
          case 'closed':
            return d.state !== 'active';
        }
      });
      const per = 30;
      const p = Math.max(1, page);
      return { items: hit.slice((p - 1) * per, p * per).map((d) => clone(summaryOf(d))), page: p, hasMore: hit.length > p * per };
    },

    async getPullRequest(tabId, id) {
      const s = await store(tabId);
      return clone(find(s, id).detail);
    },

    async createPullRequest(tabId, draft: NewPullRequest) {
      const s = await store(tabId);
      if (draft.title.trim() === '') throw err('INVALID_REFERENCE', 'El título no puede estar vacío.');
      if (draft.sourceBranch === draft.targetBranch) throw err('INVALID_REFERENCE', 'La rama de origen y la de destino son la misma.');
      if ([...s.prs.values()].some((p) => p.detail.state === 'active' && p.detail.sourceBranch === draft.sourceBranch && p.detail.targetBranch === draft.targetBranch)) {
        throw err('CONFLICT', 'Ya existe una pull request abierta para esas ramas.');
      }
      const id = s.next++;
      const template = ([...s.prs.values()][0] ?? seed(s.provider, 'https://example.test/repo').get(101)) as StoredPr;
      const webBase = template.detail.webUrl.replace(/\/(pullrequest|pull)\/\d+$/, '');
      const d: PullRequestDetail = {
        ...clone(template.detail),
        id,
        title: draft.title.trim(),
        description: draft.description,
        sourceBranch: draft.sourceBranch,
        targetBranch: draft.targetBranch,
        isDraft: draft.draft,
        author: ME,
        createdAt: deps.now().toISOString(),
        updatedAt: null,
        reviewers: draft.reviewers.map((r) => ({ user: user(r, r), vote: 'noVote' as const, required: false })),
        mergeStatus: 'clean',
        mergeDetail: 'clean',
        webUrl: `${webBase}/${s.provider === 'azureDevOps' ? 'pullrequest' : 'pull'}/${id}`,
        checkoutRef: s.provider === 'azureDevOps' ? `refs/heads/${draft.sourceBranch}` : `refs/pull/${id}/head`
      };
      s.prs.set(id, { detail: d, threads: [], diffs: clone(template.diffs) });
      return clone(d);
    },

    async pullRequestTemplate(tabId) {
      await deps.delay();
      pick(tabId);
      return '## Qué cambia\n\n## Cómo se ha probado\n\n## Notas para quien revise\n';
    },

    async pullRequestDiff(tabId, id): Promise<PrDiff> {
      const s = await store(tabId);
      const p = find(s, id);
      return clone({ files: p.detail.files, diffs: p.diffs, omitted: [] });
    },

    async pullRequestThreads(tabId, id) {
      const s = await store(tabId);
      return clone(find(s, id).threads);
    },

    async addPullRequestComment(tabId, id, input) {
      const s = await store(tabId);
      const p = find(s, id);
      if (input.body.trim() === '') throw err('INVALID_REFERENCE', 'El comentario no puede estar vacío.');
      const comment = (n: number) => ({ id: String(n), author: ME, body: input.body, createdAt: deps.now().toISOString() });
      if (input.kind === 'reply') {
        const t = p.threads.find((x) => x.id === input.threadId);
        if (!t) throw err('NOT_FOUND', 'Ese hilo ya no existe.');
        if (t.id.startsWith('issue:')) throw err('INVALID_REFERENCE', 'GitHub no anida los comentarios generales: crea un comentario nuevo.');
        t.comments.push(comment(t.comments.length + 1));
        return clone(t);
      }
      const t: PrReviewThread = {
        id: s.provider === 'github' && !input.anchor ? `issue:${id}${p.threads.length + 10}` : `${id}${p.threads.length + 10}`,
        status: 'active',
        anchor: input.anchor ?? null,
        outdated: false,
        comments: [comment(1)],
        canResolve: s.provider === 'azureDevOps' || !!input.anchor
      };
      p.threads.push(t);
      return clone(t);
    },

    async resolvePullRequestThread(tabId, id, threadId, resolved) {
      const s = await store(tabId);
      const t = find(s, id).threads.find((x) => x.id === threadId);
      if (!t) throw err('NOT_FOUND', 'Ese hilo ya no existe.');
      if (!t.canResolve) throw err('INVALID_REFERENCE', 'Ese comentario no se puede resolver.');
      t.status = resolved ? 'resolved' : 'active';
      return clone(t);
    },

    async votePullRequest(tabId, id, vote, comment): Promise<PrAction> {
      const s = await store(tabId);
      const d = find(s, id).detail;
      if (!capabilities(s.provider).votes.includes(vote)) throw err('INVALID_REFERENCE', `${LABEL[s.provider]} no admite ese voto.`);
      if (REJECTIONS.includes(vote)) {
        requireActive(d);
        if (!confirm('Votar en contra', d, ' El voto bloquea la PR hasta que lo cambies.')) return { done: false, detail: null };
      }
      if (s.provider === 'github' && vote !== 'approve' && !(comment ?? '').trim()) {
        throw err('INVALID_REFERENCE', 'GitHub exige un comentario al solicitar cambios o comentar.');
      }
      const mine = d.reviewers.find((r) => r.user.id === ME.id);
      if (mine) mine.vote = vote;
      else d.reviewers.push({ user: ME, vote, required: false });
      return { done: true, detail: clone(d) };
    },

    async completePullRequest(tabId, id, options): Promise<PrAction> {
      const s = await store(tabId);
      const d = find(s, id).detail;
      const caps = capabilities(s.provider);
      if (!caps.strategies.includes(options.strategy)) throw err('INVALID_REFERENCE', `${LABEL[s.provider]} no admite esa estrategia.`);
      if (options.autoComplete && !caps.autoComplete) throw err('INVALID_REFERENCE', `${LABEL[s.provider]} no admite la finalización automática desde Ariadna.`);
      requireActive(d);
      if (!confirm('Completar pull request', d, ` Estrategia: ${options.strategy}.`)) return { done: false, detail: null };
      if (d.mergeStatus === 'conflicts') throw err('POLICY', 'La PR tiene conflictos de fusión: resuélvelos antes de completarla.');
      if (d.policies.some((p) => p.status === 'failing' && p.blocking !== false)) {
        throw err('POLICY', 'Una política obligatoria no se cumple (comprobaciones fallidas).');
      }
      if (options.autoComplete) d.autoComplete = true;
      else d.state = 'completed';
      d.updatedAt = deps.now().toISOString();
      return { done: true, detail: clone(d) };
    },

    async abandonPullRequest(tabId, id): Promise<PrAction> {
      const s = await store(tabId);
      const d = find(s, id).detail;
      requireActive(d);
      if (!confirm(s.provider === 'github' ? 'Cerrar pull request' : 'Abandonar pull request', d)) return { done: false, detail: null };
      d.state = 'abandoned';
      d.updatedAt = deps.now().toISOString();
      return { done: true, detail: clone(d) };
    },

    async reopenPullRequest(tabId, id): Promise<PrAction> {
      const s = await store(tabId);
      const d = find(s, id).detail;
      if (d.state === 'completed') throw err('POLICY', 'Una pull request completada no se puede reabrir.');
      d.state = 'active';
      return { done: true, detail: clone(d) };
    }
  };
}
