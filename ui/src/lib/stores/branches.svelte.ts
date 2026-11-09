import type { Branch, MergeFastForward } from '$lib/api/types';
import { toApiError } from '$lib/api/types';
import { t } from '$lib/i18n';
import { toasts } from './toasts.svelte';
import type { TabState } from './tab.svelte';

/** Lo que el panel de ramas está esperando que el usuario confirme. */
export type BranchPrompt =
  | { kind: 'checkout-dirty'; name: string; changes: number }
  | { kind: 'merge'; name: string }
  | null;

/**
 * Acciones sobre ramas de UNA pestaña. Las confirmaciones destructivas (borrar, divergencia)
 * las abre Rust como diálogo nativo; aquí solo el aviso previo de «tienes cambios» al cambiar
 * de rama. El estado de las ramas llega por el `snapshot` que Rust publica tras cada mutación.
 */
export class BranchesController {
  selected = $state<string | null>(null);
  busy = $state<string | null>(null);
  prompt = $state.raw<BranchPrompt>(null);
  /** Último fallo de una fusión por conflictos (se muestra junto al detalle de la rama). */
  conflict = $state.raw<{ branch: string; message: string; files: string[] } | null>(null);

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
    this.conflict = null;
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

  /** Fusiona sin conflictos; si los hay, Rust aborta y aquí se explica. */
  async merge(name: string, fastForward: MergeFastForward = 'allow'): Promise<boolean> {
    this.prompt = null;
    this.conflict = null;
    if (this.busy) return false;
    this.busy = 'merge';
    try {
      const res = await this.tab.api.mergeClean(this.tab.id, name, fastForward);
      toasts.success(res === 'upToDate' ? t('branches.mergeUpToDate', { name }) : t('branches.merged', { name }));
      return true;
    } catch (e) {
      const err = toApiError(e);
      if (err.code === 'MERGE_CONFLICTS') {
        this.conflict = { branch: name, message: err.message, files: (err.detail ?? '').split('\n').filter((l) => l !== '') };
      } else {
        toasts.error(t('branches.mergeFailed'), err);
      }
      return false;
    } finally {
      this.busy = null;
    }
  }

  cancelPrompt(): void {
    this.prompt = null;
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
