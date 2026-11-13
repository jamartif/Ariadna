<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { t, tn } from '$lib/i18n';
  import type { AiPullRequestSuggestion, ApiError } from '$lib/api/types';
  import { prOf } from '$lib/stores/pullRequests.svelte';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { toasts } from '$lib/stores/toasts.svelte';
  import { baseOptions, defaultBase, parseIds, parseList } from '$lib/util/pr';
  import Banner from './Banner.svelte';
  import Icon from './Icon.svelte';
  import MarkdownView from './MarkdownView.svelte';
  import PrError from './PrError.svelte';
  import ProposalSheet from './ProposalSheet.svelte';
  import { profileLabel } from '$lib/stores/ai.svelte';

  /** Crear PR desde la rama actual (PR-04): base, título, descripción (con plantilla e IA), borrador y revisores. */
  let { tab }: { tab: TabState } = $props();

  const pr = $derived(prOf(tab));
  const caps = $derived(pr.caps);
  const snap = $derived(tab.snapshot);
  const remote = $derived(pr.support?.remote ?? null);
  const head = $derived(tab.info.detached ? null : (snap?.status.branchName ?? tab.info.branch));
  const publication = $derived(pr.publication);
  const ahead = $derived(snap?.branches.find((b) => b.isHead)?.ahead ?? 0);

  let base = $state(untrack(() => defaultBase(tab.snapshot, prOf(tab).support?.remote ?? null, tab.info.detached ? null : tab.info.branch)));
  let baseTouched = false;
  let title = $state('');
  let description = $state('');
  let draft = $state(false);
  let reviewers = $state('');
  let workItems = $state('');
  let preview = $state(false);
  let templateApplied = $state(false);
  let titleInput = $state<HTMLInputElement>();
  const uid = Math.random().toString(36).slice(2, 8);

  // Mientras el usuario no elija otra, la base sigue a la rama por defecto del remoto (puede llegar tarde).
  $effect(() => {
    const d = defaultBase(snap, remote, head);
    if (!baseTouched) base = d;
  });

  const bases = $derived.by(() => {
    const list = baseOptions(snap, remote, head);
    return list.includes(base) || base === '' ? list : [base, ...list];
  });
  const ids = $derived(parseIds(workItems));
  const sameBranch = $derived(head !== null && head === base);
  const problem = $derived(
    head === null
      ? 'pr.create.detached'
      : sameBranch
        ? 'pr.create.sameBranch'
        : title.trim() === ''
          ? 'pr.create.noTitle'
          : ids.invalid.length > 0
            ? 'pr.create.badWorkItems'
            : publication === 'unpublished'
              ? 'pr.create.unpublished'
              : null
  );

  onMount(() => {
    void tab.ai.loadState();
    titleInput?.focus();
  });

  // La plantilla del repo rellena la descripción una vez, y solo si el usuario no ha escrito nada.
  $effect(() => {
    if (pr.templateLoaded && !templateApplied) {
      templateApplied = true;
      if (pr.template && description.trim() === '') description = pr.template;
    }
  });

  function applyTemplate() {
    if (pr.template) description = pr.template;
  }

  async function submit(e: Event) {
    e.preventDefault();
    if (problem || pr.busyAny) return;
    await pr.create({
      title: title.trim(),
      description,
      sourceBranch: head as string,
      targetBranch: base,
      draft: draft && !!caps?.draft,
      reviewers: parseList(reviewers),
      workItemIds: caps?.workItems ? ids.ids : []
    });
  }

  // ---- IA (propone; nada se aplica hasta «Usar») ----
  let generating = $state(false);
  let aiError = $state.raw<ApiError | null>(null);
  let proposal = $state.raw<AiPullRequestSuggestion | null>(null);
  let applied = $state<{ title: string; body: string } | null>(null);
  let sheet = $state(false);
  let regenerating = $state(false);
  let sheetError = $state<string | null>(null);

  async function generate() {
    if (generating || regenerating) return;
    generating = true;
    aiError = null;
    toasts.announce(t('ai.generating'));
    const res = await pr.generate(base, pr.template);
    generating = false;
    if (!res.ok) {
      if (res.error.code !== 'CANCELLED') aiError = res.error;
      return;
    }
    proposal = res.value;
    sheet = true;
    void tab.ai.loadAccounts();
  }

  async function regenerate() {
    if (regenerating) return;
    regenerating = true;
    sheetError = null;
    const res = await pr.generate(base, pr.template);
    regenerating = false;
    if (!res.ok) {
      if (res.error.code !== 'CANCELLED') sheetError = res.error.message;
      return;
    }
    proposal = res.value;
  }

  async function cancelAi() {
    try {
      await tab.api.aiCancel(tab.id);
    } catch {
      /* ya había terminado */
    }
  }

  function useProposal(v: { title: string; body: string }) {
    title = v.title;
    description = v.body;
    applied = v;
    sheet = false;
    toasts.announce(t('ai.used'));
    queueMicrotask(() => titleInput?.focus());
  }

  function closeSheet() {
    if (regenerating) void cancelAi();
    sheet = false;
    sheetError = null;
  }

  const stale = $derived(proposal !== null && proposal.repoRevision !== tab.revision);
  const inUse = $derived(applied !== null && proposal !== null && applied.title === title.trim() && applied.body === description.trim());
</script>

<form class="create" onsubmit={submit} aria-label={t('pr.create.title')}>
  <header class="head">
    <h2><Icon name="git-merge" size={18} />{t('pr.create.title')}</h2>
    <button type="button" class="btn btn-ghost btn-icon" onclick={() => pr.cancelCreate()} aria-label={t('pr.create.close')}><Icon name="x" size={16} /></button>
  </header>

  <div class="scroll">
    {#if head === null}
      <Banner kind="warning" title={t('pr.create.detachedTitle')} message={t('pr.create.detached')} />
    {/if}

    {#if publication === 'unpublished' || (publication === 'ahead' && ahead > 0)}
      <Banner
        kind="warning"
        title={publication === 'unpublished' ? t('pr.create.unpublishedTitle') : t('pr.create.aheadTitle')}
        message={publication === 'unpublished' ? t('pr.create.unpublishedBody', { branch: head ?? '' }) : tn('pr.create.aheadBody', ahead)}
      >
        {#snippet actions()}
          <button type="button" class="btn btn-primary" onclick={() => void pr.publish()} disabled={pr.busyAny || head === null}>
            {#if pr.busy === 'push'}<Icon name="loader" size={14} spin />{:else}<Icon name="arrow-up" size={14} />{/if}
            {publication === 'unpublished' ? t('pr.create.publish') : t('pr.create.pushNow')}
          </button>
        {/snippet}
      </Banner>
    {/if}

    <div class="branches">
      <div class="f">
        <span class="lbl" id="pr-src-{uid}">{t('pr.create.source')}</span>
        <span class="static mono" aria-labelledby="pr-src-{uid}"><Icon name="git-branch" size={13} />{head ?? t('status.detached')}</span>
      </div>
      <Icon name="arrow-right" size={16} />
      <div class="f">
        <label class="lbl" for="pr-base-{uid}">{t('pr.create.base')}</label>
        <select id="pr-base-{uid}" class="field mono" bind:value={base} onchange={() => (baseTouched = true)}>
          {#each bases as b (b)}<option value={b}>{b}</option>{/each}
        </select>
      </div>
    </div>
    {#if sameBranch}<p class="warn" role="alert"><Icon name="triangle-alert" size={12} />{t('pr.create.sameBranch')}</p>{/if}

    <div class="f">
      <label class="lbl" for="pr-title-{uid}">{t('pr.create.titleField')}</label>
      <input id="pr-title-{uid}" bind:this={titleInput} class="field" bind:value={title} autocomplete="off" spellcheck="true" required aria-required="true" />
    </div>

    <div class="f">
      <div class="lblrow">
        <label class="lbl" for="pr-desc-{uid}">{t('pr.create.description')}</label>
        <span class="grow"></span>
        {#if pr.template}
          <button type="button" class="btn btn-sm btn-ghost" onclick={applyTemplate} disabled={description === pr.template}>
            <Icon name="clipboard" size={12} />{t('pr.create.template')}
          </button>
        {/if}
        <button type="button" class="btn btn-sm btn-ghost" aria-pressed={preview} onclick={() => (preview = !preview)}>
          <Icon name="eye" size={12} />{preview ? t('pr.create.write') : t('pr.create.preview')}
        </button>
      </div>
      {#if preview}
        <div class="previewbox" aria-label={t('pr.create.preview')} role="region"><MarkdownView source={description} onopen={(u) => void pr.openUrl(u)} empty={t('pr.detail.noDescription')} /></div>
      {:else}
        <textarea id="pr-desc-{uid}" class="field" rows="10" bind:value={description} spellcheck="true"></textarea>
      {/if}
      {#if pr.template}<p class="tip"><Icon name="info" size={12} />{t('pr.create.templateNote')}</p>{/if}
    </div>

    <div class="ai">
      {#if generating}
        <button type="button" class="btn" onclick={() => void cancelAi()}><Icon name="stop" size={14} />{t('pr.create.aiCancel')}</button>
        <span class="tip" role="status"><Icon name="loader" size={12} spin />{t('ai.generating')}</span>
      {:else}
        <button type="button" class="btn" onclick={() => void generate()} disabled={head === null || sameBranch || pr.busyAny}>
          <Icon name="sparkles" size={14} />{t('pr.create.ai')}
        </button>
        <span class="tip">{t('pr.create.aiNote')}</span>
      {/if}
    </div>
    {#if aiError}
      <Banner kind="error" title={t('pr.create.aiFailed')} message={aiError.message} detail={aiError.detail}>
        {#snippet actions()}
          <button type="button" class="btn" onclick={() => (aiError = null)}>{t('pr.error.dismiss')}</button>
        {/snippet}
      </Banner>
    {/if}

    {#if caps?.draft}
      <label class="chk"><input type="checkbox" bind:checked={draft} />{t('pr.create.draft')}</label>
    {/if}

    <div class="f">
      <label class="lbl" for="pr-rev-{uid}">{t('pr.create.reviewers')}</label>
      <input id="pr-rev-{uid}" class="field" bind:value={reviewers} autocomplete="off" spellcheck="false" aria-describedby="pr-rev-hint-{uid}" />
      <p class="tip" id="pr-rev-hint-{uid}">{caps?.provider === 'github' ? t('pr.create.reviewersHintGithub') : t('pr.create.reviewersHint')}</p>
    </div>

    {#if caps?.workItems}
      <div class="f">
        <label class="lbl" for="pr-wi-{uid}">{t('pr.create.workItems')}</label>
        <input id="pr-wi-{uid}" class="field mono" bind:value={workItems} autocomplete="off" spellcheck="false" aria-invalid={ids.invalid.length > 0 ? 'true' : undefined} />
        <p class="tip" class:bad={ids.invalid.length > 0}>{ids.invalid.length > 0 ? t('pr.create.badWorkItems') : t('pr.create.workItemsHint')}</p>
      </div>
    {/if}

    {#if pr.createError}
      <PrError error={pr.createError} ondismiss={() => (pr.createError = null)} />
    {/if}
  </div>

  <footer class="foot">
    <p class="why" aria-live="polite">{#if problem}<Icon name="circle-alert" size={12} />{t(problem as 'pr.create.noTitle')}{/if}</p>
    <button type="button" class="btn btn-ghost" onclick={() => pr.cancelCreate()}>{t('pr.cancel')}</button>
    <button type="submit" class="btn btn-primary" disabled={problem !== null || pr.busyAny}>
      {#if pr.busy === 'create'}<Icon name="loader" size={14} spin />{:else}<Icon name="plus" size={14} />{/if}{t('pr.create.submit')}
    </button>
  </footer>
</form>

{#if sheet && proposal}
  <ProposalSheet
    kind="pr"
    title={proposal.title}
    body={proposal.body}
    providerName={tab.ai.providerName(proposal.provider)}
    profileName={tab.ai.state ? profileLabel(tab.ai.state.profile.kind) : null}
    account={tab.ai.accountLabel(proposal.provider)}
    commandSummary={proposal.commandSummary}
    {stale}
    {inUse}
    {regenerating}
    error={sheetError}
    onuse={useProposal}
    onregenerate={() => void regenerate()}
    ondiscard={closeSheet}
    onclose={closeSheet}
  />
{/if}

<style>
  .create {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--border);
  }
  h2 {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-lg);
    color: var(--accent);
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
  }
  .branches {
    display: flex;
    align-items: flex-end;
    gap: var(--space-3);
    color: var(--text-muted);
  }
  .branches .f {
    flex: 1;
    min-width: 0;
  }
  .branches > :global(svg) {
    margin-bottom: 9px;
    flex: none;
  }
  .f {
    display: grid;
    gap: 4px;
  }
  .lbl {
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .lblrow {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .grow {
    flex: 1;
  }
  .static {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: var(--control-h);
    padding: 0 var(--space-3);
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-md);
    color: var(--text);
    font-size: var(--text-md);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  textarea {
    padding: var(--space-2) var(--space-3);
    resize: vertical;
    line-height: 1.5;
    min-height: 140px;
  }
  .previewbox {
    min-height: 140px;
    max-height: 360px;
    overflow: auto;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border-field);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
  }
  .ai {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .tip {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
  .tip.bad {
    color: var(--danger);
  }
  .warn {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: var(--text-xs);
    color: var(--warning);
  }
  .chk {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-top: 1px solid var(--border);
    background: var(--bg-app);
  }
  .why {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
</style>
