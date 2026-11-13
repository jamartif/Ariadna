<script lang="ts">
  import { t, tn } from '$lib/i18n';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { branchesOf } from '$lib/stores/branches.svelte';
  import { conflictsOf } from '$lib/stores/conflicts.svelte';
  import { prOf } from '$lib/stores/pullRequests.svelte';
  import { remotesOf } from '$lib/stores/remotes.svelte';
  import { branchNameProblem } from '$lib/util/branchName';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';

  /** Detalle y acciones de la rama seleccionada (columna derecha). */
  let { tab }: { tab: TabState } = $props();

  const b = $derived(branchesOf(tab));
  const br = $derived(b.selectedBranch);
  const remote = $derived(br ? b.isRemote(br.name) : false);
  const current = $derived(b.current);
  const busy = $derived(b.busy !== null);
  const integrating = $derived((tab.snapshot?.integration ?? 'none') !== 'none');

  const remotes = $derived(remotesOf(tab));
  // El panel puede abrirse solo (desde el riel): no depende de que RemotesSection haya cargado.
  const remoteSig = $derived((tab.snapshot?.remotes ?? []).map((r) => `${r.name}=${r.fetchUrl}`).join('|'));
  $effect(() => {
    void remoteSig;
    if (!tab.hibernated) void remotes.load();
  });
  /** Remoto y rama para los enlaces al navegador (null = sin remoto con página web). */
  const web = $derived(br ? remotes.linkFor(br.name, remote) : null);
  const webOk = $derived(web !== null && web.remote.canOpen);
  const prOk = $derived(web !== null && web.remote.canCreatePullRequest);
  /** Alternativa al navegador: el formulario de PR de Ariadna (solo si el remoto lo admite). */
  const prc = $derived(prOf(tab));
  function createHere() {
    prc.startCreate();
    tab.panel = 'pullRequests';
  }

  let renaming = $state(false);
  let renameTo = $state('');
  let renameInput = $state<HTMLInputElement>();
  const renameProblem = $derived(renameTo === '' ? null : renameTo === br?.name ? 'branches.problem.same' : branchNameProblem(renameTo));

  function startRename() {
    renaming = true;
    renameTo = br?.name ?? '';
    queueMicrotask(() => renameInput?.select());
  }

  async function submitRename(e: Event) {
    e.preventDefault();
    if (!br || renameProblem || renameTo === '') return;
    if (await b.rename(br.name, renameTo)) renaming = false;
  }
</script>

<section class="detail" aria-label={t('branches.detail')}>
  {#if !br}
    <EmptyState icon="git-branch" title={t('branches.pickTitle')} body={t('branches.pickBody')} />
  {:else}
    <header class="head">
      <Icon name={remote ? 'cloud' : 'git-branch'} size={20} />
      <div class="titles">
        <h2 class="mono">{br.name}</h2>
        <p>{remote ? t('branches.kindRemote') : br.isHead ? t('branches.kindCurrent') : t('branches.kindLocal')}</p>
      </div>
    </header>

    <dl class="facts">
      <div><dt>{t('branches.tip')}</dt><dd><code class="mono">{br.oid.slice(0, 10)}</code></dd></div>
      {#if br.upstream}<div><dt>{t('branches.upstream')}</dt><dd><code class="mono">{br.upstream}</code></dd></div>{/if}
      {#if !remote && br.upstream}
        <div>
          <dt>{t('branches.sync')}</dt>
          <dd>
            {#if br.ahead === 0 && br.behind === 0}{t('branches.inSync')}
            {:else}
              {#if br.ahead > 0}<span class="sync"><Icon name="arrow-up" size={12} />{tn('branches.ahead', br.ahead)}</span>{/if}
              {#if br.behind > 0}<span class="sync"><Icon name="arrow-down" size={12} />{tn('branches.behind', br.behind)}</span>{/if}
            {/if}
          </dd>
        </div>
      {/if}
    </dl>

    {#if integrating}
      <div class="integ" role="status">
        <p><Icon name="git-merge" size={14} />{t(`integration.${tab.snapshot?.integration ?? 'merging'}`)}. {t('branches.integrationNote')}</p>
        <div class="row">
          <button type="button" class="btn btn-ghost" disabled={busy} onclick={() => conflictsOf(tab).abort()}>{t('conflicts.abort')}</button>
          <button type="button" class="btn btn-primary" onclick={() => (tab.panel = 'conflicts')}>{t('conflicts.banner.open')}</button>
        </div>
      </div>
    {/if}

    {#if renaming && !remote}
      <form class="rename" onsubmit={submitRename}>
        <label class="lbl" for="branch-rename">{t('branches.renameTo')}</label>
        <input
          id="branch-rename"
          bind:this={renameInput}
          class="field mono"
          bind:value={renameTo}
          autocomplete="off"
          spellcheck="false"
          aria-invalid={renameProblem ? 'true' : undefined}
          aria-describedby="branch-rename-hint"
        />
        <p id="branch-rename-hint" class="hint" class:bad={renameProblem !== null} aria-live="polite">
          {#if renameProblem}<Icon name="circle-alert" size={12} />{t(renameProblem)}{/if}
        </p>
        <div class="row">
          <button type="button" class="btn btn-ghost" onclick={() => (renaming = false)}>{t('branches.cancel')}</button>
          <button type="submit" class="btn btn-primary" disabled={renameTo === '' || renameProblem !== null || busy}>{t('branches.rename')}</button>
        </div>
      </form>
    {/if}

    {#if b.prompt?.kind === 'merge' && b.prompt.name === br.name}
      <div class="mergeask" role="group" aria-label={t('branches.mergeAsk', { name: br.name, into: current?.name ?? 'HEAD' })}>
        <p>{t('branches.mergeAsk', { name: br.name, into: current?.name ?? 'HEAD' })}</p>
        <p class="small">{t('branches.mergeNote')}</p>
        <div class="row">
          <button type="button" class="btn btn-ghost" onclick={() => b.cancelPrompt()}>{t('branches.cancel')}</button>
          <button type="button" class="btn btn-primary" onclick={() => b.merge(br.name)}>{t('branches.mergeConfirm')}</button>
        </div>
      </div>
    {/if}

    <div class="buttons">
      <button type="button" class="btn btn-primary" disabled={br.isHead || busy} onclick={() => b.checkout(br.name)}>
        {#if b.busy === 'checkout'}<Icon name="loader" size={14} spin />{:else}<Icon name="arrow-right" size={14} />{/if}
        {remote ? t('branches.checkoutRemote') : t('branches.checkout')}
      </button>
      <button type="button" class="btn" disabled={br.isHead || busy || !current || integrating} onclick={() => b.askMerge(br.name)}>
        {#if b.busy === 'merge'}<Icon name="loader" size={14} spin />{:else}<Icon name="git-merge" size={14} />{/if}
        {t('branches.merge', { into: current?.name ?? 'HEAD' })}
      </button>
      {#if !remote}
        <button type="button" class="btn" disabled={busy} onclick={startRename}><Icon name="pencil" size={14} />{t('branches.rename')}</button>
        <button type="button" class="btn btn-danger" disabled={br.isHead || busy} onclick={() => b.remove(br.name)} title={br.isHead ? t('branches.cannotDeleteCurrent') : undefined}>
          <Icon name="trash" size={14} />{t('branches.delete')}
        </button>
      {/if}
    </div>
    <div class="buttons web" role="group" aria-label={t('remotes.open')}>
      <button type="button" class="btn" disabled={!webOk || busy || remotes.isBusy()} onclick={() => web && remotes.open(web.remote.name, 'branch', web.branch)}>
        <Icon name="external-link" size={14} />{t('branches.openBranch')}
      </button>
      <button
        type="button"
        class="btn"
        disabled={!prOk || busy || remotes.isBusy()}
        title={web && prOk ? t('branches.createPrHint', { provider: web.remote.providerLabel, branch: web.branch }) : undefined}
        onclick={() => web && remotes.open(web.remote.name, 'pull-request', web.branch)}
      >
        <Icon name="git-merge" size={14} />{t('branches.createPr')}
      </button>
      {#if prc.supported}
        <button
          type="button"
          class="btn"
          disabled={!br.isHead || busy}
          title={br.isHead ? t('branches.createPrHere.hint') : t('branches.createPrHere.needCurrent')}
          onclick={createHere}
        >
          <Icon name="git-pull-request" size={14} />{t('branches.createPrHere')}
        </button>
      {/if}
    </div>
    {#if web && webOk}
      <p class="note"><Icon name="cloud" size={12} />{t('branches.webVia', { remote: web.remote.name, provider: web.remote.providerLabel })}</p>
    {:else}
      <p class="note"><Icon name="info" size={12} />{t('branches.noWebRemote')}</p>
    {/if}
    {#if br.isHead}<p class="note"><Icon name="info" size={12} />{t('branches.cannotDeleteCurrent')}</p>{/if}
    {#if remote}<p class="note"><Icon name="info" size={12} />{t('branches.remoteNote')}</p>{/if}
  {/if}
</section>

<style>
  .detail {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    height: 100%;
    min-height: 0;
    overflow: auto;
    padding: var(--space-4);
    background: var(--bg-surface);
  }
  .head {
    display: flex;
    gap: var(--space-3);
    align-items: center;
    color: var(--accent);
  }
  .titles {
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: var(--text-lg);
    color: var(--text);
    overflow-wrap: anywhere;
  }
  .titles p {
    margin: 2px 0 0;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .facts {
    display: grid;
    gap: 4px;
    margin: 0;
    font-size: var(--text-sm);
  }
  .facts > div {
    display: flex;
    gap: var(--space-3);
  }
  dt {
    width: 110px;
    flex: none;
    color: var(--text-subtle);
  }
  dd {
    margin: 0;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
    min-width: 0;
  }
  .sync {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    color: var(--text-muted);
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .note {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
  .rename,
  .mergeask {
    display: grid;
    gap: var(--space-1);
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
  }
  .mergeask p {
    margin: 0;
    font-size: var(--text-sm);
  }
  .mergeask .small {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .lbl {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text-muted);
  }
  .hint {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    min-height: 16px;
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
  .hint.bad {
    color: var(--danger);
  }
  .row {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-1);
  }
  .integ {
    display: grid;
    gap: var(--space-1);
    padding: var(--space-3);
    border: 1px solid var(--warning);
    border-radius: var(--radius-md);
    background: var(--warning-soft);
  }
  .integ p {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    margin: 0;
    font-size: var(--text-sm);
  }
</style>
