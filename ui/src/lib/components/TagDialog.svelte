<script lang="ts">
  import { focusTrap } from '$lib/util/focusTrap';
  import { t } from '$lib/i18n';
  import type { TabState } from '$lib/stores/tab.svelte';
  import Icon from './Icon.svelte';
  import TagCreateForm from './TagCreateForm.svelte';

  /**
   * Ventana de «nueva etiqueta» para acciones que no viven en la pestaña de etiquetas: el menú
   * contextual de una rama (etiqueta su commit) y el detalle de un commit.
   */
  let { tab, target = null, onclose }: { tab: TabState; target?: string | null; onclose: () => void } = $props();

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    }
  }
</script>

<svelte:window onkeydown={key} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose}>
  <div
    class="dialog float"
    role="dialog"
    aria-modal="true"
    aria-labelledby="tag-dlg-title"
    tabindex="-1"
    data-testid="tag-dialog"
    use:focusTrap
    onclick={(e) => e.stopPropagation()}
  >
    <header class="head">
      <span class="badge"><Icon name="tag" size={16} /></span>
      <h2 id="tag-dlg-title">{t('tags.new')}</h2>
      <button type="button" class="btn btn-ghost btn-icon" aria-label={t('tags.cancel')} onclick={onclose}>
        <Icon name="x" size={14} />
      </button>
    </header>
    <TagCreateForm {tab} {target} idPrefix="tag-dialog" {onclose} />
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    padding: var(--space-4);
    background: color-mix(in srgb, var(--bg-app) 55%, transparent);
    backdrop-filter: blur(2px);
  }
  .dialog {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    width: min(460px, 100%);
    max-height: min(80vh, 640px);
    overflow: auto;
    padding: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg-surface);
    box-shadow: var(--elev-3);
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .head h2 {
    flex: 1;
    margin: 0;
    font-size: var(--text-md);
  }
  .badge {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    color: var(--accent);
  }
</style>
