import { toApiError, type ApiError, type Commit, type FileDiff, type LogScope, type LogScopeRequest } from '$lib/api/types';
import { emptyGraphState, layoutGraph, maxWidth, type GraphRow, type GraphState } from '$lib/util/graph';
import { Frontier } from '$lib/util/cursor';
import type { TabState } from './tab.svelte';

/** Commits por página (el backend admite hasta 500). */
export const PAGE_SIZE = 100;

/**
 * Historial de UNA pestaña: lista paginada (carga la página siguiente al llegar al final),
 * grafo de ramas incremental, búsqueda global (en Rust, sobre todo el historial), y el
 * detalle del commit seleccionado (archivos + diff). Una petición vieja nunca pisa a una nueva.
 */
export class HistoryController {
  commits = $state.raw<Commit[]>([]);
  rows = $state.raw<GraphRow[]>([]);
  graphWidth = $state(1);
  query = $state('');
  /** Ámbito del historial: `all` (grafo completo, por defecto) o `current` (solo HEAD). */
  scope = $state<LogScope>('all');
  /** Rama pinchada en el riel (sin checkout): «Rama actual» recorre esta en vez de HEAD. */
  ref = $state<string | null>(null);
  /** Commit al que la lista debe desplazarse (`seq` cambia aunque el SHA se repita). */
  reveal = $state.raw<{ sha: string; seq: number } | null>(null);
  loading = $state(false);
  loadingMore = $state(false);
  hasMore = $state(true);
  error = $state.raw<ApiError | null>(null);

  selected = $state.raw<Commit | null>(null);
  files = $state.raw<FileDiff[]>([]);
  filesLoading = $state(false);
  filesError = $state.raw<ApiError | null>(null);
  fileIndex = $state(0);

  private graph: GraphState = emptyGraphState();
  /** Frontera del historial cargado: el cursor de `logPage` (ver `util/cursor.ts`). */
  private frontier = new Frontier();
  private gen = 0;
  private focusSeq = 0;
  private detailGen = 0;
  private searchTimer: ReturnType<typeof setTimeout> | null = null;
  private loadedRevision = -1;

  constructor(
    readonly tab: TabState,
    private readonly pageSize = PAGE_SIZE
  ) {
    // Si el repo cambia (merge de una PR, checkout, fetch…), el historial se recarga solo.
    tab.onRepoChanged(() => {
      if (this.loadedRevision !== -1) void this.reload();
    });
  }

  /** Carga la primera página si aún no hay nada cargado (lo usa el refresco al cambiar de pestaña). */
  ensureLoaded(): void {
    if (this.loadedRevision === -1 && !this.loading) void this.reload();
  }

  get searching(): boolean {
    return this.query.trim() !== '';
  }

  get file(): FileDiff | null {
    return this.files[this.fileIndex] ?? null;
  }

  /** Recarga desde cero (la selección se conserva si el commit sigue existiendo). */
  async reload(): Promise<void> {
    const mine = ++this.gen;
    this.loading = true;
    // Una página siguiente en vuelo queda descartada (`gen`) y su `finally` ya no la libera: sin
    // esto `loadingMore` se quedaba en `true` y el historial dejaba de paginar (commits que faltan).
    this.loadingMore = false;
    this.error = null;
    this.graph = emptyGraphState();
    try {
      const page = await this.tab.api.logPage(this.tab.id, 0, this.pageSize, this.searching ? this.query.trim() : null, null, this.requestScope());
      if (mine !== this.gen) return;
      this.commits = page;
      this.frontier.reset();
      this.frontier.add(page);
      this.hasMore = page.length >= this.pageSize;
      this.recomputeGraph(page, true);
      this.loadedRevision = this.tab.revision;
      const keep = this.selected ? page.find((c) => c.sha === this.selected?.sha) : undefined;
      if (keep) this.selected = keep;
      else if (page[0]) await this.select(page[0]);
      else this.clearSelection();
    } catch (e) {
      if (mine !== this.gen) return;
      this.error = toApiError(e);
    } finally {
      if (mine === this.gen) this.loading = false;
    }
  }

  /** Carga la página siguiente. */
  async loadMore(): Promise<void> {
    if (this.loading || this.loadingMore || !this.hasMore) return;
    const mine = this.gen;
    this.loadingMore = true;
    try {
      const searching = this.searching;
      const query = searching ? this.query.trim() : null;
      // Sin búsqueda se continúa desde la frontera del historial cargado (sin `--skip`: el coste
      // no crece con la profundidad). Con búsqueda, con el grafo completo (varias puntas) o sin
      // frontera utilizable, se pagina por `skip`.
      const useCursor = !searching && this.scope === 'current' && this.frontier.usable;
      let page = await this.tab.api.logPage(this.tab.id, this.commits.length, this.pageSize, query, useCursor ? this.frontier.list() : null, this.requestScope());
      if (mine !== this.gen) return;
      let fresh = page.filter((c) => !this.frontier.has(c.sha));
      if (useCursor && fresh.length === 0 && page.length > 0) {
        // Una página de puros repetidos (fechas iguales): el cursor no avanzaría; por `skip` sí.
        page = await this.tab.api.logPage(this.tab.id, this.commits.length, this.pageSize, query, null, this.requestScope());
        if (mine !== this.gen) return;
        fresh = page.filter((c) => !this.frontier.has(c.sha));
      }
      // Repetidos dentro de la propia página (la frontera solo conoce lo ya cargado).
      const inPage = new Set<string>();
      fresh = fresh.filter((c) => !inPage.has(c.sha) && inPage.add(c.sha));
      this.commits = [...this.commits, ...fresh];
      this.frontier.add(fresh);
      // Con cursor una página corta no es el final (pueden haberse descartado repetidos): se acaba
      // cuando no queda frontera. Por `skip`, cuando la página no llega a llenarse.
      this.hasMore = fresh.length > 0 && (useCursor ? this.frontier.size > 0 || page.length >= this.pageSize : page.length >= this.pageSize);
      this.recomputeGraph(fresh, false);
    } catch (e) {
      if (mine === this.gen) this.error = toApiError(e);
    } finally {
      if (mine === this.gen) this.loadingMore = false;
    }
  }

  private recomputeGraph(page: Commit[], reset: boolean): void {
    if (this.searching) {
      // Con un filtro la topología no es contigua: sin grafo.
      this.rows = [];
      this.graphWidth = 1;
      return;
    }
    const out = layoutGraph(page, reset ? emptyGraphState() : this.graph);
    this.graph = out.state;
    this.rows = reset ? out.rows : [...this.rows, ...out.rows];
    this.graphWidth = maxWidth(this.rows);
  }

  /** Búsqueda con retardo (150 ms) para no lanzar un `git log` por tecla. */
  setQuery(q: string): void {
    this.query = q;
    if (this.searchTimer) clearTimeout(this.searchTimer);
    this.searchTimer = setTimeout(() => void this.reload(), 150);
  }

  /** Ámbito que se pide a Rust: con una rama pinchada, «Rama actual» recorre esa rama. */
  private requestScope(): LogScopeRequest {
    return this.scope === 'current' && this.ref ? `ref:${this.ref}` : this.scope;
  }

  /**
   * Una rama pinchada en el riel (`null` = ninguna distinta de HEAD). En «Rama actual» el historial
   * pasa a ser el de esa rama; en el grafo completo se lleva la selección a su último commit.
   */
  async focusBranch(name: string | null, tipSha?: string): Promise<void> {
    const changed = name !== this.ref;
    this.ref = name;
    // Dos ramas pinchadas seguidas: solo la última puede elegir la selección.
    const mine = ++this.focusSeq;
    if (this.scope === 'current') {
      if (!changed) return;
      this.selected = null;
      await this.reload();
      return;
    }
    // Solo al pinchar otra rama: un snapshot nuevo no debe devolver la selección al último commit.
    if (!changed || !name || !tipSha) return;
    // Grafo completo: el commit puede no estar en las páginas cargadas; se pagina hasta dar con él.
    for (let i = 0; i < 40 && !this.commits.some((c) => c.sha === tipSha) && this.hasMore; i++) {
      // Si hay una carga en curso se espera a que acabe en vez de abandonar sin seleccionar nada.
      while (this.loading || this.loadingMore) await new Promise((r) => setTimeout(r, 30));
      if (mine !== this.focusSeq) return;
      await this.loadMore();
    }
    if (mine !== this.focusSeq) return;
    const tip = this.commits.find((c) => c.sha === tipSha);
    if (!tip) return;
    await this.select(tip);
    this.reveal = { sha: tipSha, seq: (this.reveal?.seq ?? 0) + 1 };
  }

  /** Cambia el ámbito del historial (`all` = grafo completo) y recarga desde cero. */
  setScope(scope: LogScope): void {
    if (this.scope === scope) return;
    this.scope = scope;
    void this.reload();
  }

  /** Recarga si el repo cambió desde la última carga (nuevo commit, checkout, merge…). */
  syncWithRevision(): void {
    if (this.loadedRevision === -1) return;
    if (this.tab.revision !== this.loadedRevision && !this.loading) void this.reload();
  }

  async select(commit: Commit): Promise<void> {
    this.selected = commit;
    this.fileIndex = 0;
    const mine = ++this.detailGen;
    this.files = [];
    this.filesError = null;
    this.filesLoading = true;
    try {
      const files = await this.tab.api.diff(this.tab.id, { commit: commit.sha }, []);
      if (mine !== this.detailGen) return;
      this.files = files;
    } catch (e) {
      if (mine === this.detailGen) this.filesError = toApiError(e);
    } finally {
      if (mine === this.detailGen) this.filesLoading = false;
    }
  }

  selectFile(i: number): void {
    if (i >= 0 && i < this.files.length) this.fileIndex = i;
  }

  /** Mueve la selección `delta` commits (teclado). */
  async move(delta: number): Promise<void> {
    if (this.commits.length === 0) return;
    const cur = this.selected ? this.commits.findIndex((c) => c.sha === this.selected?.sha) : -1;
    const next = Math.max(0, Math.min(this.commits.length - 1, cur + delta));
    const c = this.commits[next];
    if (c && c.sha !== this.selected?.sha) await this.select(c);
    if (next >= this.commits.length - 10) void this.loadMore();
  }

  private clearSelection(): void {
    this.selected = null;
    this.files = [];
    this.filesError = null;
    this.fileIndex = 0;
  }

  dispose(): void {
    if (this.searchTimer) clearTimeout(this.searchTimer);
    this.gen++;
    this.detailGen++;
  }

  /**
   * Hibernación: suelta los commits, el grafo y los diffs cargados. Las respuestas en vuelo se
   * descartan; al volver a mostrar el panel `reload()` lo trae de nuevo.
   */
  release(): void {
    this.dispose();
    this.commits = [];
    this.rows = [];
    this.graphWidth = 1;
    this.graph = emptyGraphState();
    this.frontier.reset();
    this.query = '';
    this.ref = null;
    this.reveal = null;
    this.loading = false;
    this.loadingMore = false;
    this.hasMore = true;
    this.error = null;
    this.selected = null;
    this.files = [];
    this.filesLoading = false;
    this.filesError = null;
    this.fileIndex = 0;
    this.loadedRevision = -1;
  }
}

const registry = new WeakMap<TabState, HistoryController>();

/** Controlador de historial de la pestaña (uno por pestaña, creado a demanda). */
export function historyOf(tab: TabState): HistoryController {
  let h = registry.get(tab);
  if (!h) {
    const created = new HistoryController(tab);
    h = created;
    registry.set(tab, created);
    tab.onHibernate(() => created.release());
  }
  return h;
}
