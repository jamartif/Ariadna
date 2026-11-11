<script lang="ts">
  import { untrack } from 'svelte';
  import { t } from '$lib/i18n';
  import type { PrDiffSide, PrReviewThread } from '$lib/api/prTypes';
  import type { PullRequestsController } from '$lib/stores/pullRequests.svelte';
  import Icon from './Icon.svelte';

  /**
   * Hilo nuevo: general (`path` = null) o anclado a una línea de un archivo. La línea y el lado se
   * pueden escribir (alternativa de teclado a pulsar el botón de la línea en el diff).
   */
  interface Props {
    pr: PullRequestsController;
    path: string | null;
    line?: number;
    side?: PrDiffSide;
    /** Se llama con el hilo creado (o al cancelar, sin argumento). */
    onclose: (created?: PrReviewThread) => void;
  }
  let { pr, path, line = 1, side = 'new', onclose }: Props = $props();

  let text = $state('');
  let ln = $state(untrack(() => String(line)));
  let sd = $state<PrDiffSide>(untrack(() => side));
  let box = $state<HTMLTextAreaElement>();
  const uid = Math.random().toString(36).slice(2, 8);

  // Si el usuario pulsa otra línea del diff, el formulario se reajusta.
  $effect(() => {
    ln = String(line);
    sd = side;
  });

  const lineNum = $derived(Number.parseInt(ln, 10));
  const lineOk = $derived(path === null || (Number.isInteger(lineNum) && lineNum >= 1));
  const busy = $derived(pr.busy !== null);

  export function focus() {
    box?.focus();
  }

  $effect(() => {
    queueMicrotask(() => box?.focus());
  });

  async function send(e?: Event) {
    e?.preventDefault();
    if (text.trim() === '' || !lineOk || busy) return;
    const created = await pr.comment({
      kind: 'new',
      body: text.trim(),
      anchor: path === null ? null : { path, line: lineNum, side: sd }
    });
    if (created) {
      text = '';
      onclose(created);
    }
  }

  function key(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') void send(e);
    else if (e.key === 'Escape') {
      e.stopPropagation();
      onclose();
    }
  }
</script>

<form class="composer" onsubmit={send} aria-label={path === null ? t('pr.composer.general') : t('pr.composer.inline')}>
  {#if path !== null}
    <div class="anchor">
      <span class="where mono" title={path}><Icon name="file" size={12} />{path}</span>
      <label class="small" for="pr-c-line-{uid}">{t('pr.composer.line')}</label>
      <input id="pr-c-line-{uid}" class="field mono num" type="number" min="1" step="1" bind:value={ln} aria-invalid={lineOk ? undefined : 'true'} />
      <label class="small" for="pr-c-side-{uid}">{t('pr.composer.side')}</label>
      <select id="pr-c-side-{uid}" class="field" bind:value={sd}>
        <option value="new">{t('pr.composer.sideNew')}</option>
        <option value="old">{t('pr.composer.sideOld')}</option>
      </select>
    </div>
  {/if}
  <label class="sr-only" for="pr-c-text-{uid}">{t('pr.composer.text')}</label>
  <textarea id="pr-c-text-{uid}" bind:this={box} class="field" rows="4" bind:value={text} onkeydown={key} placeholder={path === null ? t('pr.composer.placeholderGeneral') : t('pr.composer.placeholderInline')} spellcheck="true"></textarea>
  <div class="row">
    <span class="tip">{t('pr.markdownHint')}</span>
    <button type="button" class="btn btn-ghost btn-sm" onclick={() => onclose()}>{t('pr.cancel')}</button>
    <button type="submit" class="btn btn-primary btn-sm" disabled={text.trim() === '' || !lineOk || busy}>
      {#if pr.busy === 'comment'}<Icon name="loader" size={12} spin />{/if}{t('pr.composer.send')}
    </button>
  </div>
</form>

<style>
  .composer {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--accent);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
  }
  .anchor {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .where {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
    font-size: var(--text-sm);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .small {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .anchor .field {
    width: auto;
    min-height: 28px;
    font-size: var(--text-sm);
  }
  .anchor input.field {
    width: 76px;
  }
  textarea {
    padding: var(--space-2) var(--space-3);
    resize: vertical;
    line-height: 1.45;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .tip {
    flex: 1;
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
</style>
