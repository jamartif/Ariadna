<script lang="ts">
  import { t } from '$lib/i18n';
  import type { PrMergeStrategy } from '$lib/api/prTypes';
  import type { PullRequestsController } from '$lib/stores/pullRequests.svelte';
  import Banner from './Banner.svelte';
  import Icon from './Icon.svelte';

  /**
   * Completar (PR-08). La UI solo envía las opciones: la confirmación que nombra el repo, la PR
   * y la rama destino la abre Rust (diálogo nativo) y las políticas del proveedor mandan siempre;
   * si no se cumplen, llega `POLICY` con el motivo.
   */
  let { pr, onclose }: { pr: PullRequestsController; onclose: () => void } = $props();

  const caps = $derived(pr.caps);
  const strategies = $derived<PrMergeStrategy[]>(caps?.strategies ?? ['merge']);
  const d = $derived(pr.detail);
  let strategy = $state<PrMergeStrategy | null>(null);
  let deleteSource = $state(false);
  let workItems = $state(false);
  let auto = $state(false);
  let message = $state('');
  const uid = Math.random().toString(36).slice(2, 8);

  const chosen = $derived(strategy && strategies.includes(strategy) ? strategy : (strategies[0] ?? 'merge'));
  const LABEL: Record<PrMergeStrategy, 'pr.strategy.merge' | 'pr.strategy.squash' | 'pr.strategy.rebase' | 'pr.strategy.rebaseMerge'> = {
    merge: 'pr.strategy.merge',
    squash: 'pr.strategy.squash',
    rebase: 'pr.strategy.rebase',
    rebaseMerge: 'pr.strategy.rebaseMerge'
  };
  const HINT: Record<PrMergeStrategy, 'pr.strategy.merge.hint' | 'pr.strategy.squash.hint' | 'pr.strategy.rebase.hint' | 'pr.strategy.rebaseMerge.hint'> = {
    merge: 'pr.strategy.merge.hint',
    squash: 'pr.strategy.squash.hint',
    rebase: 'pr.strategy.rebase.hint',
    rebaseMerge: 'pr.strategy.rebaseMerge.hint'
  };
  const blocking = $derived((d?.policies ?? []).filter((p) => (p.status === 'failing' || p.status === 'broken') && p.blocking !== false));
  /** Mensaje de commit: solo tiene sentido al crear un commit de fusión o de squash. */
  const canMessage = $derived(chosen === 'merge' || chosen === 'squash' || chosen === 'rebaseMerge');

  async function submit(e: Event) {
    e.preventDefault();
    if (pr.busyAny) return;
    const ok = await pr.complete({
      strategy: chosen,
      deleteSourceBranch: deleteSource,
      completeWorkItems: workItems && !!caps?.workItems,
      autoComplete: auto && !!caps?.autoComplete,
      commitMessage: canMessage && message.trim() !== '' ? message.trim() : null
    });
    if (ok) onclose();
  }
</script>

<form class="complete" onsubmit={submit} aria-label={t('pr.complete.title')}>
  {#if d && (d.mergeStatus === 'conflicts' || blocking.length > 0 || d.isDraft)}
    <Banner kind="warning" title={t('pr.complete.warnTitle')}>
      <ul class="warns">
        {#if d.mergeStatus === 'conflicts'}<li>{t('pr.complete.warnConflicts')}</li>{/if}
        {#if blocking.length > 0}<li>{t('pr.complete.warnChecks', { names: blocking.map((p) => p.name).join(', ') })}</li>{/if}
        {#if d.isDraft}<li>{t('pr.complete.warnDraft')}</li>{/if}
      </ul>
      <p class="small">{t('pr.complete.policiesWin')}</p>
    </Banner>
  {/if}

  <fieldset>
    <legend>{t('pr.complete.strategy')}</legend>
    {#each strategies as s (s)}
      <label class="opt">
        <input type="radio" name="pr-strategy-{uid}" value={s} checked={chosen === s} onchange={() => (strategy = s)} />
        <span class="lbl">{t(LABEL[s])}</span>
        <span class="hint">{t(HINT[s])}</span>
      </label>
    {/each}
  </fieldset>

  <div class="checks">
    <label class="chk"><input type="checkbox" bind:checked={deleteSource} />{t('pr.complete.deleteSource', { branch: d?.sourceBranch ?? '' })}</label>
    {#if caps?.workItems && (d?.workItems.length ?? 0) > 0}
      <label class="chk"><input type="checkbox" bind:checked={workItems} />{t('pr.complete.workItems')}</label>
    {/if}
    {#if caps?.autoComplete}
      <label class="chk"><input type="checkbox" bind:checked={auto} />{t('pr.complete.auto')}</label>
    {/if}
  </div>

  {#if canMessage}
    <div class="field-row">
      <label for="pr-complete-msg-{uid}" class="lbl2">{t('pr.complete.message')}</label>
      <textarea id="pr-complete-msg-{uid}" class="field" rows="3" bind:value={message} placeholder={t('pr.complete.messagePlaceholder')} spellcheck="true"></textarea>
    </div>
  {/if}

  <p class="native"><Icon name="shield-check" size={13} />{t('pr.complete.native')}</p>

  <div class="row">
    <button type="button" class="btn btn-ghost" onclick={onclose}>{t('pr.cancel')}</button>
    <button type="submit" class="btn btn-primary" disabled={pr.busyAny}>
      {#if pr.busy === 'complete'}<Icon name="loader" size={14} spin />{:else}<Icon name="git-merge" size={14} />{/if}{auto ? t('pr.complete.submitAuto') : t('pr.complete.submit')}
    </button>
  </div>
</form>

<style>
  .complete {
    display: grid;
    gap: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
  }
  .warns {
    margin: var(--space-1) 0 0;
    padding-left: var(--space-4);
    font-size: var(--text-sm);
  }
  .small {
    margin: var(--space-1) 0 0;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  fieldset {
    display: grid;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    border: 0;
  }
  legend {
    padding: 0;
    margin-bottom: var(--space-1);
    font-size: var(--text-sm);
    font-weight: 650;
  }
  .opt {
    display: grid;
    grid-template-columns: auto 1fr;
    column-gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .opt:hover {
    background: var(--bg-hover);
  }
  .opt input {
    grid-row: span 2;
    margin: 0;
    align-self: center;
  }
  .lbl {
    font-size: var(--text-sm);
    font-weight: 600;
  }
  .hint {
    grid-column: 2;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .checks {
    display: grid;
    gap: var(--space-1);
  }
  .chk {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
  }
  .field-row {
    display: grid;
    gap: 4px;
  }
  .lbl2 {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text-muted);
  }
  textarea {
    padding: var(--space-2) var(--space-3);
    resize: vertical;
    line-height: 1.45;
  }
  .native {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .row {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
