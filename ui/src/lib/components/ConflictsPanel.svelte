<script lang="ts">
  import { t, tn } from '$lib/i18n';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { conflictsOf } from '$lib/stores/conflicts.svelte';
  import { pathKey, pathText } from '$lib/util/gitpath';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';

  /** Columna central del estudio de conflictos: archivos, progreso y continuar/abortar. */
  let { tab }: { tab: TabState } = $props();

  const c = $derived(conflictsOf(tab));
  const into = $derived(tab.snapshot?.status.branchName ?? tab.info.branch ?? 'HEAD');
  const title = $derived(
    c.integration === 'merging'
      ? c.sourceLabel
        ? t('conflicts.title.merging', { source: c.sourceLabel, into })
        : t('conflicts.title.mergingUnknown')
      : c.integration === 'rebasing'
        ? t('conflicts.title.rebasing')
        : c.integration === 'cherryPicking'
          ? t('conflicts.title.cherryPicking')
          : ''
  );
  const progress = $derived(c.total === 0 ? 0 : Math.round((c.resolved.length / c.total) * 100));

  function listKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      c.step(e.key === 'ArrowDown' ? 1 : -1);
      queueMicrotask(() => document.querySelector<HTMLElement>('.cfile[aria-current="true"]')?.focus());
    }
  }

  $effect(() => {
    void tab.revision;
    void c.sync();
  });
</script>

<section class="cpanel" aria-label={t('conflicts.listRegion')}>
  {#if !c.active}
    <EmptyState icon="git-merge" title={t('conflicts.noIntegrationTitle')} body={t('conflicts.noIntegrationBody')} />
  {:else}
    <header class="head">
      <Icon name="git-merge" size={16} />
      <div class="titles">
        <h2>{title}</h2>
        <p class="sub">
          {t('conflicts.progress', { done: c.resolved.length, total: c.total })}
        </p>
      </div>
    </header>
    <div class="bar" role="progressbar" aria-label={t('conflicts.progress', { done: c.resolved.length, total: c.total })} aria-valuemin="0" aria-valuemax="100" aria-valuenow={progress}>
      <span style:width="{progress}%"></span>
    </div>

    <div class="scroll">
      <h3 class="sec" id="conflict-files-h">{t('conflicts.files')}</h3>
      {#if c.files.length === 0}
        <p class="empty"><Icon name="circle-check" size={14} />{t('conflicts.filesEmpty')}</p>
      {:else}
        <ul class="files" aria-labelledby="conflict-files-h">
          {#each c.files as f (pathKey(f.path))}
            {@const sel = c.selectedKey === pathKey(f.path)}
            {@const pend = c.mode(f) === 'editor' ? c.pending(f) : 0}
            <li>
              <button
                type="button"
                class="cfile"
                class:sel
                aria-current={sel ? 'true' : undefined}
                tabindex={sel ? 0 : -1}
                onclick={() => c.select(f)}
                onkeydown={listKey}
              >
                <Icon name="triangle-alert" size={14} />
                <span class="path mono" title={pathText(f.path)}>{pathText(f.path)}</span>
                <span class="meta">
                  <span class="chip kind">{t(`conflict.${f.kind}`)}</span>
                  {#if c.mode(f) === 'sides'}
                    <span class="note">{t('conflicts.versionOnly')}</span>
                  {:else if pend > 0}
                    <span class="note">{tn('conflicts.hunksPending', pend)}</span>
                  {/if}
                  {#if c.isDirty(f)}<span class="dot" title={t('conflicts.edited')} role="img" aria-label={t('conflicts.edited')}></span>{/if}
                </span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}

      {#if c.resolved.length > 0}
        <h3 class="sec">{t('conflicts.resolvedList')}</h3>
        <ul class="done">
          {#each c.resolved as p (p)}
            <li><Icon name="check" size={13} /><span class="mono">{p}</span></li>
          {/each}
        </ul>
      {/if}
    </div>

    <footer class="foot">
      {#if c.integration === 'merging' && c.canContinue}
        <label class="lbl" for="merge-message">{t('conflicts.messageLabel')}</label>
        <textarea
          id="merge-message"
          class="field mono"
          rows="3"
          spellcheck="false"
          value={c.message}
          oninput={(e) => c.setMessage(e.currentTarget.value)}
        ></textarea>
      {/if}

      {#if c.abortWarning}
        <div class="warn" role="alertdialog" aria-label={t('conflicts.abortMerge')}>
          <p><Icon name="triangle-alert" size={14} />{c.unsavedWarning}</p>
          <div class="row">
            <button type="button" class="btn btn-ghost" onclick={() => c.cancelAbortWarning()}>{t('conflicts.keepWorking')}</button>
            <button type="button" class="btn btn-danger" onclick={() => c.abort(true)}>{t('conflicts.abortAnyway')}</button>
          </div>
        </div>
      {/if}

      <div class="row">
        <button type="button" class="btn btn-danger" disabled={c.busy !== null} onclick={() => c.abort()}>
          {#if c.busy === 'abort'}<Icon name="loader" size={14} spin />{:else}<Icon name="x" size={14} />{/if}
          {c.integration === 'merging' ? t('conflicts.abortMerge') : t('conflicts.abort')}
        </button>
        <span class="grow"></span>
        <button
          type="button"
          class="btn btn-primary"
          disabled={!c.canContinue}
          aria-describedby="continue-hint"
          onclick={() => c.continueIntegration()}
        >
          {#if c.busy === 'continue'}<Icon name="loader" size={14} spin />{:else}<Icon name="arrow-right" size={14} />{/if}
          {c.integration === 'merging' ? t('conflicts.continueMerge') : t('conflicts.continue')}
        </button>
      </div>
      <p id="continue-hint" class="hint" aria-live="polite">
        {#if c.files.length > 0}{tn('conflicts.continueBlocked', c.files.length)}{:else}{t('conflicts.doneBody')}{/if}
      </p>
    </footer>
  {/if}
</section>

<style>
  .cpanel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg-surface);
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4) var(--space-2);
    color: var(--st-unmerged-fg);
  }
  .titles {
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: var(--text-md);
    color: var(--text);
    overflow-wrap: anywhere;
  }
  .sub {
    margin: 2px 0 0;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .bar {
    height: 3px;
    margin: 0 var(--space-4);
    border-radius: var(--pill);
    background: var(--bg-muted);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--success);
    transition: width var(--dur) var(--ease);
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--space-2) var(--space-3);
  }
  .sec {
    margin: var(--space-3) var(--space-2) var(--space-1);
    font-size: var(--text-xs);
    font-weight: 650;
    color: var(--text-subtle);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .files,
  .done {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 2px;
  }
  .cfile {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    grid-template-areas: 'i p' '. m';
    column-gap: var(--space-2);
    row-gap: 2px;
    width: 100%;
    min-height: var(--target-min);
    padding: var(--space-2) var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    background: none;
    color: var(--text);
    text-align: left;
    font: inherit;
  }
  .cfile :global(svg) {
    grid-area: i;
    color: var(--st-unmerged-fg);
    margin-top: 2px;
  }
  .cfile:hover {
    background: var(--bg-hover);
  }
  .cfile.sel {
    background: var(--bg-selected);
    border-color: var(--border-strong);
  }
  .path {
    grid-area: p;
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }
  .meta {
    grid-area: m;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .chip.kind {
    background: var(--st-unmerged-bg);
    color: var(--st-unmerged-fg);
  }
  .note {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--warning);
  }
  .empty {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: var(--space-2);
    font-size: var(--text-sm);
    color: var(--success);
  }
  .done li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 2px var(--space-3);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .done li :global(svg) {
    color: var(--success);
  }
  .foot {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-top: 1px solid var(--border);
    background: var(--bg-app);
  }
  .lbl {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text-muted);
  }
  textarea.field {
    resize: vertical;
    min-height: 56px;
    font-size: var(--text-sm);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .grow {
    flex: 1;
  }
  .hint {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
  .warn {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--warning);
    border-radius: var(--radius-md);
    background: var(--warning-soft);
  }
  .warn p {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    margin: 0;
    font-size: var(--text-sm);
  }
</style>
