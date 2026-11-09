<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import type { Api } from '$lib/api/api';
  import { t } from '$lib/i18n';
  import type { MessageKey } from '$lib/i18n';
  import { SETTINGS_SECTIONS, SettingsPanel, settingsUi, type SettingsSection } from '$lib/stores/settingsPanel.svelte';
  import { focusTrap } from '$lib/util/focusTrap';
  import Banner from './Banner.svelte';
  import Icon from './Icon.svelte';
  import type { IconName } from './icons';
  import AboutSection from './settings/AboutSection.svelte';
  import AccountsSection from './settings/AccountsSection.svelte';
  import AiSection from './settings/AiSection.svelte';
  import GeneralSection from './settings/GeneralSection.svelte';
  import GitSection from './settings/GitSection.svelte';

  let { api, tabId, onclose }: { api: Api; tabId?: string; onclose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  const panel = new SettingsPanel(api, tabId);
  const ICONS: Record<SettingsSection, IconName> = { general: 'sliders', git: 'git-branch', ai: 'sparkles', accounts: 'key', about: 'info' };

  onMount(() => {
    void panel.load();
  });
  onDestroy(() => panel.dispose());

  const section = $derived(settingsUi.section);

  function close() {
    panel.revert(); // devuelve la vista previa de tema y tamaño
    onclose();
  }

  async function save() {
    await panel.save();
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      close();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      if (panel.dirty) void save();
    }
  }

  // Pestañas verticales (patrón WAI-ARIA): flechas, Inicio y Fin; el foco sigue a la selección.
  function navKey(e: KeyboardEvent, current: SettingsSection) {
    const i = SETTINGS_SECTIONS.indexOf(current);
    let next = i;
    if (e.key === 'ArrowDown' || e.key === 'ArrowRight') next = (i + 1) % SETTINGS_SECTIONS.length;
    else if (e.key === 'ArrowUp' || e.key === 'ArrowLeft') next = (i - 1 + SETTINGS_SECTIONS.length) % SETTINGS_SECTIONS.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = SETTINGS_SECTIONS.length - 1;
    else return;
    e.preventDefault();
    const target = SETTINGS_SECTIONS[next] as SettingsSection;
    settingsUi.section = target;
    document.getElementById(`settings-tab-${target}`)?.focus();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={close}>
  <div
    class="dialog float"
    role="dialog"
    aria-modal="true"
    aria-labelledby="settings-title"
    tabindex="-1"
    use:focusTrap
    onclick={(e) => e.stopPropagation()}
    onkeydown={key}
  >
    <header class="head">
      <span class="badge"><Icon name="sliders" size={18} /></span>
      <div class="titles">
        <h2 id="settings-title">{t('settings.title')}</h2>
        <p>{t('settings.subtitle')}</p>
      </div>
      <button type="button" class="btn btn-ghost btn-icon" onclick={close} aria-label={t('settings.close')}>
        <Icon name="x" size={16} />
      </button>
    </header>

    <div class="layout">
      <div class="nav" role="tablist" aria-orientation="vertical" aria-label={t('settings.nav')}>
        {#each SETTINGS_SECTIONS as s (s)}
          <button
            type="button"
            role="tab"
            id="settings-tab-{s}"
            class="navitem"
            aria-selected={section === s}
            aria-controls="settings-panel-{s}"
            tabindex={section === s ? 0 : -1}
            onclick={() => (settingsUi.section = s)}
            onkeydown={(e) => navKey(e, s)}
            data-autofocus={section === s ? '' : undefined}
          >
            <Icon name={ICONS[s]} size={15} />
            <span>{t(`settings.section.${s}` as MessageKey)}</span>
          </button>
        {/each}
      </div>

      <div class="panelwrap" role="tabpanel" id="settings-panel-{section}" aria-labelledby="settings-tab-{section}" tabindex="-1">
        {#if panel.loadError}
          <Banner kind="error" title={t('settings.loadFailed')} message={panel.loadError.message} detail={panel.loadError.detail}>
            {#snippet actions()}
              <button type="button" class="btn" onclick={() => void panel.load()}>
                <Icon name="refresh-cw" size={14} />{t('common.retry')}
              </button>
            {/snippet}
          </Banner>
        {:else if !panel.draft || !panel.view}
          <div class="loading" aria-busy="true">
            <span class="skeleton" style:width="40%" style:height="18px"></span>
            <span class="skeleton" style:width="100%" style:height="44px"></span>
            <span class="skeleton" style:width="100%" style:height="44px"></span>
            <span class="sr-only">{t('settings.loading')}</span>
          </div>
        {:else}
          {#if panel.view.load.warning}
            <div class="warn">
              <Banner kind="warning" title={t('settings.loadWarningTitle')} message={panel.view.load.warning} />
            </div>
          {/if}
          {#if section === 'general'}
            <GeneralSection {panel} />
          {:else if section === 'git'}
            <GitSection {panel} />
          {:else if section === 'ai'}
            <AiSection {panel} />
          {:else if section === 'accounts'}
            <AccountsSection {panel} />
          {:else}
            <AboutSection {panel} />
          {/if}
        {/if}
      </div>
    </div>

    <footer class="foot">
      {#if panel.saveError}
        <p class="err" role="alert">
          <Icon name="circle-alert" size={14} />
          <span>{t('settings.saveFailed')}: {panel.saveError.message}</span>
        </p>
      {:else if panel.dirty}
        <p class="dirty" role="status"><Icon name="info" size={14} />{t('settings.unsaved')}</p>
      {/if}
      <span class="spacer"></span>
      {#if panel.dirty}
        <button type="button" class="btn" onclick={close} disabled={panel.saving}>{t('settings.cancel')}</button>
        <button type="button" class="btn btn-primary" onclick={() => void save()} disabled={panel.saving || panel.fontProblem}>
          {#if panel.saving}<Icon name="loader" size={14} spin />{t('settings.saving')}{:else}{t('settings.save')}{/if}
        </button>
      {:else}
        <button type="button" class="btn" onclick={close}>{t('settings.done')}</button>
      {/if}
    </footer>
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
    padding: 6vh var(--space-4) var(--space-4);
    background: var(--bg-overlay);
    backdrop-filter: blur(3px);
    animation: fade-in var(--dur) var(--ease);
    overflow: auto;
  }
  .dialog {
    display: flex;
    flex-direction: column;
    width: min(820px, 100%);
    max-height: 88vh;
    border-radius: 16px;
    overflow: hidden;
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
    width: 36px;
    height: 36px;
    flex: none;
    border-radius: 11px;
    background: var(--grad-accent-soft);
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
    color: var(--accent);
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  h2 {
    font-size: var(--text-lg);
    font-weight: 650;
    letter-spacing: -0.015em;
  }
  .titles p {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }
  .layout {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: none;
    width: 212px;
    padding: var(--space-3) var(--space-2);
    border-right: 1px solid var(--border);
    background: var(--bg-subtle);
    overflow: auto;
  }
  .navitem {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 34px;
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-md);
    background: none;
    color: var(--text-muted);
    font: inherit;
    font-size: var(--text-sm);
    text-align: left;
    cursor: pointer;
  }
  .navitem:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .navitem[aria-selected='true'] {
    background: var(--accent-soft);
    color: var(--text);
    font-weight: 600;
  }
  .navitem:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
  }
  .panelwrap {
    flex: 1;
    min-width: 0;
    padding: var(--space-4);
    overflow: auto;
    outline: none;
  }
  .warn {
    margin-bottom: var(--space-4);
  }
  .loading {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-top: 1px solid var(--border);
  }
  .spacer {
    flex: 1;
  }
  .dirty,
  .err {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    font-size: var(--text-sm);
  }
  .dirty {
    color: var(--text-muted);
  }
  .err {
    color: var(--danger);
  }

  /* ---- Estilos compartidos por las secciones (scoped al panel) ---- */
  .panelwrap :global(.sx) {
    display: flex;
    flex-direction: column;
    gap: var(--space-5, 20px);
    max-width: 560px;
  }
  .panelwrap :global(.sx h3) {
    margin: 0 0 2px;
    font-size: var(--text-md, 1rem);
    font-weight: 650;
    letter-spacing: -0.01em;
  }
  .panelwrap :global(.sx .group) {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    margin: 0;
    padding: 0;
    border: 0;
  }
  .panelwrap :global(.sx .group > .lbl),
  .panelwrap :global(.sx legend) {
    padding: 0;
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .panelwrap :global(.sx .hint) {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .panelwrap :global(.sx .row) {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .panelwrap :global(.sx .row > .field) {
    flex: 1;
    min-width: 0;
  }
  .panelwrap :global(.sx .seg) {
    display: inline-flex;
    align-self: flex-start;
    padding: 2px;
    gap: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
  }
  .panelwrap :global(.sx .opt) {
    position: relative;
    display: inline-flex;
    align-items: center;
    min-height: 30px;
    padding: 0 var(--space-4);
    border-radius: calc(var(--radius-md) - 2px);
    font-size: var(--text-sm);
    color: var(--text-muted);
    cursor: pointer;
  }
  .panelwrap :global(.sx .opt.on) {
    background: var(--bg-raised);
    color: var(--text);
    font-weight: 600;
    box-shadow: var(--elev-1);
  }
  .panelwrap :global(.sx .opt input) {
    position: absolute;
    inset: 0;
    opacity: 0;
    margin: 0;
    cursor: pointer;
  }
  .panelwrap :global(.sx .opt:has(input:focus-visible)) {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .panelwrap :global(.sx .check) {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    cursor: pointer;
  }
  .panelwrap :global(.sx .check input) {
    flex: none;
    width: 18px;
    height: 18px;
    margin: 1px 0 0;
    accent-color: var(--accent);
  }
  .panelwrap :global(.sx .check input:focus-visible) {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .panelwrap :global(.sx .check .main) {
    display: block;
    font-size: var(--text-sm);
    font-weight: 600;
  }
  .panelwrap :global(.sx .status) {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    font-size: var(--text-sm);
  }
  .panelwrap :global(.sx .status.ok) {
    color: var(--success);
  }
  .panelwrap :global(.sx .status.bad) {
    color: var(--danger);
  }
  .panelwrap :global(.sx .mono) {
    font-family: var(--font-mono);
    word-break: break-all;
  }
  @media (max-width: 640px) {
    .layout {
      flex-direction: column;
    }
    .nav {
      width: auto;
      flex-direction: row;
      overflow-x: auto;
      border-right: 0;
      border-bottom: 1px solid var(--border);
    }
  }
  @media (forced-colors: active) {
    .navitem[aria-selected='true'] {
      outline: 2px solid Highlight;
    }
  }
</style>
