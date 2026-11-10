import type { Branch, RemoteProviderId, RemoteView, WebTarget } from '$lib/api/types';
import { toApiError } from '$lib/api/types';
import { t, tn } from '$lib/i18n';
import type { RemoteProvider } from '$lib/util/remote';
import { toasts } from './toasts.svelte';
import type { TabState } from './tab.svelte';

/** Proveedor de `ariadna-hosting` → marca de `ProviderMark`. */
export function markOf(provider: RemoteProviderId): RemoteProvider {
  return provider === 'azureDevOps' ? 'azure' : provider;
}

export type RemoteAction = 'fetch' | 'pull' | 'push' | 'add' | 'rename' | 'remove' | 'open';

/** Lo que se abrirá en el navegador para una rama: remoto + nombre de rama sin prefijo. */
export interface WebLink {
  remote: RemoteView;
  branch: string;
}

/**
 * Remotos de UNA pestaña: lista con proveedor (la calcula Rust), fetch/pull/push, añadir /
 * renombrar / quitar (la confirmación es un diálogo nativo de Rust: `done: false` = cancelado) y
 * «Abrir en el navegador» / «Crear pull request» (solo enlaces; Rust valida https y el host).
 */
export class RemotesController {
  views = $state.raw<RemoteView[]>([]);
  loaded = $state(false);
  /** Operación en curso como `acción:remoto`; solo una a la vez por pestaña. */
  busy = $state<string | null>(null);
  private gen = 0;

  constructor(readonly tab: TabState) {}

  isBusy(action?: RemoteAction, name?: string): boolean {
    if (this.busy === null) return false;
    if (!action) return true;
    return this.busy === `${action}:${name ?? ''}`;
  }

  /** Recarga la lista (el snapshot cambió sus remotos). Una respuesta vieja no pisa a una nueva. */
  async load(): Promise<void> {
    const mine = ++this.gen;
    try {
      const views = await this.tab.api.listRemotes(this.tab.id);
      if (mine !== this.gen) return;
      this.views = views;
    } catch {
      if (mine === this.gen) this.views = [];
    } finally {
      if (mine === this.gen) this.loaded = true;
    }
  }

  get current(): Branch | undefined {
    return (this.tab.snapshot?.branches ?? []).find((b) => b.isHead);
  }

  /** `true` si hay una rama local actual que bajar/subir. */
  get hasCurrent(): boolean {
    return this.current !== undefined && !this.tab.info.detached;
  }

  private async run<T>(action: RemoteAction, name: string, title: string, fn: () => Promise<T>): Promise<T | undefined> {
    if (this.busy) return undefined;
    this.busy = `${action}:${name}`;
    try {
      return await fn();
    } catch (e) {
      toasts.error(title, toApiError(e));
      return undefined;
    } finally {
      this.busy = null;
    }
  }

  async fetch(name: string): Promise<boolean> {
    const ok = await this.run('fetch', name, t('remotes.fetchFailed', { name }), async () => {
      await this.tab.api.fetch(this.tab.id, name);
      return true;
    });
    if (ok) toasts.success(t('remotes.fetched', { name }));
    return ok === true;
  }

  /** Baja la rama actual. Si no sigue a ese remoto se pide expresamente `remoto rama`. */
  async pull(name: string): Promise<boolean> {
    const cur = this.current;
    if (!cur || this.tab.info.detached) return false;
    const refspec = cur.upstream?.startsWith(`${name}/`) ? null : cur.name;
    const res = await this.run('pull', name, t('remotes.pullFailed', { name }), () => this.tab.api.pull(this.tab.id, name, refspec, null));
    if (res === undefined) return false;
    if (typeof res === 'object' && 'conflicts' in res) {
      toasts.warning(t('branches.mergeStarted'), tn('conflicts.banner.pending', res.conflicts.paths.length));
      this.tab.panel = 'conflicts';
      return false;
    }
    toasts.success(res === 'upToDate' ? t('remotes.pullUpToDate', { name }) : t('remotes.pulled', { name }));
    return true;
  }

  /** Sube la rama actual; si todavía no tiene origen, la publica y lo fija (`--set-upstream`). */
  async push(name: string): Promise<boolean> {
    const cur = this.current;
    if (!cur || this.tab.info.detached) return false;
    const setUpstream = !cur.upstream;
    const ok = await this.run('push', name, t('remotes.pushFailed', { name }), async () => {
      await this.tab.api.push(this.tab.id, name, cur.name, setUpstream);
      return true;
    });
    if (ok) toasts.success(t('remotes.pushed', { branch: cur.name, name }));
    return ok === true;
  }

  async add(name: string, url: string): Promise<boolean> {
    const res = await this.run('add', name, t('remotes.addFailed'), () => this.tab.api.addRemote(this.tab.id, name.trim(), url.trim()));
    if (!res?.done) return false;
    toasts.success(t('remotes.added', { name: name.trim() }));
    await this.load();
    return true;
  }

  async rename(oldName: string, newName: string): Promise<boolean> {
    const res = await this.run('rename', oldName, t('remotes.renameFailed'), () => this.tab.api.renameRemote(this.tab.id, oldName, newName.trim()));
    if (!res?.done) return false;
    toasts.success(t('remotes.renamed', { old: oldName, name: newName.trim() }));
    await this.load();
    return true;
  }

  async remove(name: string): Promise<boolean> {
    const res = await this.run('remove', name, t('remotes.removeFailed'), () => this.tab.api.removeRemote(this.tab.id, name));
    if (!res?.done) return false;
    toasts.success(t('remotes.removed', { name }));
    await this.load();
    return true;
  }

  /** Abre una página del proveedor. Rust construye la URL y valida https + host del remoto. */
  async open(remote: string, kind: WebTarget, branch: string | null = null): Promise<boolean> {
    const ok = await this.run('open', remote, t('remotes.openFailed'), async () => {
      await this.tab.api.openRemoteUrl(this.tab.id, remote, kind, branch);
      return true;
    });
    return ok === true;
  }

  // ---- Enlaces por rama --------------------------------------------------------------------

  /**
   * Remoto y nombre de rama para los enlaces web de una rama: la remota `origin/x` usa `origin` y
   * `x`; una local usa el remoto de su upstream, o `origin`, o el primero que tenga web.
   */
  linkFor(branchName: string, isRemoteBranch: boolean): WebLink | null {
    const views = this.views;
    if (views.length === 0) return null;
    if (isRemoteBranch) {
      const owner = views.filter((v) => branchName.startsWith(`${v.name}/`)).sort((a, b) => b.name.length - a.name.length)[0];
      return owner ? { remote: owner, branch: branchName.slice(owner.name.length + 1) } : null;
    }
    const local = (this.tab.snapshot?.branches ?? []).find((b) => b.name === branchName);
    const upstream = local?.upstream ?? null;
    const viaUpstream = upstream ? views.find((v) => upstream.startsWith(`${v.name}/`)) : undefined;
    const remote = viaUpstream ?? views.find((v) => v.name === 'origin' && v.canOpen) ?? views.find((v) => v.canOpen) ?? null;
    return remote ? { remote, branch: branchName } : null;
  }
}

const registry = new WeakMap<TabState, RemotesController>();

export function remotesOf(tab: TabState): RemotesController {
  let c = registry.get(tab);
  if (!c) {
    c = new RemotesController(tab);
    registry.set(tab, c);
  }
  return c;
}
