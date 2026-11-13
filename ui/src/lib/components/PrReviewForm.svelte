<script lang="ts">
  import { t } from '$lib/i18n';
  import type { PrVote } from '$lib/api/prTypes';
  import type { PullRequestsController } from '$lib/stores/pullRequests.svelte';
  import { VOTE_HINT, VOTE_ICON, VOTE_TONE, voteLabel } from '$lib/util/pr';
  import Icon from './Icon.svelte';

  /**
   * Votar / revisar (PR-07). Solo se ofrece lo que el proveedor admite (`capabilities.votes`).
   * Los votos en contra (`reject`, `waitForAuthor`) abren una confirmación nativa en Rust: la UI
   * solo envía el voto; si el usuario cancela, no pasa nada.
   */
  let { pr, onclose }: { pr: PullRequestsController; onclose: () => void } = $props();

  const caps = $derived(pr.caps);
  const provider = $derived(caps?.provider);
  const votes = $derived<PrVote[]>(caps?.votes ?? []);
  const github = $derived(provider === 'github');
  let vote = $state<PrVote | null>(null);
  let comment = $state('');
  const uid = Math.random().toString(36).slice(2, 8);

  const chosen = $derived(vote ?? votes[0] ?? null);
  /** GitHub exige texto al solicitar cambios o comentar. */
  const needsComment = $derived(github && chosen !== null && chosen !== 'approve');
  const ready = $derived(chosen !== null && (!needsComment || comment.trim() !== ''));

  async function submit(e: Event) {
    e.preventDefault();
    if (!ready || chosen === null || pr.busyAny) return;
    if (await pr.vote(chosen, comment)) onclose();
  }
</script>

<form class="review" onsubmit={submit} aria-label={t('pr.review.title')}>
  <fieldset>
    <legend>{t('pr.review.title')}</legend>
    {#each votes as v (v)}
      <label class="opt" data-tone={VOTE_TONE[v]}>
        <input type="radio" name="pr-vote-{uid}" value={v} checked={chosen === v} onchange={() => (vote = v)} />
        <span class="lbl"><Icon name={VOTE_ICON[v]} size={14} />{t(voteLabel(v, provider))}</span>
        <span class="hint">{t(VOTE_HINT[v])}</span>
      </label>
    {/each}
  </fieldset>

  <div class="field-row">
    <label for="pr-review-comment-{uid}" class="lbl2">
      {needsComment ? t('pr.review.commentRequired') : t('pr.review.comment')}
    </label>
    <textarea id="pr-review-comment-{uid}" class="field" rows="3" bind:value={comment} aria-required={needsComment ? 'true' : undefined} spellcheck="true"></textarea>
  </div>

  <div class="row">
    <button type="button" class="btn btn-ghost" onclick={onclose}>{t('pr.cancel')}</button>
    <button type="submit" class="btn btn-primary" disabled={!ready || pr.busyAny}>
      {#if pr.busy === 'vote'}<Icon name="loader" size={14} spin />{/if}{t('pr.review.send')}
    </button>
  </div>
</form>

<style>
  .review {
    display: grid;
    gap: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
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
    align-items: center;
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
  }
  .lbl {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--text-sm);
    font-weight: 600;
  }
  .opt[data-tone='success'] .lbl {
    color: var(--success);
  }
  .opt[data-tone='danger'] .lbl {
    color: var(--danger);
  }
  .opt[data-tone='warning'] .lbl {
    color: var(--warning);
  }
  .hint {
    grid-column: 2;
    font-size: var(--text-xs);
    color: var(--text-muted);
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
  .row {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
