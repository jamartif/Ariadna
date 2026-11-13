<script lang="ts">
  import type { TabState } from '$lib/stores/tab.svelte';
  import { focusTrap } from '$lib/util/focusTrap';
  import { t } from '$lib/i18n';
  import Icon from './Icon.svelte';
  import RemotesSection from './RemotesSection.svelte';
  import TagsSection from './TagsSection.svelte';

  /**
   * «Remotos y etiquetas»: lo que antes vivía dentro de la pestaña Ramas del centro.
   * El riel gestiona ramas; los remotos y las etiquetas (operaciones de repositorio)
   * se abren desde su cabecera para no cargar el árbol de ramas con otro menú.
   */
  let { tab, onclose }: { tab: TabState; onclose: () => void } = $props();

  let closeBtn = $state<HTMLButtonElement>();

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose}>
  <div
    class="dialog float"
    role="dialog"
    aria-modal="true"
    aria-labelledby="repo-title"
    tabindex="-1"
    data-testid="repo-dialog"
    use:focusTrap
    onclick={(e) => e.stopPropagation()}
    onkeydown={key}
  >
    <header class="head">
      <span class="badge"><Icon name="cloud" size={18} /></span>
      <div class="titles">
        <h2 id="repo-title">{t('repo.title')}</h2>
        <p>{t('repo.subtitle')}</p>
      </div>
      <button bind:this={closeBtn} type="button" class="btn btn-ghost btn-icon" aria-label={t('repo.close')} onclick={onclose}>
        <Icon name="x" size={16} />
      </button>
    </header>
    <div class="body">
      <RemotesSection {tab} />
      <TagsSection {tab} />
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: 8vh var(--space-4) var(--space-4);
    background: var(--bg-overlay);
    backdrop-filter: blur(3px);
    animation: fade-in var(--dur) var(--ease);
    overflow: auto;
  }
  .dialog {
    display: flex;
    flex-direction: column;
    width: min(680px, 100%);
    max-height: min(760px, 86vh);
    border-radius: var(--radius-lg);
    overflow: hidden;
    background: var(--bg-float);
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-4) var(--space-3);
    border-bottom: 1px solid var(--border);
  }
  .badge {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    flex: none;
    border-radius: var(--radius-md);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  h2 {
    font-size: var(--text-lg);
    font-weight: 650;
  }
  .titles p {
    margin-top: 2px;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding-bottom: var(--space-3);
  }
  .body :global(section:last-child) {
    border-bottom: none;
  }

  @media (forced-colors: active) {
    .badge {
      border: 1px solid ButtonText;
    }
  }
</style>
