<script lang="ts">
  import { t } from '$lib/i18n';
  import type { Snapshot } from '$lib/api/types';
  import Icon from './Icon.svelte';
  import EmptyState from './EmptyState.svelte';

  let { snapshot, loading }: { snapshot: Snapshot | null; loading: boolean } = $props();

  // Placeholder navegable: el contenido real (checkout, crear, fusionar) llega en la oleada 2.
  let current = $state<string | null>(null);
</script>

<nav class="sidebar" aria-label={t('sidebar.label')}>
  {#if loading && !snapshot}
    <div class="sk">
      {#each [60, 80, 50, 70] as w, i (i)}<span class="skeleton" style:width="{w}%" style:height="16px"></span>{/each}
    </div>
  {:else if !snapshot || (snapshot.branches.length === 0 && snapshot.remotes.length === 0)}
    <EmptyState compact icon="git-branch" title={t('sidebar.emptyTitle')} body={t('sidebar.emptyBody')} />
  {:else}
    <section>
      <h2><span>{t('sidebar.branches')}</span><span class="chip">{snapshot.branches.length}</span></h2>
      <ul>
        {#each snapshot.branches as b (b.name)}
          <li>
            <button
              type="button"
              class="item"
              class:head={b.isHead}
              aria-current={b.isHead ? 'true' : undefined}
              aria-pressed={current === b.name}
              onclick={() => (current = b.name)}
            >
              <Icon name="git-branch" size={14} />
              <span class="name mono">{b.name}</span>
              {#if b.ahead > 0}<span class="sync" title={t('sidebar.ahead', { n: b.ahead })}><Icon name="arrow-up" size={11} />{b.ahead}</span>{/if}
              {#if b.behind > 0}<span class="sync" title={t('sidebar.behind', { n: b.behind })}><Icon name="arrow-down" size={11} />{b.behind}</span>{/if}
              {#if b.isHead}<span class="sr-only">{t('sidebar.current')}</span>{/if}
            </button>
          </li>
        {/each}
      </ul>
    </section>

    {#if snapshot.remoteBranches.length > 0}
      <section>
        <h2><span>{t('sidebar.tracking')}</span><span class="chip">{snapshot.remoteBranches.length}</span></h2>
        <ul>
          {#each snapshot.remoteBranches as b (b.name)}
            <li>
              <button type="button" class="item" aria-pressed={current === b.name} onclick={() => (current = b.name)}>
                <Icon name="cloud" size={14} />
                <span class="name mono">{b.name}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if snapshot.remotes.length > 0}
      <section>
        <h2><span>{t('sidebar.remotes')}</span><span class="chip">{snapshot.remotes.length}</span></h2>
        <ul>
          {#each snapshot.remotes as r (r.name)}
            <li>
              <button type="button" class="item two" aria-pressed={current === r.name} onclick={() => (current = r.name)}>
                <Icon name="cloud" size={14} />
                <span class="stack">
                  <span class="name">{r.name}</span>
                  <span class="url mono">{r.fetchUrl}</span>
                </span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
    <p class="note"><Icon name="info" size={12} />{t('sidebar.soon')}</p>
  {/if}
</nav>

<style>
  .sidebar {
    height: 100%;
    overflow: auto;
    padding: var(--space-3) var(--space-2);
    background: var(--bg-app);
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  h2 {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 var(--space-2);
    margin-bottom: var(--space-1);
    color: var(--text-muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: 28px;
    padding: 2px var(--space-2);
    border: none;
    border-radius: var(--radius-md);
    background: none;
    color: var(--text-muted);
    text-align: left;
    font-size: var(--text-sm);
  }
  .item:hover {
    background: var(--bg-subtle);
    color: var(--text);
  }
  .item[aria-pressed='true'] {
    background: var(--bg-muted);
    color: var(--text);
  }
  .item.head {
    color: var(--text);
    font-weight: 600;
    background: var(--accent-soft);
  }
  .item.head :global(svg) {
    color: var(--accent);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-xs);
  }
  .two {
    align-items: flex-start;
    padding-top: 6px;
    padding-bottom: 6px;
  }
  .stack {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .stack .name {
    font-family: var(--font-ui);
    font-size: var(--text-sm);
  }
  .url {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-subtle);
    font-size: 0.6875rem;
  }
  .sync {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    font-size: 0.6875rem;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .note {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    margin-top: auto;
    padding: var(--space-2);
    color: var(--text-subtle);
    font-size: var(--text-xs);
  }
  .sk {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-2);
  }
</style>
