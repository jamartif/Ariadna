<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { tagsOf } from '$lib/stores/tags.svelte';
  import { tagNameProblem } from '$lib/util/tagName';
  import Icon from './Icon.svelte';

  /**
   * Formulario de nueva etiqueta. `target` = SHA del commit donde crearla (`null` = HEAD).
   * `idPrefix` evita ids repetidos si hay dos formularios a la vez (lista y detalle del commit).
   */
  let {
    tab,
    target = null,
    idPrefix = 'tag-new',
    onclose
  }: { tab: TabState; target?: string | null; idPrefix?: string; onclose: () => void } = $props();

  const tags = $derived(tagsOf(tab));
  let name = $state('');
  let message = $state('');
  let nameInput = $state<HTMLInputElement>();

  const clash = $derived(tags.tags.some((x) => x.name === name.trim()));
  const problem = $derived(name === '' ? null : (tagNameProblem(name) ?? (clash ? 'tags.problem.exists' : null)));
  const busy = $derived(tags.isBusy('create', name.trim()));

  onMount(() => {
    nameInput?.focus();
    // Para avisar de un nombre repetido sin esperar al error de Rust.
    if (!tags.loaded) void tags.load();
  });

  async function submit(e: Event) {
    e.preventDefault();
    if (name.trim() === '' || problem || clash) return;
    if (await tags.create(name, target, message)) onclose();
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onclose();
    }
  }
</script>

<form class="form" onsubmit={submit} aria-label={t('tags.new')}>
  <p class="where">
    <Icon name="tag" size={12} />
    {target ? t('tags.newAt', { sha: target.slice(0, 7) }) : t('tags.newAtHead')}
  </p>
  <label class="lbl" for="{idPrefix}-name">{t('tags.name')}</label>
  <input
    id="{idPrefix}-name"
    bind:this={nameInput}
    class="field mono"
    bind:value={name}
    onkeydown={key}
    placeholder={t('tags.namePlaceholder')}
    autocomplete="off"
    spellcheck="false"
    aria-invalid={problem ? 'true' : undefined}
    aria-describedby="{idPrefix}-hint"
  />
  <p id="{idPrefix}-hint" class="hint" class:bad={problem !== null} aria-live="polite">
    {#if problem}<Icon name="circle-alert" size={12} />{t(problem)}{:else}{t('tags.nameHint')}{/if}
  </p>
  <label class="lbl" for="{idPrefix}-msg">{t('tags.message')}</label>
  <textarea id="{idPrefix}-msg" class="field" rows="3" bind:value={message} onkeydown={key} aria-describedby="{idPrefix}-msg-hint"></textarea>
  <p id="{idPrefix}-msg-hint" class="hint">{t('tags.messageHint')}</p>
  <div class="actions">
    <button type="button" class="btn btn-ghost" onclick={onclose}>{t('tags.cancel')}</button>
    <button type="submit" class="btn btn-primary" disabled={name.trim() === '' || problem !== null || clash || tags.isBusy()}>
      {#if busy}<Icon name="loader" size={14} spin />{/if}{t('tags.create')}
    </button>
  </div>
</form>

<style>
  .form {
    display: grid;
    gap: var(--space-1);
    margin: var(--space-1) var(--space-2) var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-surface);
  }
  .where {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 var(--space-1);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .lbl {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text-muted);
    margin-top: var(--space-1);
  }
  textarea.field {
    resize: vertical;
    min-height: 56px;
    font: inherit;
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
</style>
