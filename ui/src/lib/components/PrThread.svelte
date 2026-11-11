<script lang="ts">
  import { t } from '$lib/i18n';
  import type { PrReviewThread } from '$lib/api/prTypes';
  import type { PullRequestsController } from '$lib/stores/pullRequests.svelte';
  import { initials, THREAD_STATUS_LABEL } from '$lib/util/pr';
  import { relativeTime } from '$lib/util/time';
  import Icon from './Icon.svelte';
  import MarkdownView from './MarkdownView.svelte';
  import PrBadge from './PrBadge.svelte';

  /** Un hilo de comentarios: ver, responder y resolver. */
  interface Props {
    thread: PrReviewThread;
    pr: PullRequestsController;
    /** Resalta el hilo (se llegó desde una línea del diff). */
    highlighted?: boolean;
    /** Muestra `archivo:línea` y el botón para ir a la línea. */
    showAnchor?: boolean;
    ongoto?: (thread: PrReviewThread) => void;
  }
  let { thread, pr, highlighted = false, showAnchor = true, ongoto }: Props = $props();

  let replying = $state(false);
  let text = $state('');
  let box = $state<HTMLTextAreaElement>();
  const uid = Math.random().toString(36).slice(2, 8);

  /** GitHub no anida los comentarios generales: cada uno es un hilo propio. */
  const canReply = $derived(!thread.id.startsWith('issue:'));
  const busy = $derived(pr.busy === 'comment' || pr.busy === 'resolve');
  const tone = $derived(thread.status === 'active' ? 'warning' : thread.status === 'resolved' ? 'success' : 'muted');

  function startReply() {
    replying = true;
    queueMicrotask(() => box?.focus());
  }

  async function send(e?: Event) {
    e?.preventDefault();
    if (text.trim() === '' || busy) return;
    const res = await pr.comment({ kind: 'reply', threadId: thread.id, body: text.trim() });
    if (res) {
      text = '';
      replying = false;
    }
  }

  function key(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') void send(e);
    else if (e.key === 'Escape') {
      e.stopPropagation();
      replying = false;
    }
  }
</script>

<article class="thread" class:hl={highlighted} id="pr-thread-{thread.id}" aria-labelledby="pr-thread-h-{uid}" data-status={thread.status}>
  <header>
    <h4 id="pr-thread-h-{uid}" class="anchor">
      {#if thread.anchor && showAnchor}
        <Icon name="file" size={12} />
        <span class="path mono">{thread.anchor.path}</span><span class="ln mono">:{thread.anchor.line}</span>
        {#if ongoto}
          <button type="button" class="btn btn-sm btn-ghost" onclick={() => ongoto?.(thread)}>{t('pr.thread.goto')}</button>
        {/if}
      {:else if thread.anchor}
        <span class="ln mono">{t('pr.thread.line', { line: thread.anchor.line })}</span>
      {:else}
        <Icon name="info" size={12} />{t('pr.thread.general')}
      {/if}
    </h4>
    <PrBadge {tone} icon={thread.status === 'resolved' ? 'circle-check' : thread.status === 'closed' ? 'ban' : 'circle-alert'} label={t(THREAD_STATUS_LABEL[thread.status])} />
    {#if thread.outdated}<PrBadge tone="muted" icon="history" label={t('pr.thread.outdated')} title={t('pr.thread.outdatedHint')} />{/if}
  </header>

  <ol class="comments">
    {#each thread.comments as c (c.id)}
      <li>
        <div class="who">
          <span class="avatar" aria-hidden="true">{initials(c.author.displayName)}</span>
          <strong>{c.author.displayName}</strong>
          {#if c.createdAt}<time datetime={c.createdAt} title={c.createdAt}>{relativeTime(c.createdAt)}</time>{/if}
        </div>
        <MarkdownView source={c.body} onopen={(u) => void pr.openUrl(u)} />
      </li>
    {/each}
  </ol>

  {#if replying}
    <form class="reply" onsubmit={send}>
      <label class="sr-only" for="pr-reply-{uid}">{t('pr.thread.replyLabel')}</label>
      <textarea id="pr-reply-{uid}" bind:this={box} class="field" rows="3" bind:value={text} onkeydown={key} placeholder={t('pr.thread.replyPlaceholder')} spellcheck="true"></textarea>
      <div class="row">
        <span class="tip">{t('pr.markdownHint')}</span>
        <button type="button" class="btn btn-ghost btn-sm" onclick={() => (replying = false)}>{t('pr.cancel')}</button>
        <button type="submit" class="btn btn-primary btn-sm" disabled={text.trim() === '' || busy}>
          {#if pr.busy === 'comment'}<Icon name="loader" size={12} spin />{/if}{t('pr.thread.send')}
        </button>
      </div>
    </form>
  {:else}
    <footer>
      {#if canReply}
        <button type="button" class="btn btn-sm" onclick={startReply} disabled={busy}><Icon name="plus" size={12} />{t('pr.thread.reply')}</button>
      {/if}
      {#if thread.canResolve}
        <button type="button" class="btn btn-sm" disabled={busy} onclick={() => void pr.resolve(thread.id, thread.status !== 'resolved')}>
          <Icon name={thread.status === 'resolved' ? 'undo' : 'check'} size={12} />{thread.status === 'resolved' ? t('pr.thread.reactivate') : t('pr.thread.resolve')}
        </button>
      {/if}
    </footer>
  {/if}
</article>

<style>
  .thread {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-surface);
  }
  .thread[data-status='resolved'] {
    background: var(--bg-subtle);
  }
  .thread.hl {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-glow);
  }
  header {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .anchor {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
    margin: 0;
    font-size: var(--text-sm);
    font-weight: 600;
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .ln {
    color: var(--text-muted);
    flex: none;
  }
  .comments {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .who {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-bottom: 2px;
    font-size: var(--text-sm);
  }
  .who time {
    color: var(--text-subtle);
    font-size: var(--text-xs);
  }
  .avatar {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--bg-muted);
    border: 1px solid var(--border-strong);
    font-size: 8px;
    font-weight: 700;
  }
  footer,
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .row {
    justify-content: flex-end;
  }
  .tip {
    flex: 1;
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
  .reply {
    display: grid;
    gap: var(--space-2);
  }
  textarea {
    padding: var(--space-2) var(--space-3);
    resize: vertical;
    line-height: 1.45;
  }

  /* Colores forzados: la selección no puede depender solo del color de fondo. */
  @media (forced-colors: active) {
    .thread.hl {
      outline: 2px solid Highlight;
    }
  }
</style>
