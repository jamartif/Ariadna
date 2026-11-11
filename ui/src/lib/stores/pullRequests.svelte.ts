import type {
  NewPullRequest,
  PrAction,
  PrCapabilities,
  PrCommentInput,
  PrCompleteOptions,
  PrDiff,
  PrFilter,
  PrReviewThread,
  PrSupport,
  PrVote,
  PullRequestDetail,
  PullRequestSummary
} from '$lib/api/prTypes';
import { toApiError, type AiPullRequestSuggestion, type ApiError } from '$lib/api/types';
import { t } from '$lib/i18n';
import { matchesQuery, publication, type Publication } from '$lib/util/pr';
import { remotesOf } from './remotes.svelte';
import { toasts } from './toasts.svelte';
import type { TabState } from './tab.svelte';

export type PrDetailView = 'overview' | 'files' | 'conversation';
export type PrBusy = 'vote' | 'complete' | 'abandon' | 'reopen' | 'comment' | 'resolve' | 'create' | 'push' | 'ai';

const AUTO_KEY = 'ariadna.pr.autorefresh.v1';
/** PR-09: refresco automático opcional, apagado por defecto, mínimo 60 s y solo con el panel abierto. */
export const AUTO_REFRESH_OPTIONS = [0, 60, 300] as const;

export function summaryOf(d: PullRequestSummary): PullRequestSummary {
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

/**
 * Pull requests de UNA pestaña (spec §4.4). La verdad vive en el proveedor y llega por Rust:
 * aquí solo hay una vista (lista, detalle, diff, hilos) y el estado de las acciones en curso.
 * Las confirmaciones destructivas (completar, abandonar, votar en contra) las abre Rust como
 * diálogo nativo; la UI solo envía el comando y muestra el `PrAction.detail` que vuelve.
 * El token no existe en la UI.
 */
export class PullRequestsController {
  // ---- Soporte (PR-01, sin red) ----
  support = $state.raw<PrSupport | null>(null);
  supportLoaded = $state(false);
  supportError = $state.raw<ApiError | null>(null);

  // ---- Lista (PR-02) ----
  filter = $state<PrFilter>('open');
  query = $state('');
  page = $state(1);
  items = $state.raw<PullRequestSummary[]>([]);
  hasMore = $state(false);
  listLoading = $state(false);
  listLoaded = $state(false);
  listError = $state.raw<ApiError | null>(null);
  autoRefresh = $state<number>(loadAuto());

  // ---- Detalle (PR-03) ----
  selectedId = $state<number | null>(null);
  detail = $state.raw<PullRequestDetail | null>(null);
  detailLoading = $state(false);
  detailError = $state.raw<ApiError | null>(null);
  view = $state<PrDetailView>('overview');

  // ---- Archivos e hilos (PR-06) ----
  diff = $state.raw<PrDiff | null>(null);
  diffLoading = $state(false);
  diffError = $state.raw<ApiError | null>(null);
  threads = $state.raw<PrReviewThread[]>([]);
  threadsLoading = $state(false);
  threadsError = $state.raw<ApiError | null>(null);

  /** Hilo al que saltar al abrir la pestaña de archivos (desde «Conversación»). */
  focus = $state.raw<{ path: string; threadId: string } | null>(null);

  // ---- Acciones ----
  busy = $state<PrBusy | null>(null);
  actionError = $state.raw<ApiError | null>(null);
  /** Formulario de crear PR abierto en la columna derecha. */
  creating = $state(false);
  createError = $state.raw<ApiError | null>(null);
  /** Plantilla del repo (`null` = no hay) y si ya se pidió. */
  template = $state.raw<string | null>(null);
  templateLoaded = $state(false);

  private listGen = 0;
  private detailGen = 0;
  private diffGen = 0;
  private threadsGen = 0;
  private supportGen = 0;
  private disposed = false;

  constructor(readonly tab: TabState) {
    tab.onHibernate(() => this.release());
  }

  get api() {
    return this.tab.api;
  }

  get supported(): boolean {
    return this.support?.supported === true;
  }

  get caps(): PrCapabilities | null {
    return this.support?.capabilities ?? null;
  }

  /** Lista tal como se ve: la búsqueda filtra lo cargado de la página actual. */
  get visible(): PullRequestSummary[] {
    return this.query.trim() === '' ? this.items : this.items.filter((p) => matchesQuery(p, this.query));
  }

  get busyAny(): boolean {
    return this.busy !== null;
  }

  get publication(): Publication {
    return publication(this.tab.snapshot, this.support?.remote ?? null);
  }

  get activeThreads(): number {
    return this.threads.filter((x) => x.status === 'active').length;
  }

  release(): void {
    this.items = [];
    this.detail = null;
    this.diff = null;
    this.threads = [];
    this.listLoaded = false;
    this.selectedId = null;
    this.creating = false;
  }

  dispose(): void {
    this.disposed = true;
    this.listGen++;
    this.detailGen++;
    this.diffGen++;
    this.threadsGen++;
  }

  // ---- Soporte ---------------------------------------------------------------------------

  async loadSupport(): Promise<void> {
    const mine = ++this.supportGen;
    try {
      const s = await this.api.pullRequestSupport(this.tab.id);
      if (mine !== this.supportGen || this.disposed) return;
      this.support = s;
      this.supportError = null;
    } catch (e) {
      if (mine !== this.supportGen || this.disposed) return;
      this.supportError = toApiError(e);
      this.support = null;
    } finally {
      if (mine === this.supportGen) this.supportLoaded = true;
    }
  }

  // ---- Lista -----------------------------------------------------------------------------

  /** Carga la página `page` del filtro actual. `quiet` no vacía la lista mientras llega. */
  async loadList(page = this.page, quiet = false): Promise<void> {
    const mine = ++this.listGen;
    this.listLoading = true;
    this.listError = null;
    if (!quiet) this.items = [];
    try {
      const res = await this.api.listPullRequests(this.tab.id, this.filter, page);
      if (mine !== this.listGen || this.disposed) return;
      this.items = res.items;
      this.page = res.page;
      this.hasMore = res.hasMore;
      this.listLoaded = true;
    } catch (e) {
      if (mine !== this.listGen || this.disposed) return;
      this.listError = toApiError(e);
      this.listLoaded = true;
    } finally {
      if (mine === this.listGen) this.listLoading = false;
    }
  }

  async setFilter(filter: PrFilter): Promise<void> {
    if (filter === this.filter && this.listLoaded && !this.listError) return;
    this.filter = filter;
    this.page = 1;
    await this.loadList(1);
  }

  async nextPage(): Promise<void> {
    if (this.hasMore && !this.listLoading) await this.loadList(this.page + 1);
  }

  async prevPage(): Promise<void> {
    if (this.page > 1 && !this.listLoading) await this.loadList(this.page - 1);
  }

  /** Refresco manual (o por intervalo): lista y, si hay una abierta, su detalle e hilos. */
  async refresh(): Promise<void> {
    const work: Promise<unknown>[] = [this.loadList(this.page, true)];
    if (this.selectedId !== null) {
      work.push(this.loadDetail(this.selectedId, true));
      work.push(this.loadThreads(this.selectedId, true));
      if (this.view === 'files') work.push(this.loadDiff(this.selectedId, true));
    }
    await Promise.all(work);
  }

  setAutoRefresh(seconds: number): void {
    this.autoRefresh = (AUTO_REFRESH_OPTIONS as readonly number[]).includes(seconds) ? seconds : 0;
    try {
      localStorage.setItem(AUTO_KEY, String(this.autoRefresh));
    } catch {
      /* ignorar */
    }
  }

  // ---- Detalle ---------------------------------------------------------------------------

  select(id: number): void {
    if (this.selectedId === id && this.detail && !this.creating) {
      this.creating = false;
      return;
    }
    this.creating = false;
    this.selectedId = id;
    this.detail = null;
    this.diff = null;
    this.diffError = null;
    this.threads = [];
    this.threadsError = null;
    this.detailError = null;
    this.actionError = null;
    this.view = 'overview';
    void this.loadDetail(id);
    void this.loadThreads(id);
  }

  closeDetail(): void {
    this.selectedId = null;
    this.detail = null;
    this.creating = false;
    this.detailGen++;
  }

  async loadDetail(id: number, quiet = false): Promise<void> {
    const mine = ++this.detailGen;
    if (!quiet) this.detailLoading = true;
    this.detailError = null;
    try {
      const d = await this.api.getPullRequest(this.tab.id, id);
      if (mine !== this.detailGen || this.disposed || this.selectedId !== id) return;
      this.applyDetail(d);
    } catch (e) {
      if (mine !== this.detailGen || this.disposed) return;
      this.detailError = toApiError(e);
    } finally {
      if (mine === this.detailGen) this.detailLoading = false;
    }
  }

  async loadThreads(id: number, quiet = false): Promise<void> {
    const mine = ++this.threadsGen;
    if (!quiet) this.threadsLoading = true;
    this.threadsError = null;
    try {
      const list = await this.api.pullRequestThreads(this.tab.id, id);
      if (mine !== this.threadsGen || this.disposed || this.selectedId !== id) return;
      this.threads = list;
    } catch (e) {
      if (mine !== this.threadsGen || this.disposed) return;
      this.threadsError = toApiError(e);
    } finally {
      if (mine === this.threadsGen) this.threadsLoading = false;
    }
  }

  async loadDiff(id: number, quiet = false): Promise<void> {
    const mine = ++this.diffGen;
    if (!quiet) this.diffLoading = true;
    this.diffError = null;
    try {
      const d = await this.api.pullRequestDiff(this.tab.id, id);
      if (mine !== this.diffGen || this.disposed || this.selectedId !== id) return;
      this.diff = d;
    } catch (e) {
      if (mine !== this.diffGen || this.disposed) return;
      this.diffError = toApiError(e);
    } finally {
      if (mine === this.diffGen) this.diffLoading = false;
    }
  }

  /** Abre el archivo del hilo en la pestaña de archivos y lo resalta. */
  goToThread(thread: PrReviewThread): void {
    if (!thread.anchor) return;
    this.focus = { path: thread.anchor.path, threadId: thread.id };
    this.setView('files');
  }

  setView(view: PrDetailView): void {
    this.view = view;
    if (view === 'files' && this.selectedId !== null && !this.diff && !this.diffLoading) void this.loadDiff(this.selectedId);
  }

  /** Aplica un detalle nuevo y refleja el cambio en la fila de la lista. */
  private applyDetail(d: PullRequestDetail): void {
    this.detail = d;
    const s = summaryOf(d);
    this.items = this.items.map((p) => (p.id === d.id ? s : p));
  }

  // ---- Acciones --------------------------------------------------------------------------

  /** Ejecuta una acción del detalle. El error se queda en `actionError` (no es un toast: lleva reintento). */
  private async act<T>(kind: PrBusy, fn: () => Promise<T>): Promise<T | undefined> {
    if (this.busy) return undefined;
    this.busy = kind;
    this.actionError = null;
    try {
      return await fn();
    } catch (e) {
      if (!this.disposed) this.actionError = toApiError(e);
      return undefined;
    } finally {
      this.busy = null;
    }
  }

  /** Resultado de una acción con confirmación nativa: `done: false` = el usuario canceló. */
  private finish(res: PrAction | undefined, message: string): boolean {
    if (!res) return false;
    if (!res.done) {
      toasts.announce(t('pr.cancelled'));
      return false;
    }
    if (res.detail) this.applyDetail(res.detail);
    toasts.success(message);
    void this.loadList(this.page, true);
    return true;
  }

  async vote(vote: PrVote, comment: string | null): Promise<boolean> {
    const id = this.selectedId;
    if (id === null) return false;
    const res = await this.act('vote', () => this.api.votePullRequest(this.tab.id, id, vote, comment && comment.trim() !== '' ? comment.trim() : null));
    const ok = this.finish(res, t('pr.toast.voted'));
    if (ok && comment && comment.trim() !== '') void this.loadThreads(id, true);
    return ok;
  }

  async complete(options: PrCompleteOptions): Promise<boolean> {
    const id = this.selectedId;
    if (id === null) return false;
    const res = await this.act('complete', () => this.api.completePullRequest(this.tab.id, id, options));
    return this.finish(res, options.autoComplete ? t('pr.toast.autoComplete') : t('pr.toast.completed'));
  }

  async abandon(): Promise<boolean> {
    const id = this.selectedId;
    if (id === null) return false;
    const res = await this.act('abandon', () => this.api.abandonPullRequest(this.tab.id, id));
    return this.finish(res, this.support?.provider === 'github' ? t('pr.toast.closed') : t('pr.toast.abandoned'));
  }

  async reopen(): Promise<boolean> {
    const id = this.selectedId;
    if (id === null) return false;
    const res = await this.act('reopen', () => this.api.reopenPullRequest(this.tab.id, id));
    return this.finish(res, t('pr.toast.reopened'));
  }

  /** Crea un hilo (general o anclado) o una respuesta. Devuelve el hilo resultante. */
  async comment(input: PrCommentInput): Promise<PrReviewThread | null> {
    const id = this.selectedId;
    if (id === null) return null;
    const thread = await this.act('comment', () => this.api.addPullRequestComment(this.tab.id, id, input));
    if (!thread) return null;
    this.upsertThread(thread);
    toasts.announce(t('pr.toast.commented'));
    return thread;
  }

  async resolve(threadId: string, resolved: boolean): Promise<boolean> {
    const id = this.selectedId;
    if (id === null) return false;
    const thread = await this.act('resolve', () => this.api.resolvePullRequestThread(this.tab.id, id, threadId, resolved));
    if (!thread) return false;
    this.upsertThread(thread);
    toasts.announce(resolved ? t('pr.toast.resolved') : t('pr.toast.reactivated'));
    return true;
  }

  private upsertThread(thread: PrReviewThread): void {
    const i = this.threads.findIndex((x) => x.id === thread.id);
    this.threads = i < 0 ? [...this.threads, thread] : this.threads.map((x) => (x.id === thread.id ? thread : x));
  }

  dismissActionError(): void {
    this.actionError = null;
  }

  // ---- Crear (PR-04) ---------------------------------------------------------------------

  startCreate(): void {
    this.creating = true;
    this.createError = null;
    if (!this.templateLoaded) void this.loadTemplate();
  }

  cancelCreate(): void {
    this.creating = false;
    this.createError = null;
  }

  async loadTemplate(): Promise<void> {
    try {
      this.template = await this.api.pullRequestTemplate(this.tab.id);
    } catch {
      this.template = null; // la plantilla es opcional: sin ella el formulario sigue
    } finally {
      this.templateLoaded = true;
    }
  }

  async create(draft: NewPullRequest): Promise<PullRequestDetail | null> {
    if (this.busy) return null;
    this.busy = 'create';
    this.createError = null;
    try {
      const d = await this.api.createPullRequest(this.tab.id, draft);
      if (this.disposed) return d;
      toasts.success(t('pr.toast.created', { id: d.id }));
      this.creating = false;
      this.filter = 'open';
      this.page = 1;
      this.selectedId = d.id;
      this.detail = d;
      this.diff = null;
      this.threads = [];
      this.view = 'overview';
      void this.loadThreads(d.id);
      void this.loadList(1, true);
      return d;
    } catch (e) {
      if (!this.disposed) this.createError = toApiError(e);
      return null;
    } finally {
      this.busy = null;
    }
  }

  /** Publica la rama actual (push con `--set-upstream`) usando los comandos de remotos. */
  async publish(): Promise<boolean> {
    const remote = this.support?.remote ?? 'origin';
    if (this.busy) return false;
    this.busy = 'push';
    try {
      return await remotesOf(this.tab).push(remote);
    } finally {
      this.busy = null;
    }
  }

  /** «Generar con IA»: Rust lee la rama, los commits y el diff contra la base; aquí solo se propone. */
  async generate(base: string | null, template: string | null): Promise<{ ok: true; value: AiPullRequestSuggestion } | { ok: false; error: ApiError }> {
    try {
      const value = await this.api.aiGeneratePullRequest(this.tab.id, { base: base && base.trim() !== '' ? base.trim() : null, template, language: this.tab.ai.options.language });
      return { ok: true, value };
    } catch (e) {
      return { ok: false, error: toApiError(e) };
    }
  }

  async openUrl(url: string): Promise<void> {
    try {
      await this.api.openExternalUrl(url);
    } catch (e) {
      toasts.error(t('pr.openFailed'), e);
    }
  }
}

function loadAuto(): number {
  try {
    const n = Number(localStorage.getItem(AUTO_KEY));
    return (AUTO_REFRESH_OPTIONS as readonly number[]).includes(n) ? n : 0;
  } catch {
    return 0;
  }
}

const registry = new WeakMap<TabState, PullRequestsController>();

export function prOf(tab: TabState): PullRequestsController {
  let c = registry.get(tab);
  if (!c) {
    c = new PullRequestsController(tab);
    registry.set(tab, c);
  }
  return c;
}
