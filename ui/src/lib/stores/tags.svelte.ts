import type { Tag } from '$lib/api/tagApi';
import { toApiError } from '$lib/api/types';
import { t } from '$lib/i18n';
import { toasts } from './toasts.svelte';
import type { TabState } from './tab.svelte';

export type TagAction = 'create' | 'delete' | 'publish' | 'delete-remote';

/**
 * Etiquetas de UNA pestaña: lista, crear (en HEAD o en un commit; con mensaje es anotada),
 * borrar en local, publicar y borrar en el remoto. Las confirmaciones de borrado son diálogos
 * nativos de Rust (`done: false` = el usuario canceló). Una respuesta vieja no pisa a una nueva.
 */
export class TagsController {
  tags = $state.raw<Tag[]>([]);
  loaded = $state(false);
  /** Operación en curso como `acción:nombre`; solo una a la vez por pestaña. */
  busy = $state<string | null>(null);
  private gen = 0;

  constructor(readonly tab: TabState) {}

  isBusy(action?: TagAction, name?: string): boolean {
    if (this.busy === null) return false;
    if (!action) return true;
    return this.busy === `${action}:${name ?? ''}`;
  }

  /** Remoto por defecto para publicar: `origin` si existe, si no el primero. */
  get defaultRemote(): string | null {
    const remotes = this.tab.snapshot?.remotes ?? [];
    return (remotes.find((r) => r.name === 'origin') ?? remotes[0])?.name ?? null;
  }

  get remoteNames(): string[] {
    return (this.tab.snapshot?.remotes ?? []).map((r) => r.name);
  }

  /** Recarga la lista (nueva revisión del repo). */
  async load(): Promise<void> {
    const mine = ++this.gen;
    try {
      const tags = await this.tab.api.listTags(this.tab.id);
      if (mine !== this.gen) return;
      this.tags = tags;
    } catch (e) {
      if (mine === this.gen) {
        this.tags = [];
        toasts.error(t('tags.loadFailed'), toApiError(e));
      }
    } finally {
      if (mine === this.gen) this.loaded = true;
    }
  }

  private async run<R>(action: TagAction, name: string, title: string, fn: () => Promise<R>): Promise<R | undefined> {
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

  /** Crea una etiqueta en `target` (SHA; `null` = HEAD). Con `message` no vacío es anotada. */
  async create(name: string, target: string | null, message: string): Promise<boolean> {
    const clean = name.trim();
    const tag = await this.run('create', clean, t('tags.createFailed'), () =>
      this.tab.api.createTag(this.tab.id, clean, target, message.trim() === '' ? null : message.trim())
    );
    if (!tag) return false;
    toasts.success(t('tags.created', { name: tag.name }));
    await this.load();
    return true;
  }

  async remove(name: string): Promise<boolean> {
    const res = await this.run('delete', name, t('tags.deleteFailed', { name }), () => this.tab.api.deleteTag(this.tab.id, name));
    if (!res?.done) return false;
    toasts.success(t('tags.deleted', { name }));
    await this.load();
    return true;
  }

  async publish(remote: string, name: string): Promise<boolean> {
    const ok = await this.run('publish', name, t('tags.publishFailed', { name }), async () => {
      await this.tab.api.pushTag(this.tab.id, remote, name);
      return true;
    });
    if (ok) toasts.success(t('tags.published', { name, remote }));
    return ok === true;
  }

  async removeRemote(remote: string, name: string): Promise<boolean> {
    const res = await this.run('delete-remote', name, t('tags.deleteRemoteFailed', { name }), () =>
      this.tab.api.deleteRemoteTag(this.tab.id, remote, name)
    );
    if (!res?.done) return false;
    toasts.success(t('tags.deletedRemote', { name, remote }));
    return true;
  }
}

const registry = new WeakMap<TabState, TagsController>();

export function tagsOf(tab: TabState): TagsController {
  let c = registry.get(tab);
  if (!c) {
    c = new TagsController(tab);
    registry.set(tab, c);
  }
  return c;
}
