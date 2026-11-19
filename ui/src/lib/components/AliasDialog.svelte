<script lang="ts">
  import type { Api } from '$lib/api/api';
  import { t } from '$lib/i18n';
  import { repoAliases } from '$lib/stores/repoAliases.svelte';
  import { toasts } from '$lib/stores/toasts.svelte';
  import { focusTrap } from '$lib/util/focusTrap';
  import Icon from './Icon.svelte';

  /**
   * Alias de un repositorio: nombre corto para la barra superior, guardado en la configuración.
   * Se abre desde el menú contextual de la pestaña. Deja ver siempre la ruta completa, y vacío
   * significa «sin alias» (se vuelve a recortar el nombre).
   */
  let {
    api,
    path,
    name,
    onclose
  }: { api: Api; path: string; name: string; onclose: () => void } = $props();

  let value = $state('');

  // Valor inicial del campo: el alias guardado de ese repo (se relee si cambia la ruta).
  $effect(() => {
    value = repoAliases.get(path) ?? '';
  });
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let input = $state<HTMLInputElement>();

  const MAX = 24;
  const VALID = /^[\p{L}\p{N} ._/-]*$/u;

  function check(v: string): string | null {
    const alias = v.trim();
    if ([...alias].length > MAX || !VALID.test(alias)) return t('tabs.aliasInvalid');
    return null;
  }

  async function save(): Promise<void> {
    const why = check(value);
    if (why) {
      problem = why;
      return;
    }
    busy = true;
    try {
      await repoAliases.set(api, path, value);
      toasts.success(t('tabs.aliasSaved'));
      onclose();
    } catch (e) {
      problem = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) onclose(); }} onkeydown={(e) => { if (e.key === 'Escape') onclose(); }}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="alias-title" tabindex="-1" use:focusTrap>
    <header class="head">
      <h2 id="alias-title">{t('tabs.aliasTitle')}</h2>
      <button type="button" class="btn btn-ghost btn-icon" aria-label={t('tabs.closeHint')} onclick={onclose}>
        <Icon name="x" size={14} />
      </button>
    </header>
    <p class="path mono" title={path}>{t('tabs.aliasPath', { path })}</p>
    <p class="hint">{name}</p>
    <label class="fieldwrap">
      <span>{t('tabs.aliasTitle')}</span>
      <input
        class="field"
        type="text"
        maxlength={MAX}
        bind:this={input}
        bind:value
        data-testid="alias-input"
        aria-describedby="alias-hint"
        oninput={() => (problem = null)}
        onkeydown={(e) => {
          if (e.key === 'Enter') void save();
          if (e.key === 'Escape') onclose();
        }}
      />
    </label>
    <p class="hint" id="alias-hint">{t('tabs.aliasHint')}</p>
    {#if problem}
      <p class="bad" role="alert">{problem}</p>
    {/if}
    <div class="actions">
      <button type="button" class="btn" onclick={onclose}>{t('settings.cancel')}</button>
      <button type="button" class="btn btn-primary" disabled={busy} onclick={() => void save()} data-testid="alias-save">
        {t('settings.save')}
      </button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.45);
  }
  .dialog {
    width: min(420px, 92vw);
    padding: var(--space-4);
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--bg-surface);
    box-shadow: var(--shadow-lg, 0 18px 48px rgba(0, 0, 0, 0.45));
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
  }
  .head h2 {
    margin: 0;
    font-size: 15px;
  }
  .path {
    margin: 0 0 2px;
    font-size: 12px;
    color: var(--text-muted);
    overflow-wrap: anywhere;
  }
  .hint {
    margin: 0 0 var(--space-2);
    font-size: 12px;
    color: var(--text-muted);
  }
  .fieldwrap {
    display: grid;
    gap: 4px;
    font-size: 12px;
  }
  .bad {
    margin: var(--space-2) 0 0;
    color: var(--danger, #e5484d);
    font-size: 12px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-3);
  }
</style>
