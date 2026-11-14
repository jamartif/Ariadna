import type { Branch, FileDiff, MergeFastForward } from '$lib/api/types';
import { toApiError, type ApiError } from '$lib/api/types';
import { t, tn } from '$lib/i18n';
import { toasts } from './toasts.svelte';
import type { TabState } from './tab.svelte';

/** Lo que el panel de ramas está esperando que el usuario confirme. */
export type BranchPrompt =
  | { kind: 'checkout-dirty'; name: string; changes: number }
  | { kind: 'merge'; name: string }
  | null;

/** Diff de una rama contra otra (`base...head`), tal como lo pinta el detalle de rama. */
export interface BranchDiff {
  base: string;
  head: string;
  files: FileDiff[];
}

/**
 * Acciones sobre ramas de UNA pestaña. Las confirmaciones destructivas (borrar, divergencia)
 * las abre Rust como diálogo nativo; aquí solo el aviso previo de «tienes cambios» al cambiar
 * de rama. El estado de las ramas llega por el `snapshot` que Rust publica tras cada mutación.
 */
export class BranchesController {
  selected = $state<string | null>(null);
  busy = $state<string | null>(null);
  prompt = $state.raw<BranchPrompt>(null);

  constructor(readonly tab: TabState) {}

  get locals(): Branch[] {
    return this.tab.snapshot?.branches ?? [];
  }

  get remotes(): Branch[] {
    return this.tab.snapshot?.remoteBranches ?? [];
  }

  get current(): Branch | undefined {
    return this.locals.find((b) => b.isHead);
  }

  /** La rama seleccionada (si desapareció tras un snapshot, la actual). */
  get selectedBranch(): Branch | undefined {
    const all = [...this.locals, ...this.remotes];
    return all.find((b) => b.name === this.selected) ?? this.current;
  }

  isRemote(name: string): boolean {
    return this.remotes.some((b) => b.name === name) && !this.locals.some((b) => b.name === name);
  }

  select(name: string): void {
    this.selected = name;
  }

  private async run<T>(label: string, title: string, fn: () => Promise<T>): Promise<T | undefined> {
    if (this.busy) return undefined;
    this.busy = label;
    try {
      return await fn();
    } catch (e) {
      toasts.error(title, toApiError(e));
      return undefined;
    } finally {
      this.busy = null;
    }
  }

  async create(name: string, startPoint: string | null): Promise<boolean> {
    const n = name.trim();
    const ok = await this.run('create', t('branches.createFailed'), async () => {
      await this.tab.api.createBranch(this.tab.id, n, startPoint);
      return true;
    });
    if (!ok) return false;
    toasts.success(t('branches.created', { name: n }));
    this.selected = n;
    return true;
  }

  /** Cambia de rama. Con cambios sin confirmar pide antes un aviso (`prompt`). */
  async checkout(name: string, confirmed = false): Promise<boolean> {
    const changes = this.tab.info.changeCount;
    if (changes > 0 && !confirmed) {
      this.prompt = { kind: 'checkout-dirty', name, changes };
      return false;
    }
    this.prompt = null;
    const res = await this.run('checkout', t('branches.checkoutFailed'), () => this.tab.api.checkout(this.tab.id, name));
    if (!res?.done) return false;
    toasts.success(t('branches.switched', { name }));
    this.selected = this.isRemote(name) ? (name.split('/').slice(1).join('/') || name) : name;
    return true;
  }

  async rename(oldName: string, newName: string): Promise<boolean> {
    const n = newName.trim();
    const ok = await this.run('rename', t('branches.renameFailed'), async () => {
      await this.tab.api.renameBranch(this.tab.id, oldName, n);
      return true;
    });
    if (!ok) return false;
    toasts.success(t('branches.renamed', { old: oldName, name: n }));
    if (this.selected === oldName) this.selected = n;
    return true;
  }

  /** El diálogo de confirmación lo abre Rust (nativo); `done: false` = el usuario canceló. */
  async remove(name: string): Promise<boolean> {
    const res = await this.run('delete', t('branches.deleteFailed'), () => this.tab.api.deleteBranch(this.tab.id, name, false));
    if (!res?.done) return false;
    toasts.success(t('branches.deleted', { name }));
    if (this.selected === name) this.selected = null;
    return true;
  }

  askMerge(name: string): void {
    this.prompt = { kind: 'merge', name };
  }

  /**
   * Fusiona. Si hay conflictos el merge **queda en curso** y se pasa al panel «Conflictos»
   * (desde ahí se resuelven o se aborta). Devuelve `true` si la fusión terminó.
   */
  async merge(name: string, fastForward: MergeFastForward = 'allow'): Promise<boolean> {
    this.prompt = null;
    if (this.busy) return false;
    this.busy = 'merge';
    try {
      const { result } = await this.tab.api.mergeStart(this.tab.id, name, fastForward);
      if (typeof result === 'object' && 'conflicts' in result) {
        toasts.warning(t('branches.mergeStarted'), tn('conflicts.banner.pending', result.conflicts.paths.length));
        this.tab.panel = 'conflicts';
        return false;
      }
      toasts.success(result === 'upToDate' ? t('branches.mergeUpToDate', { name }) : t('branches.merged', { name }));
      return true;
    } catch (e) {
      toasts.error(t('branches.mergeFailed'), toApiError(e));
      return false;
    } finally {
      this.busy = null;
    }
  }

  cancelPrompt(): void {
    this.prompt = null;
  }

  // ---- Menú contextual del riel -------------------------------------------------------------

  /**
   * Publica una rama **cualquiera** (no solo la actual). Con una rama sin origen, Rust la
   * publica con `--set-upstream`: queda siguiendo su remoto.
   */
  async pushBranch(name: string): Promise<boolean> {
    const local = this.locals.find((x) => x.name === name);
    if (!local) return false;
    const setUpstream = !local.upstream;
    const res = await this.run('push', t('branches.pushFailed'), () => this.tab.api.pushBranch(this.tab.id, name, setUpstream));
    if (!res) return false;
    toasts.success(t('branches.pushed', { name, remote: res.remote }));
    return true;
  }

  /**
   * Rebasa la **rama actual** sobre `name`. Si hay conflictos el rebase queda en curso y se pasa
   * al panel de Conflictos (desde ahí se continúa o se aborta).
   */
  async rebase(name: string): Promise<boolean> {
    if (this.busy) return false;
    this.busy = 'rebase';
    try {
      const { done, result } = await this.tab.api.rebaseOnto(this.tab.id, name);
      if (!done) return false;
      if (typeof result === 'object' && 'conflicts' in result) {
        toasts.warning(t('branches.rebaseStarted'), tn('conflicts.banner.pending', result.conflicts.paths.length));
        this.tab.panel = 'conflicts';
        return false;
      }
      toasts.success(t('branches.rebased', { branch: name }));
      return true;
    } catch (e) {
      toasts.error(t('branches.rebaseFailed'), toApiError(e));
      return false;
    } finally {
      this.busy = null;
    }
  }

  /** Borra una rama en el remoto (`remoto/rama`). Confirmación nativa desde Rust. */
  async removeRemote(remote: string, name: string): Promise<boolean> {
    const res = await this.run('deleteRemote', t('branches.deleteRemoteFailed'), () =>
      this.tab.api.deleteRemoteBranch(this.tab.id, remote, name)
    );
    if (!res?.done) return false;
    toasts.success(t('branches.deletedRemote', { name: `${remote}/${name}` }));
    if (this.selected === `${remote}/${name}`) this.selected = null;
    return true;
  }

  /** Diff `base...head` que pinta el detalle de una rama («Ver diff contra la actual»). */
  diff = $state.raw<BranchDiff | null>(null);
  diffError = $state.raw<ApiError | null>(null);
  diffBusy = $state(false);
  private diffGen = 0;

  /** Carga (o limpia, si `base === head`) el diff de `head` contra `base`. */
  async compare(base: string, head: string): Promise<void> {
    const mine = ++this.diffGen;
    if (base === head) {
      this.diff = null;
      this.diffError = null;
      this.diffBusy = false;
      return;
    }
    this.diffBusy = true;
    try {
      const files = await this.tab.api.branchDiffAgainst(this.tab.id, base, head);
      if (mine !== this.diffGen) return;
      this.diff = { base, head, files };
      this.diffError = null;
    } catch (e) {
      if (mine !== this.diffGen) return;
      this.diff = null;
      this.diffError = toApiError(e);
    } finally {
      if (mine === this.diffGen) this.diffBusy = false;
    }
  }

  clearDiff(): void {
    this.diffGen++;
    this.diff = null;
    this.diffError = null;
    this.diffBusy = false;
  }
}

const registry = new WeakMap<TabState, BranchesController>();

export function branchesOf(tab: TabState): BranchesController {
  let b = registry.get(tab);
  if (!b) {
    b = new BranchesController(tab);
    registry.set(tab, b);
  }
  return b;
}
