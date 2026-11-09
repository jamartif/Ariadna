import type { Api } from '$lib/api/api';
import {
  ApiError,
  toApiError,
  type DiffHunk,
  type GitPath,
  type Snapshot,
  type SessionEvent,
  type StatusEntry,
  type TabInfo
} from '$lib/api/types';
import { t } from '$lib/i18n';
import { groupEntries, SECTION_ORDER, type SectionId } from '$lib/util/changes';
import { pathKey, pathText } from '$lib/util/gitpath';
import { toasts } from './toasts.svelte';

export interface Selection {
  section: SectionId;
  key: string;
  path: GitPath;
}

export type Panel = 'changes' | 'history';

const DRAFT_PREFIX = 'ariadna.draft:';

/**
 * Estado de UNA pestaña de repositorio. La verdad vive en Rust: esto es una vista
 * del último `Snapshot` con revisión mayor. Un snapshot con revisión menor o igual
 * que la vigente se descarta (ipc.md, «Revisión»).
 */
export class TabState {
  info = $state.raw<TabInfo>() as TabInfo;
  snapshot = $state.raw<Snapshot | null>(null);
  loading = $state(true);
  sessionError = $state.raw<ApiError | null>(null);
  panel = $state<Panel>('changes');
  selected = $state.raw<Selection | null>(null);
  showIgnored = $state(false);
  filter = $state('');
  /** Claves `sección:ruta` con una operación en curso hasta el próximo snapshot. */
  pending = $state.raw<ReadonlySet<string>>(new Set());
  operations = $state.raw<{ id: number; label: string }[]>([]);
  message = $state('');
  amend = $state(false);
  /** Mensaje que había antes de activar «amend», para restaurarlo al desactivar. */
  private savedMessage = '';
  committing = $state(false);

  /** Descartados (para tests y depuración). */
  discardedSnapshots = 0;

  private selectedIndex = 0;
  private pendingTimer: ReturnType<typeof setTimeout> | null = null;
  private unlisten: (() => void)[] = [];
  private disposed = false;

  readonly sections = $derived(groupEntries(this.snapshot?.status.entries ?? []));
  readonly revision = $derived(this.snapshot?.revision ?? 0);

  constructor(
    readonly api: Api,
    info: TabInfo
  ) {
    this.info = info;
    try {
      this.message = localStorage.getItem(DRAFT_PREFIX + info.path) ?? '';
    } catch {
      /* sin almacenamiento */
    }
  }

  get id(): string {
    return this.info.tabId;
  }

  get stagedCount(): number {
    return this.sections.staged.length;
  }

  // ---- Ciclo de vida ------------------------------------------------------------

  async attach(): Promise<void> {
    const offs = await Promise.all([
      this.api.listen('snapshot', (p) => {
        if (p.tabId === this.id) this.applySnapshot(p.snapshot);
      }),
      this.api.listen('tab-updated', (p) => {
        if (p.tabId === this.id) this.updateInfo(p);
      }),
      this.api.listen('session-event', (ev) => {
        if (ev.sessionId === this.id) this.onSessionEvent(ev);
      })
    ]);
    if (this.disposed) offs.forEach((f) => f());
    else this.unlisten = offs;
  }

  dispose(): void {
    this.disposed = true;
    this.unlisten.forEach((f) => f());
    this.unlisten = [];
    if (this.pendingTimer) clearTimeout(this.pendingTimer);
  }

  async load(): Promise<void> {
    this.loading = true;
    this.sessionError = null;
    try {
      const snap = await this.api.getSnapshot(this.id);
      this.applySnapshot(snap);
    } catch (e) {
      this.sessionError = toApiError(e);
    } finally {
      this.loading = false;
    }
  }

  updateInfo(info: TabInfo): void {
    this.info = info;
    if (info.missing && !this.sessionError) {
      this.sessionError = new ApiError({ code: 'REPO_MISSING', message: t('banner.missing', { path: info.path }) });
    }
  }

  private onSessionEvent(ev: SessionEvent): void {
    const k = ev.kind;
    switch (k.type) {
      case 'operationStarted':
        this.operations = [...this.operations, { id: k.operationId, label: k.label }];
        toasts.announce(k.label);
        break;
      case 'operationFinished':
        this.operations = this.operations.filter((o) => o.id !== k.operationId);
        break;
      case 'refreshFailed':
        this.sessionError = toApiError(k.error);
        break;
      default:
        break;
    }
  }

  /** Aplica un snapshot. Devuelve `false` si se descartó por tener revisión antigua. */
  applySnapshot(s: Snapshot): boolean {
    const current = this.snapshot;
    if (current && s.revision <= current.revision) {
      this.discardedSnapshots++;
      return false;
    }
    this.snapshot = s;
    this.sessionError = null;
    this.loading = false;
    this.clearPending();
    this.reconcileSelection();
    return true;
  }

  // ---- Selección ---------------------------------------------------------------------

  select(section: SectionId, entry: StatusEntry): void {
    const list = this.sections[section];
    this.selectedIndex = Math.max(0, list.indexOf(entry));
    this.selected = { section, key: pathKey(entry.path), path: entry.path };
  }

  isSelected(section: SectionId, entry: StatusEntry): boolean {
    return this.selected?.section === section && this.selected.key === pathKey(entry.path);
  }

  private visibleSections(): SectionId[] {
    return SECTION_ORDER.filter((s) => s !== 'ignored' || this.showIgnored);
  }

  private reconcileSelection(): void {
    const sel = this.selected;
    const secs = this.sections;
    const has = (s: SectionId, key: string) => secs[s].some((e) => pathKey(e.path) === key);
    if (sel) {
      if (has(sel.section, sel.key)) return;
      // El archivo cambió de sección (p. ej. se preparó): se sigue su ruta.
      const moved = this.visibleSections().find((s) => s !== 'conflicts' && has(s, sel.key));
      if (moved) {
        const entry = secs[moved].find((e) => pathKey(e.path) === sel.key) as StatusEntry;
        this.select(moved, entry);
        return;
      }
      // Desapareció: el vecino en la misma sección.
      const list = secs[sel.section];
      const neighbor = list[Math.min(this.selectedIndex, list.length - 1)];
      if (neighbor) {
        this.select(sel.section, neighbor);
        return;
      }
    }
    const first = this.visibleSections()
      .filter((s) => s !== 'conflicts')
      .map((s) => ({ s, e: secs[s][0] }))
      .find((x) => x.e);
    this.selected = null;
    if (first?.e) this.select(first.s, first.e);
  }

  // ---- Operaciones -------------------------------------------------------------------

  private markPending(keys: string[]): void {
    this.pending = new Set([...this.pending, ...keys]);
  }

  private clearPending(): void {
    if (this.pending.size) this.pending = new Set();
    if (this.pendingTimer) clearTimeout(this.pendingTimer);
    this.pendingTimer = null;
  }

  private armPendingFallback(): void {
    if (this.pendingTimer) clearTimeout(this.pendingTimer);
    this.pendingTimer = setTimeout(() => this.clearPending(), 2500);
  }

  /** Ejecuta una mutación mostrando el error como toast; refresca si el hunk ya no existe. */
  private async run<T>(title: string, keys: string[], fn: () => Promise<T>): Promise<T | undefined> {
    this.markPending(keys);
    try {
      const out = await fn();
      this.armPendingFallback();
      return out;
    } catch (e) {
      this.clearPending();
      const err = toApiError(e);
      toasts.error(title, err);
      if (err.code === 'HUNK_NOT_FOUND') void this.refresh();
      return undefined;
    }
  }

  async stage(section: SectionId, entries: StatusEntry[]): Promise<void> {
    if (entries.length === 0) return;
    await this.run(
      t('op.stageFailed'),
      entries.map((e) => `${section}:${pathKey(e.path)}`),
      () => this.api.stage(this.id, entries.map((e) => e.path))
    );
  }

  async unstage(entries: StatusEntry[]): Promise<void> {
    if (entries.length === 0) return;
    await this.run(
      t('op.unstageFailed'),
      entries.map((e) => `staged:${pathKey(e.path)}`),
      () => this.api.unstage(this.id, entries.map((e) => e.path))
    );
  }

  async stageAll(): Promise<void> {
    const s = this.sections;
    await this.stage('unstaged', [...s.unstaged, ...s.untracked]);
  }

  async unstageAll(): Promise<void> {
    await this.unstage(this.sections.staged);
  }

  async discard(section: SectionId, entries: StatusEntry[]): Promise<boolean> {
    if (entries.length === 0) return false;
    const res = await this.run(
      t('op.discardFailed'),
      entries.map((e) => `${section}:${pathKey(e.path)}`),
      () => this.api.discard(this.id, entries.map((e) => e.path))
    );
    if (!res || !res.done) {
      this.clearPending();
      return false;
    }
    toasts.announce(t('op.discarded', { n: entries.length }));
    return true;
  }

  async stageHunk(path: GitPath, hunk: DiffHunk): Promise<void> {
    await this.run(t('op.stageHunkFailed'), [], () => this.api.stageHunk(this.id, path, hunk));
  }

  async unstageHunk(path: GitPath, hunk: DiffHunk): Promise<void> {
    await this.run(t('op.unstageHunkFailed'), [], () => this.api.unstageHunk(this.id, path, hunk));
  }

  async refresh(): Promise<void> {
    try {
      const s = await this.api.refresh(this.id);
      if (s) this.applySnapshot(s);
    } catch (e) {
      this.sessionError = toApiError(e);
    }
  }

  // ---- Commit ------------------------------------------------------------------------

  setMessage(m: string): void {
    this.message = m;
    try {
      if (m === '') localStorage.removeItem(DRAFT_PREFIX + this.info.path);
      else localStorage.setItem(DRAFT_PREFIX + this.info.path, m);
    } catch {
      /* ignorar */
    }
  }

  /** Activa/desactiva «corregir último commit»: rellena el mensaje con `amend_draft`. */
  async setAmend(on: boolean): Promise<void> {
    if (!on) {
      this.amend = false;
      this.setMessage(this.savedMessage);
      return;
    }
    try {
      const draft = await this.api.amendDraft(this.id);
      if (!draft.canAmend) {
        this.amend = false;
        toasts.warning(t('commit.cannotAmend'), draft.reason ?? undefined);
        return;
      }
      this.savedMessage = this.message;
      this.amend = true;
      this.setMessage(draft.message);
    } catch (e) {
      this.amend = false;
      toasts.error(t('commit.cannotAmend'), e);
    }
  }

  /** Crea el commit (o lo corrige). Devuelve `true` si se hizo. */
  async submit(): Promise<boolean> {
    if (this.committing) return false;
    const message = this.message;
    if (message.trim() === '') return false;
    this.committing = true;
    try {
      if (this.amend) {
        const sha = await this.api.amend(this.id, message);
        toasts.success(t('commit.amended'), sha.slice(0, 7));
        toasts.announce(t('commit.amended'));
      } else {
        const res = await this.api.commit(this.id, message, false);
        if (!res.done) return false;
        toasts.success(t('commit.created'), (res.sha ?? '').slice(0, 7));
        toasts.announce(t('commit.created'));
      }
      this.amend = false;
      this.savedMessage = '';
      this.setMessage('');
      return true;
    } catch (e) {
      toasts.error(t('commit.failed'), e);
      return false;
    } finally {
      this.committing = false;
    }
  }

  displayPath(p: GitPath): string {
    return pathText(p);
  }
}
