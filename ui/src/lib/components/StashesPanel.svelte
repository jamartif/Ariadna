<script lang="ts">
  import { t, tn } from '$lib/i18n';
  import type { StashEntry } from '$lib/api/types';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { stashOf } from '$lib/stores/stash.svelte';
  import Banner from './Banner.svelte';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';
  import StashPushForm from './StashPushForm.svelte';

  /** Columna central de Stashes: guardar cambios y lista de stashes del repositorio. */
  let { tab }: { tab: TabState } = $props();

  const s = $derived(stashOf(tab));

  $effect(() => {
    // El espacio de trabajo relee la lista con cada snapshot; si el panel se monta solo, la pide.
    if (!s.loaded) void s.load();
  });

  // Con el panel visible siempre hay un stash seleccionado si existe alguno.
  $effect(() => {
    if (s.loaded && s.selectedOid === null && s.list[0]) void s.select(s.list[0].oid);
  });

  function when(iso: string): string {
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? '' : d.toLocaleString('es-ES', { dateStyle: 'medium', timeStyle: 'short' });
  }

  function where(e: StashEntry): string {
    return e.branch ? t('stash.onBranch', { branch: e.branch }) : t('stash.detached');
  }

  function listKey(e: KeyboardEvent, i: number) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault();
    const next = s.list[Math.min(s.list.length - 1, Math.max(0, i + (e.key === 'ArrowDown' ? 1 : -1)))];
    if (!next) return;
    void s.select(next.oid);
    queueMicrotask(() => document.getElementById(`stash-item-${next.oid}`)?.focus());
  }
</script>

<section class="stashes" aria-label={t('stash.region')}>
  <div class="toolbar">
    <button type="button" class="btn btn-primary" onclick={() => s.openPush()} disabled={s.pushOpen}>
      <Icon name="archive" size={14} />{t('stash.save')}
    </button>
  </div>

  {#if s.pushOpen}<StashPushForm {tab} />{/if}

  <div class="scroll">
    {#if s.loadError}
      <Banner kind="error" title={t('stash.loadFailed')} message={s.loadError.message} detail={s.loadError.detail}>
        {#snippet actions()}
          <button type="button" class="btn" onclick={() => s.load()}>{t('common.retry')}</button>
        {/snippet}
      </Banner>
    {:else if !s.loaded}
      <div class="sk" aria-busy="true">
        {#each [70, 55, 80] as w, i (i)}<span class="skeleton" style:width="{w}%" style:height="16px"></span>{/each}
      </div>
    {:else if s.list.length === 0}
      <EmptyState compact icon="archive" title={t('stash.emptyTitle')} body={t('stash.emptyBody')} />
    {:else}
      <h3><span>{t('stash.list')}</span><span class="chip">{s.list.length}</span></h3>
      <ul aria-label={t('stash.list')}>
        {#each s.list as e, i (e.oid)}
          {@const on = s.selectedOid === e.oid}
          <li>
            <button
              type="button"
              id="stash-item-{e.oid}"
              class="item"
              class:sel={on}
              aria-pressed={on}
              tabindex={on || (s.selectedOid === null && i === 0) ? 0 : -1}
              onclick={() => s.select(e.oid)}
              onkeydown={(ev) => listKey(ev, i)}
            >
              <Icon name="archive" size={14} />
              <span class="body">
                <span class="msg" class:auto={e.autoMessage}>{e.message}</span>
                <span class="meta">
                  <span class="mono">stash@&#123;{e.index}&#125;</span>
                  <span>{where(e)}</span>
                  {#if when(e.createdAtIso)}<span>{when(e.createdAtIso)}</span>{/if}
                </span>
              </span>
            </button>
          </li>
        {/each}
      </ul>
      <p class="count sr-only" role="status">{tn('stash.chip', s.list.length)}</p>
    {/if}
  </div>
</section>

<style>
  .stashes {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .toolbar {
    display: flex;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--border);
    align-items: center;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--space-2);
  }
  .sk {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3);
  }
  h3 {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: var(--space-2) var(--space-2) var(--space-1);
    font-size: var(--text-xs);
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 2px;
  }
  .item {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-2);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }
  .item:hover {
    background: var(--bg-hover);
  }
  .item.sel {
    background: var(--bg-selected);
    border-color: var(--border-hover);
  }
  .item:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
  }
  .body {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .msg {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }
  .msg.auto {
    font-weight: 500;
    color: var(--text-muted);
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
</style>
