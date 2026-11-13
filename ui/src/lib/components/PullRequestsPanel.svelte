<script lang="ts">
  import { onMount } from 'svelte';
  import { t, tn } from '$lib/i18n';
  import type { PullRequestSummary } from '$lib/api/prTypes';
  import { AUTO_REFRESH_OPTIONS, prOf } from '$lib/stores/pullRequests.svelte';
  import { remotesOf } from '$lib/stores/remotes.svelte';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { FILTERS, MERGE_LABEL, STATE_ICON, STATE_LABEL, STATE_TONE } from '$lib/util/pr';
  import { relativeTime } from '$lib/util/time';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';
  import PrBadge from './PrBadge.svelte';
  import PrError from './PrError.svelte';
  import PrReviewers from './PrReviewers.svelte';

  /** Lista de pull requests del remoto (columna central del panel «Pull requests»). */
  let { tab }: { tab: TabState } = $props();

  const pr = $derived(prOf(tab));
  const remotes = $derived(remotesOf(tab));
  const supported = $derived(pr.supported);
  const webRemote = $derived(remotes.views.find((r) => r.canOpen));

  onMount(() => {
    if (!pr.supportLoaded) void pr.loadSupport();
    if (!remotes.loaded) void remotes.load();
  });

  // Primera carga cuando se sabe que el remoto admite PR.
  $effect(() => {
    if (supported && !pr.listLoaded && !pr.listLoading) void pr.loadList();
  });

  // PR-09: refresco opcional a intervalo (≥ 60 s), solo mientras el panel está abierto.
  $effect(() => {
    const secs = pr.autoRefresh;
    if (!supported || secs < 60) return;
    const id = setInterval(() => void pr.refresh(), secs * 1000);
    return () => clearInterval(id);
  });

  function rowKey(e: KeyboardEvent) {
    const rows = [...(e.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>('button.row')];
    const i = rows.indexOf(document.activeElement as HTMLButtonElement);
    let n = i;
    if (e.key === 'ArrowDown') n = Math.min(rows.length - 1, i + 1);
    else if (e.key === 'ArrowUp') n = Math.max(0, i - 1);
    else if (e.key === 'Home') n = 0;
    else if (e.key === 'End') n = rows.length - 1;
    else return;
    e.preventDefault();
    rows[n]?.focus();
  }

  function checksTitle(p: PullRequestSummary): string {
    const c = p.checks;
    return c ? t('pr.checks.summary', { passing: c.passing, failing: c.failing, pending: c.pending }) : '';
  }

  const EMPTY = {
    open: 'pr.empty.open',
    mine: 'pr.empty.mine',
    reviewing: 'pr.empty.reviewing',
    closed: 'pr.empty.closed'
  } as const;

  const selectedFor = (id: number) => pr.selectedId === id && !pr.creating;
</script>

<section class="prs" aria-label={t('pr.region')}>
  {#if !pr.supportLoaded}
    <div class="loading" role="status" aria-busy="true">
      <Icon name="loader" size={16} spin />{t('pr.loadingSupport')}
    </div>
  {:else if pr.supportError}
    <div class="pad"><PrError error={pr.supportError} onretry={() => pr.loadSupport()} /></div>
  {:else if !supported}
    <EmptyState icon="git-merge" title={t('pr.unsupported.title')} body={pr.support?.reason ?? t('pr.unsupported.body')}>
      <p class="small">{t('pr.unsupported.body')}</p>
      {#if webRemote}
        <button type="button" class="btn" onclick={() => remotes.open(webRemote.name, 'repo', null)}>
          <Icon name="external-link" size={14} />{t('pr.unsupported.open')}
        </button>
      {/if}
    </EmptyState>
  {:else}
    <div class="toolbar">
      <div class="search">
        <Icon name="search" size={14} />
        <input class="field" type="search" bind:value={pr.query} placeholder={t('pr.search.placeholder')} aria-label={t('pr.search')} autocomplete="off" spellcheck="false" />
      </div>
      <button type="button" class="btn btn-icon" onclick={() => void pr.refresh()} disabled={pr.listLoading} aria-label={t('pr.refresh')} title={t('pr.refresh')}>
        <Icon name="refresh-cw" size={14} spin={pr.listLoading} />
      </button>
      <button type="button" class="btn btn-primary" onclick={() => pr.startCreate()}>
        <Icon name="plus" size={14} />{t('pr.new')}
      </button>
    </div>

    <div class="filters" role="group" aria-label={t('pr.filters')}>
      {#each FILTERS as f (f.id)}
        <button type="button" class="fbtn" class:on={pr.filter === f.id} aria-pressed={pr.filter === f.id} onclick={() => void pr.setFilter(f.id)}>{t(f.label)}</button>
      {/each}
    </div>

    <div class="scroll">
      {#if pr.listError}
        <div class="pad"><PrError error={pr.listError} onretry={() => pr.loadList()} /></div>
      {:else if pr.listLoading && pr.items.length === 0}
        <ul class="skeletons" role="status" aria-busy="true" aria-label={t('pr.loading')}>
          {#each [0, 1, 2, 3] as k (k)}<li class="skeleton"></li>{/each}
        </ul>
      {:else if pr.items.length === 0}
        <EmptyState compact icon="git-merge" title={t('pr.empty.title')} body={t(EMPTY[pr.filter])}>
          <button type="button" class="btn btn-primary" onclick={() => pr.startCreate()}><Icon name="plus" size={14} />{t('pr.new')}</button>
        </EmptyState>
      {:else if pr.visible.length === 0}
        <p class="none" role="status">{t('pr.noMatches', { query: pr.query.trim() })}</p>
      {:else}
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <ul class="list" aria-label={t('pr.list')} aria-busy={pr.listLoading} onkeydown={rowKey}>
          {#each pr.visible as p (p.id)}
            <li>
              <button type="button" class="row" class:sel={selectedFor(p.id)} aria-current={selectedFor(p.id) ? 'true' : undefined} onclick={() => pr.select(p.id)}>
                <span class="line1">
                  <span class="title">{p.title}</span>
                </span>
                <span class="line2">
                  <span class="num mono">#{p.id}</span>
                  <span>{p.author.displayName}</span>
                  <span aria-hidden="true">·</span>
                  <span title={p.updatedAt ?? p.createdAt}>{relativeTime(p.updatedAt ?? p.createdAt)}</span>
                </span>
                <span class="line3 mono"><Icon name="git-branch" size={11} /><span class="br">{p.sourceBranch}</span><Icon name="arrow-right" size={11} /><span class="br">{p.targetBranch}</span></span>
                <span class="line4">
                  <PrBadge tone={STATE_TONE[p.state]} icon={STATE_ICON[p.state]} label={t(STATE_LABEL[p.state])} />
                  {#if p.isDraft}<PrBadge tone="muted" icon="pencil" label={t('pr.draft')} />{/if}
                  {#if p.mergeStatus === 'conflicts'}<PrBadge tone="danger" icon="triangle-alert" label={t(MERGE_LABEL.conflicts)} />{/if}
                  {#if p.checks && p.checks.passing + p.checks.failing + p.checks.pending > 0}
                    <span class="checks" title={checksTitle(p)}>
                      <span class="sr-only">{checksTitle(p)}</span>
                      {#if p.checks.passing > 0}<span class="c ok" aria-hidden="true"><Icon name="circle-check" size={11} />{p.checks.passing}</span>{/if}
                      {#if p.checks.failing > 0}<span class="c bad" aria-hidden="true"><Icon name="circle-alert" size={11} />{p.checks.failing}</span>{/if}
                      {#if p.checks.pending > 0}<span class="c wait" aria-hidden="true"><Icon name="clock" size={11} />{p.checks.pending}</span>{/if}
                    </span>
                  {/if}
                  <span class="grow"></span>
                  <PrReviewers reviewers={p.reviewers} provider={pr.support?.provider} />
                </span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <footer class="foot">
      <div class="pager">
        <button type="button" class="btn btn-sm" disabled={pr.page <= 1 || pr.listLoading} onclick={() => void pr.prevPage()} aria-label={t('pr.prevPage')}>
          <Icon name="chevron-left" size={12} />{t('pr.prev')}
        </button>
        <span class="pg" aria-live="polite">{t('pr.page', { n: pr.page })}{#if pr.items.length > 0}{' · '}{tn('pr.count', pr.visible.length)}{/if}</span>
        <button type="button" class="btn btn-sm" disabled={!pr.hasMore || pr.listLoading} onclick={() => void pr.nextPage()} aria-label={t('pr.nextPage')}>
          {t('pr.next')}<Icon name="chevron-right" size={12} />
        </button>
      </div>
      <label class="auto">
        <span>{t('pr.auto')}</span>
        <select class="field" value={String(pr.autoRefresh)} onchange={(e) => pr.setAutoRefresh(Number(e.currentTarget.value))}>
          {#each AUTO_REFRESH_OPTIONS as s (s)}
            <option value={String(s)}>{s === 0 ? t('pr.auto.off') : s === 60 ? t('pr.auto.1m') : t('pr.auto.5m')}</option>
          {/each}
        </select>
      </label>
      {#if pr.support}
        <p class="repo" title={pr.support.repoLabel}><Icon name="cloud" size={11} />{pr.support.providerLabel} · {pr.support.repoLabel}</p>
      {/if}
    </footer>
  {/if}
</section>

<style>
  .prs {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .loading {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-4);
    color: var(--text-muted);
    font-size: var(--text-sm);
  }
  .pad {
    padding: var(--space-3);
  }
  .small {
    max-width: 38ch;
    margin: 0 0 var(--space-3);
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
  }
  .search {
    flex: 1 1 120px;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--text-muted);
    min-width: 0;
  }
  .search .field {
    flex: 1;
    min-width: 0;
  }
  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 0 var(--space-3) var(--space-2);
    border-bottom: 1px solid var(--border);
  }
  .fbtn {
    min-height: 26px;
    padding: 0 var(--space-2);
    border: 1px solid var(--border-strong);
    border-radius: var(--pill);
    background: none;
    color: var(--text-muted);
    font-size: var(--text-xs);
    font-weight: 600;
  }
  .fbtn:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .fbtn.on {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .list,
  .skeletons {
    list-style: none;
    margin: 0;
    padding: var(--space-1) var(--space-2);
  }
  .skeletons li {
    height: 76px;
    margin: var(--space-1) 0;
    border-radius: var(--radius-md);
  }
  .row {
    display: flex;
    flex-direction: column;
    gap: 3px;
    width: 100%;
    padding: var(--space-2);
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    background: none;
    color: var(--text);
    text-align: left;
  }
  .row:hover {
    background: var(--bg-hover);
  }
  .row.sel {
    background: var(--bg-selected);
    border-color: var(--border-strong);
  }
  .title {
    font-size: var(--text-md);
    font-weight: 600;
    line-height: 1.3;
    overflow-wrap: anywhere;
  }
  .line2,
  .line3 {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .num {
    font-weight: 650;
  }
  .br {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .line4 {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 2px;
  }
  .grow {
    flex: 1;
  }
  .checks {
    display: inline-flex;
    gap: 6px;
    font-size: var(--text-2xs);
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }
  .c {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
  .c.ok {
    color: var(--success);
  }
  .c.bad {
    color: var(--danger);
  }
  .c.wait {
    color: var(--warning);
  }
  .none {
    padding: var(--space-4);
    color: var(--text-muted);
    font-size: var(--text-sm);
    text-align: center;
  }
  .foot {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2) var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-top: 1px solid var(--border);
    background: var(--bg-app);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .pager {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .pg {
    font-variant-numeric: tabular-nums;
  }
  .auto {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    margin-left: auto;
  }
  .auto .field {
    width: auto;
    min-height: 26px;
    font-size: var(--text-xs);
  }
  .repo {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-basis: 100%;
    margin: 0;
    color: var(--text-subtle);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Colores forzados: la selección no puede depender solo del color de fondo. */
  @media (forced-colors: active) {
    .row.sel {
      outline: 2px solid Highlight;
      outline-offset: -2px;
    }
    .fbtn {
      border: 1px solid ButtonText;
    }
    .fbtn.on {
      background: Highlight;
      color: HighlightText;
      border-color: Highlight;
    }
  }
</style>
