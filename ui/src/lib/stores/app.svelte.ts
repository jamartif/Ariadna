import type { Api } from '$lib/api/api';
import {
  toApiError,
  type ApiError,
  type AppState,
  type GitInfo,
  type OpenResult,
  type RecentRepository,
  type TabInfo
} from '$lib/api/types';
import { t } from '$lib/i18n';
import { runMenuAction } from '$lib/shortcuts/registry';
import { moveItem } from '$lib/util/virtual';
import { TabState } from './tab.svelte';
import { toasts } from './toasts.svelte';

export type Entry = { id: string; kind: 'repo'; info: TabInfo } | { id: string; kind: 'new' };

/** Tope blando de pestañas (spec §8): avisa pero no impide abrir. */
export const SOFT_TAB_LIMIT = 20;

/** Una pestaña que lleva este tiempo sin verse libera sus datos pesados (spec §8). */
export const HIBERNATE_AFTER_MS = 10 * 60 * 1000;
/** Cada cuánto revisa la ventana qué pestañas toca hibernar. */
export const HIBERNATE_CHECK_MS = 60 * 1000;

export type OpenOutcome = { ok: true } | { ok: false; error: ApiError };

/** Estado de la ventana: pestañas (repos y «nuevas»), recientes, git y paleta. */
export class AppStore {
  entries = $state.raw<Entry[]>([]);
  private activeRaw = $state<string | null>(null);
  recents = $state.raw<RecentRepository[]>([]);
  git = $state.raw<GitInfo | null>(null);
  gitProblem = $state<string | null>(null);
  status = $state<'loading' | 'ready' | 'error'>('loading');
  loadError = $state.raw<ApiError | null>(null);
  paletteOpen = $state(false);
  /** Diálogo «Clonar repositorio» (desde la bienvenida o la paleta). */
  cloneOpen = $state(false);
  opening = $state(false);

  private tabStores = new Map<string, TabState>();
  private unlisten: (() => void)[] = [];
  private draftCounter = 0;
  /** Desde cuándo no se ve cada pestaña (ms). Solo cuentan las que no son la activa. */
  private idleSince = new Map<string, number>();
  private lastActive: string | null = null;
  /** Borradores a los que ya se les avisó del tope de pestañas (para no avisar dos veces). */
  private limitWarned = new Set<string>();

  constructor(
    readonly api: Api,
    private readonly clock: () => number = () => Date.now()
  ) {}

  get activeId(): string | null {
    return this.activeRaw;
  }

  /** Cambiar de pestaña arranca el reloj de la anterior y despierta la nueva (hibernación). */
  set activeId(id: string | null) {
    this.activeRaw = id;
    this.trackActive(id);
  }

  // ---- Arranque -----------------------------------------------------------------

  async init(): Promise<void> {
    const offs = await Promise.all([
      this.api.listen('tabs-changed', (s) => this.applyAppState(s)),
      this.api.listen('tab-updated', (info) => this.patchInfo(info)),
      this.api.listen('notice', (n) => {
        if (n.kind === 'watcherFallback') toasts.push({ kind: 'warning', title: t('toast.watcherFallback'), message: n.message });
      }),
      this.api.listen('menu-action', (a) => {
        runMenuAction(a.id);
      })
    ]);
    this.unlisten = offs;
    await this.reload();
  }

  /**
   * La ventana recupera el foco: se pide un refresco de la pestaña activa. Es lo que más se
   * nota cuando se edita fuera y el vigilante del SO no entrega eventos (red, WSL).
   * Rust no publica nada si no cambió nada.
   */
  refreshActiveOnFocus(): void {
    const tab = this.activeTab;
    if (tab) void tab.refresh();
  }

  // ---- Hibernación (spec §8) ------------------------------------------------------------
  // Rust ya libera el vigilante de las pestañas ocultas y las refresca al activarlas. Aquí se
  // libera lo pesado de la interfaz (snapshot, historial y diffs cargados) tras N minutos.

  /** Llamar cuando cambia la pestaña activa: arranca el reloj de la anterior y despierta la nueva. */
  trackActive(id: string | null): void {
    const prev = this.lastActive;
    if (prev === id) return;
    const now = this.clock();
    if (prev) this.idleSince.set(prev, now);
    this.lastActive = id;
    if (id) {
      this.idleSince.delete(id);
      this.tabStores.get(id)?.wake();
    }
  }

  /** Hiberna las pestañas ocultas que llevan `afterMs` sin verse. Devuelve las hibernadas. */
  hibernateIdle(afterMs: number = HIBERNATE_AFTER_MS): string[] {
    const now = this.clock();
    const done: string[] = [];
    for (const e of this.entries) {
      if (e.kind !== 'repo' || e.id === this.activeId) continue;
      const since = this.idleSince.get(e.id);
      if (since === undefined) {
        this.idleSince.set(e.id, now);
        continue;
      }
      if (now - since < afterMs) continue;
      if (this.tabStores.get(e.id)?.hibernate()) done.push(e.id);
    }
    return done;
  }

  async reload(): Promise<void> {
    this.status = 'loading';
    this.loadError = null;
    try {
      const state = await this.api.appState();
      this.applyAppState(state, true);
      this.status = 'ready';
    } catch (e) {
      this.loadError = toApiError(e);
      this.status = 'error';
    }
  }

  dispose(): void {
    this.unlisten.forEach((f) => f());
    this.unlisten = [];
    for (const s of this.tabStores.values()) s.dispose();
    this.tabStores.clear();
  }

  // ---- Reconciliación con el estado de Rust ------------------------------------------

  applyAppState(state: AppState, initial = false): void {
    this.recents = state.recents;
    this.git = state.git;
    this.gitProblem = state.gitProblem;

    const repoEntries: Entry[] = state.tabs.map((info) => ({ id: info.tabId, kind: 'repo', info }));
    const drafts = initial ? [] : this.entries.filter((e) => e.kind === 'new');
    const next: Entry[] = [...repoEntries];
    // Los borradores («Nueva pestaña») conservan su posición relativa.
    for (const d of drafts) {
      const prev = this.entries.findIndex((e) => e.id === d.id);
      next.splice(Math.min(prev, next.length), 0, d);
    }
    if (next.length === 0) next.push(this.makeDraft());
    this.entries = next;

    const keepDraft = !initial && drafts.some((d) => d.id === this.activeId);
    if (!keepDraft) {
      const wanted = state.activeTabId && next.some((e) => e.id === state.activeTabId) ? state.activeTabId : null;
      this.activeId = wanted ?? next[0]?.id ?? null;
    }
    // Libera los TabState de pestañas que ya no existen.
    for (const id of [...this.tabStores.keys()]) {
      if (!next.some((e) => e.id === id)) this.dropTabState(id);
    }
    for (const id of [...this.idleSince.keys()]) {
      if (!next.some((e) => e.id === id)) this.idleSince.delete(id);
    }
  }

  private patchInfo(info: TabInfo): void {
    this.entries = this.entries.map((e) => (e.kind === 'repo' && e.id === info.tabId ? { ...e, info } : e));
  }

  private makeDraft(): Entry {
    return { id: `new:${++this.draftCounter}`, kind: 'new' };
  }

  // ---- Consultas -----------------------------------------------------------------------

  get activeEntry(): Entry | null {
    return this.entries.find((e) => e.id === this.activeId) ?? null;
  }

  get repoTabs(): TabInfo[] {
    return this.entries.flatMap((e) => (e.kind === 'repo' ? [e.info] : []));
  }

  /** Estado de la pestaña de repo (se crea al primer uso). */
  tabState(id: string): TabState | null {
    const entry = this.entries.find((e) => e.id === id);
    if (!entry || entry.kind !== 'repo') return null;
    let s = this.tabStores.get(id);
    if (!s) {
      s = new TabState(this.api, entry.info);
      this.tabStores.set(id, s);
      const created = s;
      // Se difiere: `tabState` se llama durante el renderizado y no puede mutar estado.
      queueMicrotask(() => {
        void created.attach();
        if (entry.info.missing) created.updateInfo(entry.info);
        else void created.load();
      });
    }
    return s;
  }

  get activeTab(): TabState | null {
    return this.activeId ? this.tabState(this.activeId) : null;
  }

  private dropTabState(id: string): void {
    this.tabStores.get(id)?.dispose();
    this.tabStores.delete(id);
  }

  // ---- Acciones de pestaña ---------------------------------------------------------------

  newTab(): void {
    const existing = this.entries.find((e) => e.kind === 'new');
    if (existing) {
      this.activeId = existing.id;
      return;
    }
    const draft = this.makeDraft();
    if (this.entries.length >= SOFT_TAB_LIMIT) {
      toasts.warning(t('tabs.limitTitle'), t('tabs.limitBody', { n: SOFT_TAB_LIMIT }));
      this.limitWarned.add(draft.id);
    }
    this.entries = [...this.entries, draft];
    this.activeId = draft.id;
  }

  activate(id: string): void {
    if (!this.entries.some((e) => e.id === id)) return;
    this.activeId = id;
    const e = this.entries.find((x) => x.id === id);
    if (e?.kind === 'repo') void this.api.activateTab(id).catch(() => undefined);
  }

  gotoIndex(n: number): void {
    const e = this.entries[n - 1];
    if (e) this.activate(e.id);
  }

  step(delta: 1 | -1): void {
    if (this.entries.length === 0) return;
    const i = this.entries.findIndex((e) => e.id === this.activeId);
    const next = (i + delta + this.entries.length) % this.entries.length;
    const e = this.entries[next];
    if (e) this.activate(e.id);
  }

  reorder(ids: string[]): void {
    const byId = new Map(this.entries.map((e) => [e.id, e] as const));
    const next = ids.map((id) => byId.get(id)).filter((e): e is Entry => !!e);
    if (next.length !== this.entries.length) return;
    this.entries = next;
    const real = next.filter((e) => e.kind === 'repo').map((e) => e.id);
    void this.api.reorderTabs(real).catch((e) => toasts.error(t('tabs.reorderFailed'), e));
  }

  moveTab(id: string, delta: number): void {
    const ids = this.entries.map((e) => e.id);
    const from = ids.indexOf(id);
    if (from < 0) return;
    const next = moveItem(ids, from, delta);
    if (next.join('|') !== ids.join('|')) this.reorder(next);
  }

  /** Cierra una pestaña. Para repos, Rust puede preguntar y devolver `closed: false`. */
  async close(id: string): Promise<boolean> {
    const i = this.entries.findIndex((e) => e.id === id);
    if (i < 0) return false;
    const entry = this.entries[i] as Entry;
    if (entry.kind === 'repo') {
      try {
        const res = await this.api.closeTab(id);
        if (!res.closed) return false;
      } catch (e) {
        toasts.error(t('tabs.closeFailed'), e);
        return false;
      }
    }
    // `tabs-changed` pudo adelantarse y ya haber quitado la entrada.
    const cur = this.entries.findIndex((e) => e.id === id);
    if (cur >= 0) {
      const next = this.entries.filter((e) => e.id !== id);
      if (next.length === 0) next.push(this.makeDraft());
      this.entries = next;
      if (this.activeId === id) this.activeId = next[Math.min(cur, next.length - 1)]?.id ?? null;
    }
    this.dropTabState(id);
    return true;
  }

  // ---- Abrir repos y recientes -------------------------------------------------------------

  async browse(): Promise<string | null> {
    try {
      return await this.api.pickFolder();
    } catch (e) {
      toasts.error(t('welcome.browseFailed'), e);
      return null;
    }
  }

  /** Abre una ruta en la pestaña `draftId` (si se da) o en una pestaña nueva. */
  async openPath(path: string, draftId?: string): Promise<OpenOutcome> {
    this.opening = true;
    try {
      const res = await this.api.openRepository(path);
      this.adopt(res, draftId);
      return { ok: true };
    } catch (e) {
      return { ok: false, error: toApiError(e) };
    } finally {
      this.opening = false;
    }
  }

  /** Integra un `OpenResult` (abrir o clonar): enfoca la pestaña y refresca los recientes. */
  adopt(res: OpenResult, draftId?: string): void {
    const tab = res.tab;
    const real = this.entries.findIndex((e) => e.id === tab.tabId);
    const draftIdx = draftId ? this.entries.findIndex((e) => e.id === draftId) : -1;
    let next = [...this.entries];
    if (real >= 0) {
      // Ya abierto: se enfoca y el borrador sobra.
      next[real] = { id: tab.tabId, kind: 'repo', info: tab };
      if (draftIdx >= 0) next = next.filter((e) => e.id !== draftId);
    } else if (draftIdx >= 0) {
      next[draftIdx] = { id: tab.tabId, kind: 'repo', info: tab };
    } else {
      next.push({ id: tab.tabId, kind: 'repo', info: tab });
    }
    this.entries = next;
    this.activeId = tab.tabId;
    if (res.alreadyOpen) toasts.info(t('welcome.alreadyOpen', { name: tab.name }));
    // Avisos de Rust (tope de pestañas, worktree). El del tope ya salió al crear la pestaña nueva.
    else if (res.warning && !(draftId && this.limitWarned.has(draftId))) {
      toasts.warning(t('tabs.limitTitle'), res.warning);
    }
    if (draftId) this.limitWarned.delete(draftId);
    // Refresca la lista de recientes desde Rust.
    void this.api
      .appState()
      .then((s) => {
        this.recents = s.recents;
      })
      .catch(() => undefined);
  }

  /** Abrir repositorio… (⌘O / Ctrl+O y menú Archivo): diálogo de carpeta nativo y abrir en una pestaña. */
  async openViaDialog(): Promise<void> {
    const path = await this.browse();
    if (!path) return;
    const draft = this.activeEntry?.kind === 'new' ? this.activeEntry.id : undefined;
    const out = await this.openPath(path, draft);
    if (!out.ok) toasts.error(t('repo.openFailed'), out.error);
  }

  openClone(): void {
    this.cloneOpen = true;
  }

  async toggleFavorite(path: string): Promise<void> {
    try {
      this.recents = await this.api.toggleFavorite(path);
    } catch (e) {
      toasts.error(t('welcome.favoriteFailed'), e);
    }
  }

  async forget(path: string): Promise<void> {
    try {
      this.recents = await this.api.forgetRecent(path);
      toasts.announce(t('welcome.forgotten'));
    } catch (e) {
      toasts.error(t('welcome.forgetFailed'), e);
    }
  }

  /** «Localizar» un repo desaparecido: elige carpeta y sustituye la pestaña. */
  async locate(id: string): Promise<void> {
    const path = await this.browse();
    if (!path) return;
    const out = await this.openPath(path);
    if (!out.ok) {
      toasts.error(t('banner.locateFailed'), out.error);
      return;
    }
    await this.close(id);
  }
}
