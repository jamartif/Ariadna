<script lang="ts">
  import { t } from '$lib/i18n';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { stashOf } from '$lib/stores/stash.svelte';
  import Icon from './Icon.svelte';

  /** Formulario «Guardar cambios en stash»: mensaje opcional, sin seguimiento y solo lo preparado. */
  let { tab }: { tab: TabState } = $props();

  const s = $derived(stashOf(tab));
  let message = $state('');
  let untracked = $state(false);
  let stagedOnly = $state(false);
  let input = $state<HTMLInputElement>();

  const busy = $derived(s.busy === 'push');
  const hasStaged = $derived(tab.stagedCount > 0);
  const empty = $derived(tab.info.changeCount === 0);

  $effect(() => {
    // Al abrirse el formulario el foco va al mensaje.
    input?.focus();
  });

  async function submit(e: Event) {
    e.preventDefault();
    if (busy || empty) return;
    const ok = await s.push({
      message: message.trim() === '' ? null : message.trim(),
      includeUntracked: stagedOnly ? false : untracked,
      stagedOnly
    });
    if (ok) {
      message = '';
      untracked = false;
      stagedOnly = false;
    }
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      s.closePush();
    }
  }
</script>

<form class="push" aria-label={t('stash.form.region')} onsubmit={submit}>
  <label class="lbl" for="stash-message">{t('stash.form.message')}</label>
  <input
    id="stash-message"
    bind:this={input}
    class="field"
    bind:value={message}
    placeholder={t('stash.form.messagePlaceholder')}
    autocomplete="off"
    spellcheck="true"
    onkeydown={key}
  />
  <label class="check">
    <input type="checkbox" bind:checked={untracked} disabled={stagedOnly} onkeydown={key} />
    <span>{t('stash.form.untracked')}</span>
  </label>
  <label class="check">
    <input type="checkbox" bind:checked={stagedOnly} disabled={!hasStaged} aria-describedby="stash-staged-hint" onkeydown={key} />
    <span>{t('stash.form.staged')}</span>
  </label>
  <p id="stash-staged-hint" class="hint">{t('stash.form.stagedHint')}</p>
  <div class="actions">
    <button type="button" class="btn btn-ghost" onclick={() => s.closePush()}>{t('stash.form.cancel')}</button>
    <button type="submit" class="btn btn-primary" disabled={busy || empty}>
      {#if busy}<Icon name="loader" size={14} spin />{:else}<Icon name="archive" size={14} />{/if}
      {t('stash.form.submit')}
    </button>
  </div>
  {#if empty}<p class="hint" role="status">{t('stash.saveDisabled')}</p>{/if}
</form>

<style>
  .push {
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
  }
  .check {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    color: var(--text);
    margin-top: var(--space-1);
  }
  .check input {
    accent-color: var(--accent);
  }
  .hint {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
</style>
