<script lang="ts">
  import { t, tn } from '$lib/i18n';
  import type { PrDetailView } from '$lib/stores/pullRequests.svelte';
  import { prOf } from '$lib/stores/pullRequests.svelte';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { safeHttpsUrl } from '$lib/util/markdown';
  import { CHECK_ICON, CHECK_LABEL, CHECK_TONE, MERGE_LABEL, STATE_ICON, STATE_LABEL, STATE_TONE } from '$lib/util/pr';
  import { relativeTime } from '$lib/util/time';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';
  import MarkdownView from './MarkdownView.svelte';
  import PrBadge from './PrBadge.svelte';
  import PrComposer from './PrComposer.svelte';
  import PrCompleteForm from './PrCompleteForm.svelte';
  import PrCreateForm from './PrCreateForm.svelte';
  import PrError from './PrError.svelte';
  import PrFiles from './PrFiles.svelte';
  import PrReviewForm from './PrReviewForm.svelte';
  import PrReviewers from './PrReviewers.svelte';
  import PrThread from './PrThread.svelte';

  /** Detalle de la PR seleccionada (columna derecha), o el formulario de crear. */
  let { tab }: { tab: TabState } = $props();

  const pr = $derived(prOf(tab));
  const d = $derived(pr.detail);
  const provider = $derived(pr.support?.provider);
  const github = $derived(provider === 'github');
  let panelOpen = $state<'review' | 'complete' | null>(null);
  let writing = $state(false);
  let formHost = $state<HTMLElement | null>(null);
  let prevPanel: 'review' | 'complete' | null = null;

  // Teclado y lectores: al abrir un formulario el foco entra en él y, al cerrarlo, vuelve al botón que lo abrió.
  $effect(() => {
    const cur = panelOpen;
    const prev = prevPanel;
    prevPanel = cur;
    if (cur) {
      queueMicrotask(() => formHost?.querySelector<HTMLElement>('input:checked, input:not([disabled]), textarea, select, button:not([disabled])')?.focus());
    } else if (prev) {
      queueMicrotask(() => {
        const a = document.activeElement;
        if (!a || a === document.body || a.closest('[data-pr-form]')) document.getElementById(`pr-act-${prev}`)?.focus();
      });
    }
  });

  function formKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && panelOpen) {
      e.stopPropagation();
      panelOpen = null;
    }
  }

  const VIEWS: { id: PrDetailView; label: 'pr.tab.overview' | 'pr.tab.files' | 'pr.tab.conversation' }[] = [
    { id: 'overview', label: 'pr.tab.overview' },
    { id: 'files', label: 'pr.tab.files' },
    { id: 'conversation', label: 'pr.tab.conversation' }
  ];

  // Cambiar de PR cierra los formularios de acción.
  $effect(() => {
    void pr.selectedId;
    panelOpen = null;
    writing = false;
  });

  function tabKey(e: KeyboardEvent, i: number) {
    let n = i;
    if (e.key === 'ArrowRight') n = (i + 1) % VIEWS.length;
    else if (e.key === 'ArrowLeft') n = (i - 1 + VIEWS.length) % VIEWS.length;
    else if (e.key === 'Home') n = 0;
    else if (e.key === 'End') n = VIEWS.length - 1;
    else return;
    e.preventDefault();
    const v = VIEWS[n];
    if (!v) return;
    pr.setView(v.id);
    queueMicrotask(() => document.getElementById(`pr-view-${v.id}`)?.focus());
  }

  const active = $derived(d?.state === 'active');
  /** La lista no tiene PR: el panel derecho no repite el estado vacío ni su botón. */
  const listEmpty = $derived(pr.supported && pr.listLoaded && !pr.listLoading && !pr.listError && pr.items.length === 0);
  const generalThreads = $derived(pr.threads.filter((th) => th.anchor === null));
  const inlineThreads = $derived(pr.threads.filter((th) => th.anchor !== null));
  const webUrl = $derived(d ? safeHttpsUrl(d.webUrl) : null);

  async function abandon() {
    if (await pr.abandon()) panelOpen = null;
  }
</script>

<section class="detail" aria-label={t('pr.detail.region')}>
  {#if pr.creating}
    <PrCreateForm {tab} />
  {:else if pr.selectedId === null}
    {#if listEmpty}
      <!-- La lista ya explica que no hay PR y ofrece crearla: aquí solo queda el lienzo vacío. -->
      <div class="pick-blank" aria-hidden="true"><Icon name="git-merge" size={28} /></div>
    {:else}
      <EmptyState icon="git-merge" title={t('pr.pick.title')} body={t('pr.pick.body')}>
        <button type="button" class="btn btn-primary" onclick={() => pr.startCreate()}><Icon name="plus" size={14} />{t('pr.new')}</button>
      </EmptyState>
    {/if}
  {:else if pr.detailError && !d}
    <div class="pad"><PrError error={pr.detailError} onretry={() => pr.selectedId !== null && pr.loadDetail(pr.selectedId)} onback={() => pr.closeDetail()} /></div>
  {:else if !d}
    <div class="pad" role="status" aria-busy="true" aria-label={t('pr.detail.loading')}>
      <div class="skeleton sk-title"></div>
      <div class="skeleton sk-line"></div>
      <div class="skeleton sk-block"></div>
    </div>
  {:else}
    <header class="head">
      <div class="titles">
        <h2><span class="id mono">#{d.id}</span> {d.title}</h2>
        <p class="meta">
          <PrBadge tone={STATE_TONE[d.state]} icon={STATE_ICON[d.state]} label={t(STATE_LABEL[d.state])} />
          {#if d.isDraft}<PrBadge tone="muted" icon="pencil" label={t('pr.draft')} />{/if}
          {#if d.mergeStatus === 'conflicts'}<PrBadge tone="danger" icon="triangle-alert" label={t(MERGE_LABEL.conflicts)} />{/if}
          {#if d.autoComplete}<PrBadge tone="accent" icon="zap" label={t('pr.autoCompleteOn')} />{/if}
          <span class="by">{t('pr.detail.by', { author: d.author.displayName })} · <time datetime={d.createdAt} title={d.createdAt}>{relativeTime(d.createdAt)}</time></span>
        </p>
        <p class="branches mono" aria-label={t('pr.detail.branches', { source: d.sourceBranch, target: d.targetBranch })}>
          <Icon name="git-branch" size={13} /><span class="b">{d.sourceBranch}</span><Icon name="arrow-right" size={13} /><span class="b">{d.targetBranch}</span>
          {#if d.isFork}<PrBadge tone="info" icon="git-fork" label={t('pr.fork')} />{/if}
        </p>
      </div>
      <div class="head-actions">
        <button type="button" class="btn btn-icon" onclick={() => void pr.refresh()} aria-label={t('pr.refresh')} title={t('pr.refresh')}><Icon name="refresh-cw" size={14} spin={pr.detailLoading} /></button>
        {#if webUrl}
          <button type="button" class="btn btn-sm" onclick={() => void pr.openUrl(webUrl)}><Icon name="external-link" size={13} />{t('pr.openWeb')}</button>
        {/if}
      </div>
    </header>

    <div class="actions" role="group" aria-label={t('pr.actions')}>
      {#if active}
        {#if (pr.caps?.votes.length ?? 0) > 0}
          <button type="button" class="btn" id="pr-act-review" aria-expanded={panelOpen === 'review'} onclick={() => (panelOpen = panelOpen === 'review' ? null : 'review')} disabled={pr.busyAny}>
            <Icon name="circle-check" size={14} />{t('pr.review.open')}
          </button>
        {/if}
        <button type="button" class="btn" onclick={() => void pr.checkout()} disabled={pr.busyAny}>
          {#if pr.busy === 'checkout'}<Icon name="loader" size={14} spin />{:else}<Icon name="download" size={14} />{/if}{t('pr.checkout')}
        </button>
        <button type="button" class="btn btn-primary" id="pr-act-complete" aria-expanded={panelOpen === 'complete'} onclick={() => (panelOpen = panelOpen === 'complete' ? null : 'complete')} disabled={pr.busyAny}>
          <Icon name="git-merge" size={14} />{t('pr.complete.open')}
        </button>
        <button type="button" class="btn btn-danger" onclick={() => void abandon()} disabled={pr.busyAny}>
          {#if pr.busy === 'abandon'}<Icon name="loader" size={14} spin />{:else}<Icon name="ban" size={14} />{/if}{github ? t('pr.close') : t('pr.abandon')}
        </button>
      {:else if d.state === 'abandoned'}
        <button type="button" class="btn btn-primary" onclick={() => void pr.reopen()} disabled={pr.busyAny}>
          {#if pr.busy === 'reopen'}<Icon name="loader" size={14} spin />{:else}<Icon name="undo" size={14} />{/if}{t('pr.reopen')}
        </button>
      {:else}
        <p class="note"><Icon name="info" size={12} />{t('pr.completedNote')}</p>
      {/if}
    </div>

    {#if pr.actionError}
      <div class="pad-x">
        <PrError error={pr.actionError} ondismiss={() => pr.dismissActionError()} onretry={pr.actionError.code === 'CONFLICT' || pr.actionError.code === 'NOT_FOUND' ? () => void pr.refresh() : undefined} />
      </div>
    {/if}

    {#if (panelOpen === 'review' || panelOpen === 'complete') && active}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="pad-x" data-pr-form bind:this={formHost} onkeydown={formKey}>
        {#if panelOpen === 'review'}
          <PrReviewForm {pr} onclose={() => (panelOpen = null)} />
        {:else}
          <PrCompleteForm {pr} onclose={() => (panelOpen = null)} />
        {/if}
      </div>
    {/if}

    <div class="views" role="tablist" aria-label={t('pr.views')}>
      {#each VIEWS as v, i (v.id)}
        <button
          type="button"
          role="tab"
          id="pr-view-{v.id}"
          class="vtab"
          class:on={pr.view === v.id}
          aria-selected={pr.view === v.id}
          aria-controls="pr-viewpanel-{v.id}"
          tabindex={pr.view === v.id ? 0 : -1}
          onclick={() => pr.setView(v.id)}
          onkeydown={(e) => tabKey(e, i)}
        >
          {t(v.label)}
          {#if v.id === 'files'}<span class="chip">{d.files.length}</span>{/if}
          {#if v.id === 'conversation' && pr.threads.length > 0}<span class="chip" title={tn('pr.activeThreads', pr.activeThreads)}>{pr.activeThreads}</span>{/if}
        </button>
      {/each}
    </div>

    <div class="body" role="tabpanel" id="pr-viewpanel-{pr.view}" aria-labelledby="pr-view-{pr.view}" tabindex="-1">
      {#if pr.view === 'overview'}
        <div class="scroll">
          <section class="blk" aria-labelledby="pr-h-desc">
            <h3 id="pr-h-desc">{t('pr.detail.description')}</h3>
            <MarkdownView source={d.description} onopen={(u) => void pr.openUrl(u)} empty={t('pr.detail.noDescription')} />
          </section>

          {#if d.mergeStatus === 'conflicts' || d.mergeDetail}
            <section class="blk" aria-labelledby="pr-h-merge">
              <h3 id="pr-h-merge">{t('pr.detail.merge')}</h3>
              <p class="line">
                <Icon name={d.mergeStatus === 'conflicts' ? 'triangle-alert' : d.mergeStatus === 'clean' ? 'circle-check' : 'info'} size={14} />
                {t(MERGE_LABEL[d.mergeStatus])}{#if d.mergeDetail && d.mergeStatus !== 'clean'}<span class="muted mono">{' · '}{d.mergeDetail}</span>{/if}
              </p>
            </section>
          {/if}

          <section class="blk" aria-labelledby="pr-h-rev">
            <h3 id="pr-h-rev">{t('pr.detail.reviewers')}</h3>
            <PrReviewers reviewers={d.reviewers} provider={provider} full />
          </section>

          <section class="blk" aria-labelledby="pr-h-pol">
            <h3 id="pr-h-pol">{t('pr.detail.policies')}</h3>
            {#if d.policies.length === 0}
              <p class="muted">{t('pr.detail.noPolicies')}</p>
            {:else}
              <ul class="pol">
                {#each d.policies as p, i (i)}
                  {@const url = p.url ? safeHttpsUrl(p.url) : null}
                  <li data-tone={CHECK_TONE[p.status]}>
                    <span class="pi"><Icon name={CHECK_ICON[p.status]} size={14} /></span>
                    <span class="pn">{p.name}</span>
                    {#if p.blocking === true}<span class="tag">{t('pr.check.blocking')}</span>{/if}
                    <span class="ps">{t(CHECK_LABEL[p.status])}</span>
                    {#if url}<button type="button" class="btn btn-sm btn-ghost btn-icon" onclick={() => void pr.openUrl(url)} aria-label={t('pr.check.open', { name: p.name })}><Icon name="external-link" size={12} /></button>{/if}
                  </li>
                {/each}
              </ul>
            {/if}
          </section>

          {#if d.workItems.length > 0}
            <section class="blk" aria-labelledby="pr-h-wi">
              <h3 id="pr-h-wi">{t('pr.detail.workItems')}</h3>
              <ul class="plain">
                {#each d.workItems as w (w.id)}
                  {@const url = w.url ? safeHttpsUrl(w.url) : null}
                  <li>
                    <span class="mono">#{w.id}</span>
                    {#if url}<button type="button" class="btn btn-sm btn-ghost" onclick={() => void pr.openUrl(url)}>{t('pr.check.openGeneric')}<Icon name="external-link" size={12} /></button>{/if}
                  </li>
                {/each}
              </ul>
            </section>
          {/if}

          <section class="blk" aria-labelledby="pr-h-com">
            <h3 id="pr-h-com">{t('pr.detail.commits')} <span class="chip">{d.commits.length}</span></h3>
            {#if d.commits.length === 0}
              <p class="muted">{t('pr.detail.noCommits')}</p>
            {:else}
              <ul class="commits">
                {#each d.commits as c (c.sha)}
                  <li><code class="mono sha">{c.sha.slice(0, 7)}</code><span class="subj">{c.subject}</span><span class="muted">{c.author}{#if c.date}{' · '}{relativeTime(c.date)}{/if}</span></li>
                {/each}
              </ul>
            {/if}
          </section>
        </div>
      {:else if pr.view === 'files'}
        <PrFiles {pr} />
      {:else}
        <div class="scroll">
          {#if pr.threadsError}
            <PrError error={pr.threadsError} onretry={() => pr.selectedId !== null && pr.loadThreads(pr.selectedId)} />
          {:else if pr.threadsLoading && pr.threads.length === 0}
            <p class="muted" role="status" aria-busy="true"><Icon name="loader" size={14} spin />{t('pr.threads.loading')}</p>
          {/if}
          <div class="convhead">
            <h3>{t('pr.threads.title')}</h3>
            {#if !writing}
              <button type="button" class="btn btn-sm" onclick={() => (writing = true)} disabled={pr.busyAny}><Icon name="plus" size={12} />{t('pr.threads.new')}</button>
            {/if}
          </div>
          {#if writing}
            <PrComposer {pr} path={null} onclose={() => (writing = false)} />
          {/if}
          {#each generalThreads as th (th.id)}
            <PrThread thread={th} {pr} />
          {/each}
          {#if inlineThreads.length > 0}
            <h3 class="sub">{t('pr.threads.inline')} <span class="chip">{inlineThreads.length}</span></h3>
            {#each inlineThreads as th (th.id)}
              <PrThread thread={th} {pr} ongoto={(x) => pr.goToThread(x)} />
            {/each}
          {/if}
          {#if pr.threads.length === 0 && !pr.threadsLoading && !pr.threadsError}
            <p class="muted">{t('pr.threads.none')}</p>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</section>

<style>
  .detail {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg-surface);
  }
  .pad {
    padding: var(--space-4);
    display: grid;
    gap: var(--space-3);
  }
  .pick-blank {
    flex: 1;
    display: grid;
    place-items: center;
    min-height: 0;
    color: var(--text-subtle);
    opacity: 0.5;
  }
  .pad-x {
    padding: 0 var(--space-4) var(--space-3);
  }
  .sk-title {
    height: 28px;
    width: 60%;
  }
  .sk-line {
    height: 16px;
    width: 40%;
  }
  .sk-block {
    height: 160px;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-4) var(--space-2);
  }
  .titles {
    /* Panel estrecho: los botones de cabecera bajan en vez de apretar el título. */
    flex: 1 1 16rem;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: var(--text-lg);
    line-height: 1.3;
    overflow-wrap: anywhere;
  }
  .id {
    color: var(--text-muted);
    font-weight: 500;
  }
  .meta,
  .branches {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    margin: var(--space-2) 0 0;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .branches {
    font-size: var(--text-sm);
    color: var(--text);
  }
  .branches {
    flex-wrap: nowrap;
  }
  .branches :global(svg) {
    flex: none;
  }
  .b {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .head-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: none;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4) var(--space-3);
  }
  .note {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .views {
    display: flex;
    gap: 0;
    padding: 0 var(--space-3);
    border-bottom: 1px solid var(--border);
  }
  .vtab {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 34px;
    padding: 0 var(--space-3);
    border: none;
    border-bottom: 2px solid transparent;
    background: none;
    color: var(--text-muted);
    font-size: var(--text-sm);
    font-weight: 550;
    margin-bottom: -1px;
  }
  .vtab:hover {
    color: var(--text);
  }
  .vtab.on {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4) var(--space-5);
  }
  .blk {
    display: grid;
    gap: var(--space-2);
  }
  h3 {
    margin: 0;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-xs);
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  h3.sub {
    margin-top: var(--space-2);
  }
  .convhead {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .muted {
    color: var(--text-muted);
    font-size: var(--text-sm);
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: var(--text-sm);
  }
  .pol,
  .plain,
  .commits {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 4px;
  }
  .pol li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
  }
  .pol li[data-tone='success'] .pi {
    color: var(--success);
  }
  .pol li[data-tone='danger'] .pi {
    color: var(--danger);
  }
  .pol li[data-tone='warning'] .pi {
    color: var(--warning);
  }
  .pn {
    font-weight: 600;
  }
  .ps {
    margin-left: auto;
    color: var(--text-muted);
    font-size: var(--text-xs);
  }
  .tag {
    padding: 0 6px;
    border: 1px solid var(--border-strong);
    border-radius: var(--pill);
    font-size: var(--text-2xs);
    color: var(--text-muted);
  }
  .commits li {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    font-size: var(--text-sm);
    min-width: 0;
  }
  .sha {
    flex: none;
    color: var(--accent);
    background: none;
  }
  .subj {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Colores forzados: la selección no puede depender solo del color de fondo. */
  @media (forced-colors: active) {
    .vtab.on {
      border-bottom-color: Highlight;
    }
  }

  /* Ventana baja (zoom 200 %): cabecera y acciones más compactas para dejar sitio al
     cuerpo del detalle. */
  @media (max-height: 620px) {
    .head {
      padding: var(--space-2) var(--space-3) var(--space-1);
      gap: var(--space-2);
    }
    h2 {
      font-size: var(--text-md);
    }
    .meta {
      margin-top: var(--space-1);
    }
    .actions {
      padding: var(--space-1) var(--space-3) var(--space-2);
      gap: var(--space-1);
    }
    .vtab {
      min-height: 28px;
    }
  }
</style>
