<script lang="ts">
  import { onMount } from 'svelte';
  import { t, tn } from '$lib/i18n';
  import type { Commit } from '$lib/api/types';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { historyOf } from '$lib/stores/history.svelte';
  import { relativeTime } from '$lib/util/time';
  import Banner from './Banner.svelte';
  import CommitGraph from './CommitGraph.svelte';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';
  import VirtualList from './VirtualList.svelte';

  let { tab }: { tab: TabState } = $props();

  const h = $derived(historyOf(tab));
  const ROW_H = 46;
  let list = $state<ReturnType<typeof VirtualList<Commit>>>();

  onMount(() => {
    void historyOf(tab).reload();
  });

  // Un commit, checkout o merge (nueva revisión) recarga la primera página.
  $effect(() => {
    void tab.revision;
    historyOf(tab).syncWithRevision();
  });

  function onScroll(top: number) {
    // Cerca del final → siguiente página.
    if (top > h.commits.length * ROW_H - 1400) void h.loadMore();
  }

  async function key(e: KeyboardEvent) {
    const step = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : e.key === 'PageDown' ? 10 : e.key === 'PageUp' ? -10 : 0;
    if (e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      await h.move(e.key === 'Home' ? -h.commits.length : h.commits.length);
    } else if (step !== 0) {
      e.preventDefault();
      await h.move(step);
    } else return;
    const i = h.selected ? h.commits.findIndex((c) => c.sha === h.selected?.sha) : -1;
    if (i >= 0) list?.scrollToIndex(i);
  }

  /** Referencias como etiquetas cortas: `HEAD -> main`, `origin/main`, `tag: v1`. */
  function refChips(c: Commit): { text: string; kind: 'head' | 'branch' | 'remote' | 'tag' }[] {
    return c.refs.map((r) => {
      if (r.startsWith('HEAD -> ')) return { text: r.slice(8), kind: 'head' as const };
      if (r === 'HEAD') return { text: 'HEAD', kind: 'head' as const };
      if (r.startsWith('tag: ')) return { text: r.slice(5), kind: 'tag' as const };
      return { text: r, kind: r.includes('/') ? ('remote' as const) : ('branch' as const) };
    });
  }

  const activeId = $derived(h.selected ? `commit-${h.selected.sha.slice(0, 12)}` : undefined);
</script>

<section class="history" aria-label={t('history.region')}>
  <div class="search">
    <Icon name="search" size={14} />
    <input
      class="field"
      type="search"
      value={h.query}
      oninput={(e) => h.setQuery(e.currentTarget.value)}
      placeholder={t('history.searchPlaceholder')}
      aria-label={t('history.search')}
      autocomplete="off"
      spellcheck="false"
    />
    {#if h.loading}<span class="spin" title={t('history.loading')}><Icon name="loader" size={14} spin /></span>{/if}
  </div>

  {#if h.error}
    <div class="pad">
      <Banner kind="error" title={t('history.errorTitle')} message={h.error.message} detail={h.error.detail}>
        {#snippet actions()}
          <button type="button" class="btn" onclick={() => h.reload()}>{t('common.retry')}</button>
        {/snippet}
      </Banner>
    </div>
  {:else if h.loading && h.commits.length === 0}
    <div class="skeletons" aria-busy="true" aria-label={t('history.loading')}>
      {#each [70, 55, 82, 48, 66, 74] as w, i (i)}<span class="skeleton" style:width="{w}%" style:height="14px"></span>{/each}
    </div>
  {:else if h.commits.length === 0}
    {#if h.searching}
      <EmptyState compact icon="search" title={t('history.noResultsTitle')} body={t('history.noResultsBody', { query: h.query.trim() })} />
    {:else}
      <EmptyState compact icon="history" title={t('history.emptyTitle')} body={t('history.emptyBody')} />
    {/if}
  {:else}
    <div class="count" aria-live="polite">
      {#if h.searching}{tn('history.results', h.commits.length)}{h.hasMore ? '+' : ''}{:else}{tn('history.loaded', h.commits.length)}{h.hasMore ? '+' : ''}{/if}
    </div>
    <div class="list">
      <VirtualList
        bind:this={list}
        items={h.commits}
        heightOf={() => ROW_H}
        role="listbox"
        label={t('history.list')}
        rowCount={h.commits.length}
        tabindex={0}
        activeDescendant={activeId}
        overscan={10}
        onscroll={onScroll}
        onkeydown={key}
      >
        {#snippet row(c, i)}
          {@const sel = h.selected?.sha === c.sha}
          <div
            id="commit-{c.sha.slice(0, 12)}"
            class="row"
            class:sel
            role="option"
            aria-selected={sel}
            aria-posinset={i + 1}
            aria-setsize={h.commits.length}
            style:height="{ROW_H}px"
            tabindex="-1"
            onclick={() => h.select(c)}
            onkeydown={(e) => (e.key === 'Enter' ? h.select(c) : undefined)}
          >
            {#if !h.searching && h.rows[i]}
              <CommitGraph row={h.rows[i]} columns={h.graphWidth} height={ROW_H} merge={c.parents.length > 1} />
            {/if}
            <div class="main">
              <div class="line1">
                <span class="subject">{c.subject}</span>
                {#each refChips(c).slice(0, 3) as r (r.text + r.kind)}
                  <span class="ref {r.kind}" title={r.text}>
                    <Icon name={r.kind === 'tag' ? 'tag' : r.kind === 'remote' ? 'cloud' : 'git-branch'} size={10} />{r.text}
                  </span>
                {/each}
                {#if c.refs.length > 3}<span class="ref more">+{c.refs.length - 3}</span>{/if}
              </div>
              <div class="line2">
                <span class="author">{c.authorName}</span>
                <span class="dot" aria-hidden="true">·</span>
                <time datetime={c.authorDateIso} title={new Date(c.authorDateIso).toLocaleString('es-ES')}>{relativeTime(c.authorDateIso)}</time>
                <span class="spacer"></span>
                <code class="sha mono" title={c.sha}>{c.sha.slice(0, 7)}</code>
              </div>
            </div>
          </div>
        {/snippet}
      </VirtualList>
    </div>
    {#if h.loadingMore}<div class="more-loading" role="status"><Icon name="loader" size={13} spin />{t('history.loadingMore')}</div>{/if}
  {/if}
</section>

<style>
  .history {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .search {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--border);
    color: var(--text-muted);
  }
  .search .field {
    flex: 1;
    min-width: 0;
  }
  .spin {
    display: inline-flex;
    color: var(--accent);
  }
  .count {
    padding: var(--space-1) var(--space-3);
    font-size: var(--text-xs);
    color: var(--text-subtle);
    border-bottom: 1px solid var(--border);
  }
  .list {
    flex: 1;
    min-height: 0;
  }
  .pad,
  .skeletons {
    padding: var(--space-4);
  }
  .skeletons {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .row {
    display: flex;
    align-items: stretch;
    gap: var(--space-2);
    padding: 0 var(--space-3) 0 var(--space-2);
    border-bottom: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
    cursor: default;
    outline: none;
  }
  .row:hover {
    background: var(--bg-hover);
  }
  .row.sel {
    background: var(--bg-selected);
  }
  /* Colores forzados: el commit seleccionado no puede depender del fondo. */
  @media (forced-colors: active) {
    .row.sel {
      outline: 2px solid Highlight;
      outline-offset: -2px;
    }
  }
  .row:focus-visible {
    box-shadow: inset 0 0 0 2px var(--focus-ring);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 2px;
  }
  .line1,
  .line2 {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .subject {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-sm);
    font-weight: 560;
    color: var(--text);
  }
  .line2 {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .author {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 40%;
  }
  .spacer {
    flex: 1;
  }
  .sha {
    color: var(--text-subtle);
    font-size: var(--text-xs);
  }
  .ref {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    flex: none;
    max-width: 140px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 0 6px;
    height: 18px;
    border-radius: var(--pill);
    font-size: var(--text-2xs);
    font-weight: 600;
    background: var(--bg-muted);
    color: var(--text-muted);
  }
  .ref.head {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .ref.remote {
    background: var(--info-soft);
    color: var(--info);
  }
  .ref.tag {
    background: var(--warning-soft);
    color: var(--warning);
  }
  .more-loading {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-top: 1px solid var(--border);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
</style>
