import { toApiError, type ApiError, type Commit, type FileDiff } from '$lib/api/types';
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
  private detailGen = 0;
  private searchTimer: ReturnType<typeof setTimeout> | null = null;
  private loadedRevision = -1;

  constructor(
    readonly tab: TabState,
    private readonly pageSize = PAGE_SIZE
  ) {}

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
    this.error = null;
    this.graph = emptyGraphState();
    try {
      const page = await this.tab.api.logPage(this.tab.id, 0, this.pageSize, this.searching ? this.query.trim() : null);
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
      // no crece con la profundidad). Con búsqueda, o sin frontera utilizable, se pagina por `skip`.
      const useCursor = !searching && this.frontier.usable;
      let page = await this.tab.api.logPage(this.tab.id, this.commits.length, this.pageSize, query, useCursor ? this.frontier.list() : null);
      if (mine !== this.gen) return;
      let fresh = page.filter((c) => !this.frontier.has(c.sha));
      if (useCursor && fresh.length === 0 && page.length > 0) {
        // Una página de puros repetidos (fechas iguales): el cursor no avanzaría; por `skip` sí.
        page = await this.tab.api.logPage(this.tab.id, this.commits.length, this.pageSize, query, null);
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
