import type { ApiError, FileDiff, StashEntry, StashPushOptions } from '$lib/api/types';
import { toApiError } from '$lib/api/types';
import { t, tn } from '$lib/i18n';
import { pathText } from '$lib/util/gitpath';
import { toasts } from './toasts.svelte';
import type { TabState } from './tab.svelte';

/**
 * Stash de UNA pestaña. Un stash se identifica por su `oid`: el índice (`stash@{n}`) cambia al
 * guardar o borrar, así que la interfaz nunca lo usa para actuar. Borrar pide confirmación
 * nativa a Rust. Si `apply`/`pop` dejan conflictos, el repo conserva ese estado y se lleva al
 * usuario al panel de Conflictos.
 */
export class StashController {
  list = $state.raw<StashEntry[]>([]);
  loaded = $state(false);
  loading = $state(false);
  /** Operación en curso (`push`, `apply`, `pop`, `drop`) o `null`. */
  busy = $state<string | null>(null);
  /** Formulario «Guardar cambios en stash» abierto. */
  pushOpen = $state(false);
  selectedOid = $state<string | null>(null);
  diff = $state.raw<FileDiff[] | null>(null);
  diffLoading = $state(false);
  diffError = $state.raw<ApiError | null>(null);
  /** Fallo al leer la lista. No avisa con un toast: se lee en segundo plano con cada snapshot. */
  loadError = $state.raw<ApiError | null>(null);
  /** Archivo del stash cuyo diff se muestra. */
  selectedFile = $state<string | null>(null);

  private loadSeq = 0;
  private diffSeq = 0;

  constructor(readonly tab: TabState) {}

  get count(): number {
    return this.list.length;
  }

  get selected(): StashEntry | null {
    return this.list.find((s) => s.oid === this.selectedOid) ?? null;
  }

  get selectedDiff(): FileDiff | null {
    const files = this.diff ?? [];
    return files.find((f) => pathText(f.path) === this.selectedFile) ?? files[0] ?? null;
  }

  /** Vuelve a leer la lista (barata: un `git stash list`). Conserva la selección si sigue existiendo. */
  async load(): Promise<void> {
    const mine = ++this.loadSeq;
    this.loading = true;
    try {
      const list = await this.tab.api.stashList(this.tab.id);
      if (mine !== this.loadSeq) return;
      this.list = list;
      this.loaded = true;
      this.loadError = null;
      if (this.selectedOid !== null && !list.some((s) => s.oid === this.selectedOid)) this.clearSelection();
      else if (this.selectedOid === null && list[0] && this.tab.panel === 'stashes') void this.select(list[0].oid);
    } catch (e) {
      if (mine !== this.loadSeq) return;
      this.loadError = toApiError(e);
    } finally {
      if (mine === this.loadSeq) this.loading = false;
    }
  }

  private clearSelection(): void {
    this.selectedOid = null;
    this.diff = null;
    this.diffError = null;
    this.selectedFile = null;
    this.diffSeq++;
  }

  /** Selecciona un stash y carga su diff. */
  async select(oid: string): Promise<void> {
    this.selectedOid = oid;
    this.selectedFile = null;
    this.diff = null;
    this.diffError = null;
    const mine = ++this.diffSeq;
    this.diffLoading = true;
    try {
      const files = await this.tab.api.stashDiff(this.tab.id, oid);
      if (mine !== this.diffSeq) return;
      this.diff = files;
      this.selectedFile = files[0] ? pathText(files[0].path) : null;
    } catch (e) {
      if (mine !== this.diffSeq) return;
      this.diffError = toApiError(e);
      if (this.diffError.code === 'STASH_NOT_FOUND') void this.load();
    } finally {
      if (mine === this.diffSeq) this.diffLoading = false;
    }
  }

  openPush(): void {
    this.pushOpen = true;
  }

  closePush(): void {
    this.pushOpen = false;
  }

  private async run<T>(label: string, title: string, fn: () => Promise<T>): Promise<T | undefined> {
    if (this.busy) return undefined;
    this.busy = label;
    try {
      return await fn();
    } catch (e) {
      const err = toApiError(e);
      if (err.code === 'NOTHING_TO_STASH') toasts.warning(t('stash.nothing'), t('stash.nothingBody'));
      else toasts.error(title, err);
      // La lista que vio el usuario ya no es la del repo.
      if (err.code === 'STASH_NOT_FOUND') void this.load();
      return undefined;
    } finally {
      this.busy = null;
    }
  }

  /** Guarda los cambios en un stash. `true` si se creó. */
  async push(options: StashPushOptions): Promise<boolean> {
    const entry = await this.run('push', t('stash.pushFailed'), () => this.tab.api.stashPush(this.tab.id, options));
    if (!entry) return false;
    this.pushOpen = false;
    toasts.success(t('stash.saved'), entry.message);
    toasts.announce(t('stash.saved'));
    await this.load();
    return true;
  }

  /** Aplica o saca un stash. Con conflictos avisa y abre el panel de Conflictos. */
  private async restore(kind: 'apply' | 'pop', oid: string): Promise<boolean> {
    const title = kind === 'apply' ? t('stash.applyFailed') : t('stash.popFailed');
    const api = this.tab.api;
    const res = await this.run(kind, title, () => (kind === 'apply' ? api.stashApply(this.tab.id, oid) : api.stashPop(this.tab.id, oid)));
    if (!res) return false;
    await this.load();
    if (res.conflicts.length > 0) {
      const names = res.conflicts.map((p) => pathText(p)).join(', ');
      toasts.warning(t('stash.conflictsTitle'), `${tn('stash.conflictsBody', res.conflicts.length)} ${names}`);
      toasts.announce(t('stash.conflictsTitle'));
      await this.tab.refresh();
      this.tab.panel = 'conflicts';
      return true;
    }
    toasts.success(kind === 'apply' ? t('stash.applied') : t('stash.popped'));
    toasts.announce(kind === 'apply' ? t('stash.applied') : t('stash.popped'));
    return true;
  }

  apply(oid: string): Promise<boolean> {
    return this.restore('apply', oid);
  }

  pop(oid: string): Promise<boolean> {
    return this.restore('pop', oid);
  }

  /** Borra tras la confirmación nativa de Rust; `false` si se cancela. */
  async drop(oid: string): Promise<boolean> {
    const res = await this.run('drop', t('stash.dropFailed'), () => this.tab.api.stashDrop(this.tab.id, oid));
    if (!res?.done) return false;
    toasts.success(t('stash.dropped'));
    toasts.announce(t('stash.dropped'));
    await this.load();
    return true;
  }
}

const registry = new WeakMap<TabState, StashController>();

export function stashOf(tab: TabState): StashController {
  let c = registry.get(tab);
  if (!c) {
    c = new StashController(tab);
    registry.set(tab, c);
  }
  return c;
}
