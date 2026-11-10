<script lang="ts">
  import { onMount } from 'svelte';
  import { t, tn } from '$lib/i18n';
  import type { Branch } from '$lib/api/types';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { branchesOf } from '$lib/stores/branches.svelte';
  import { branchNameProblem } from '$lib/util/branchName';
  import Banner from './Banner.svelte';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';
  import RemotesSection from './RemotesSection.svelte';

  let { tab }: { tab: TabState } = $props();

  const b = $derived(branchesOf(tab));
  let creating = $state(false);
  let newName = $state('');
  let startPoint = $state('');
  let filter = $state('');
  let nameInput = $state<HTMLInputElement>();

  const problem = $derived(newName === '' ? null : branchNameProblem(newName));
  const q = $derived(filter.trim().toLowerCase());
  const locals = $derived(b.locals.filter((x) => !q || x.name.toLowerCase().includes(q)));
  const remotes = $derived(b.remotes.filter((x) => !q || x.name.toLowerCase().includes(q)));
  const sel = $derived(b.selectedBranch?.name ?? null);

  onMount(() => {
    // Al abrir el panel se selecciona la rama actual.
    if (!branchesOf(tab).selected) branchesOf(tab).selected = branchesOf(tab).current?.name ?? null;
  });

  function startCreate() {
    creating = true;
    newName = '';
    startPoint = b.selectedBranch?.name ?? '';
    queueMicrotask(() => nameInput?.focus());
  }

  async function submitCreate(e: Event) {
    e.preventDefault();
    if (newName === '' || problem) return;
    const ok = await b.create(newName, startPoint.trim() === '' ? null : startPoint.trim());
    if (ok) {
      creating = false;
      newName = '';
    }
  }
</script>

{#snippet item(br: Branch, remote: boolean)}
  <li>
    <button
      type="button"
      class="item"
      class:head={br.isHead}
      class:sel={sel === br.name}
      aria-pressed={sel === br.name}
      aria-current={br.isHead ? 'true' : undefined}
      onclick={() => b.select(br.name)}
      ondblclick={() => !br.isHead && b.checkout(br.name)}
    >
      <Icon name={remote ? 'cloud' : 'git-branch'} size={14} />
      <span class="name mono">{br.name}</span>
      {#if br.isHead}<span class="badge">{t('branches.current')}</span>{/if}
      {#if br.ahead > 0}<span class="sync" title={t('sidebar.ahead', { n: br.ahead })}><Icon name="arrow-up" size={11} />{br.ahead}</span>{/if}
      {#if br.behind > 0}<span class="sync" title={t('sidebar.behind', { n: br.behind })}><Icon name="arrow-down" size={11} />{br.behind}</span>{/if}
    </button>
  </li>
{/snippet}

<section class="branches" aria-label={t('branches.region')}>
  <div class="toolbar">
    <div class="filter">
      <Icon name="search" size={14} />
      <input class="field" type="search" bind:value={filter} placeholder={t('branches.filter')} aria-label={t('branches.filter')} autocomplete="off" spellcheck="false" />
    </div>
    <button type="button" class="btn btn-primary" onclick={startCreate} disabled={creating || b.busy !== null}>
      <Icon name="plus" size={14} />{t('branches.new')}
    </button>
  </div>

  {#if creating}
    <form class="create" onsubmit={submitCreate} aria-label={t('branches.new')}>
      <label class="lbl" for="branch-new-name">{t('branches.name')}</label>
      <input
        id="branch-new-name"
        bind:this={nameInput}
        class="field mono"
        bind:value={newName}
        placeholder="feature/mi-cambio"
        autocomplete="off"
        spellcheck="false"
        aria-invalid={problem ? 'true' : undefined}
        aria-describedby="branch-new-hint"
      />
      <label class="lbl" for="branch-new-start">{t('branches.startPoint')}</label>
      <input id="branch-new-start" class="field mono" bind:value={startPoint} placeholder={t('branches.startPlaceholder')} autocomplete="off" spellcheck="false" />
      <p id="branch-new-hint" class="hint" class:bad={problem !== null} aria-live="polite">
        {#if problem}<Icon name="circle-alert" size={12} />{t(problem)}{:else}{t('branches.nameHint')}{/if}
      </p>
      <div class="actions">
        <button type="button" class="btn btn-ghost" onclick={() => (creating = false)}>{t('branches.cancel')}</button>
        <button type="submit" class="btn btn-primary" disabled={newName === '' || problem !== null || b.busy !== null}>
          {#if b.busy === 'create'}<Icon name="loader" size={14} spin />{/if}{t('branches.create')}
        </button>
      </div>
    </form>
  {/if}

  {#if b.prompt?.kind === 'checkout-dirty'}
    <div class="prompt">
      <Banner kind="warning" title={t('branches.dirtyTitle')} message={tn('branches.dirtyBody', b.prompt.changes, { name: b.prompt.name })}>
        {#snippet actions()}
          <button type="button" class="btn" onclick={() => b.cancelPrompt()}>{t('branches.cancel')}</button>
          <button type="button" class="btn btn-primary" onclick={() => b.prompt && b.checkout(b.prompt.name, true)}>{t('branches.switchAnyway')}</button>
        {/snippet}
      </Banner>
    </div>
  {/if}

  <div class="scroll">
    <RemotesSection {tab} />
    {#if b.locals.length === 0 && b.remotes.length === 0}
      <EmptyState compact icon="git-branch" title={t('branches.emptyTitle')} body={t('branches.emptyBody')} />
    {:else}
      <h3><span>{t('branches.local')}</span><span class="chip">{locals.length}</span></h3>
      <ul>
        {#each locals as br (br.name)}{@render item(br, false)}{/each}
      </ul>
      {#if remotes.length > 0}
        <h3><span>{t('branches.remote')}</span><span class="chip">{remotes.length}</span></h3>
        <ul>
          {#each remotes as br (br.name)}{@render item(br, true)}{/each}
        </ul>
      {/if}
      {#if locals.length === 0 && remotes.length === 0}
        <p class="none">{t('branches.noMatches', { query: filter.trim() })}</p>
      {/if}
    {/if}
  </div>
</section>

<style>
  .branches {
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
  .filter {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--text-muted);
    min-width: 0;
  }
  .filter .field {
    flex: 1;
    min-width: 0;
  }
  .create {
    display: grid;
    gap: var(--space-1);
    padding: var(--space-3);
    border-bottom: 1px solid var(--border);
    background: var(--bg-subtle);
  }
  .lbl {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text-muted);
    margin-top: var(--space-1);
  }
  .hint {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: var(--space-1) 0 0;
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
  .hint.bad {
    color: var(--danger);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
  .prompt {
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--border);
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding-bottom: var(--space-3);
  }
  h3 {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: var(--space-3) var(--space-3) var(--space-1);
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-subtle);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0 var(--space-2);
  }
  .item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: 32px;
    padding: 0 var(--space-2);
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text);
    text-align: left;
    font-size: var(--text-sm);
  }
  .item:hover {
    background: var(--bg-hover);
  }
  .item.sel {
    background: var(--bg-selected);
  }
  .item.head .name {
    font-weight: 700;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    flex: none;
    padding: 0 6px;
    height: 18px;
    line-height: 18px;
    border-radius: var(--pill);
    font-size: var(--text-2xs);
    font-weight: 650;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .sync {
    display: inline-flex;
    align-items: center;
    font-size: var(--text-xs);
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .none {
    padding: var(--space-4);
    color: var(--text-muted);
    font-size: var(--text-sm);
    text-align: center;
  }
</style>
