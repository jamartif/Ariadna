<script lang="ts">
  import { onMount } from 'svelte';
  import CloneDialog from '$lib/components/CloneDialog.svelte';
  import CommandPalette from '$lib/components/CommandPalette.svelte';
  import EmptyState from '$lib/components/EmptyState.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import TitleBar from '$lib/components/TitleBar.svelte';
  import Toasts from '$lib/components/Toasts.svelte';
  import Welcome from '$lib/components/Welcome.svelte';
  import Workspace from '$lib/components/Workspace.svelte';
  import { t } from '$lib/i18n';
  import { installShortcuts, registerHandlers } from '$lib/shortcuts/registry';
  import { HIBERNATE_CHECK_MS, type AppStore } from '$lib/stores/app.svelte';
  import { buildActions } from '$lib/stores/commands';
  import { settings } from '$lib/stores/settings.svelte';
  import SettingsDialog from '$lib/components/SettingsDialog.svelte';
  import { settingsUi, startSettingsSync } from '$lib/stores/settingsPanel.svelte';

  let { app }: { app: AppStore } = $props();

  const entry = $derived(app.activeEntry);
  const title = $derived(entry?.kind === 'repo' ? `${entry.info.name}${entry.info.branch ? ` — ${entry.info.branch}` : ''}` : '');

  // El título de la ventana (barra de tareas, Alt+Tab) refleja el repo activo.
  $effect(() => {
    document.title = title ? `${title} · Ariadna` : 'Ariadna';
  });

  onMount(() => {
    settings.apply();
    void app.init();
    const stopSync = startSettingsSync(app.api);
    const off = installShortcuts();
    const onFocus = () => app.refreshActiveOnFocus();
    const hibernation = setInterval(() => app.hibernateIdle(), HIBERNATE_CHECK_MS);
    window.addEventListener('focus', onFocus);
    const gotos = Object.fromEntries(
      Array.from({ length: 9 }, (_, i) => [`tab.goto.${i + 1}`, () => (app.gotoIndex(i + 1), true)])
    );
    const offHandlers = registerHandlers({
      ...gotos,
      'tab.new': () => (app.newTab(), true),
      'tab.close': () => {
        if (app.activeId) void app.close(app.activeId);
        return true;
      },
      'tab.next': () => (app.step(1), true),
      'tab.prev': () => (app.step(-1), true),
      'palette.open': () => ((app.paletteOpen = !app.paletteOpen), true),
      'clone.open': () => (app.openClone(), true),
      'repo.open': () => (void app.openViaDialog(), true),
      'settings.open': () => (settingsUi.toggle(), true),
      'changes.refresh': () => {
        // Ctrl/⌘+R siempre se consume: si no, la WebView recargaría la página entera y se
        // perdería el estado de la interfaz.
        const tab = entry?.kind === 'repo' ? app.activeTab : null;
        if (tab) void tab.refresh();
        return true;
      },
      'changes.stageAll': () => {
        const tab = entry?.kind === 'repo' ? app.activeTab : null;
        if (!tab) return false;
        void tab.stageAll();
        return true;
      },
      'changes.unstageAll': () => {
        const tab = entry?.kind === 'repo' ? app.activeTab : null;
        if (!tab) return false;
        void tab.unstageAll();
        return true;
      }
    });
    return () => {
      off();
      stopSync();
      window.removeEventListener('focus', onFocus);
      clearInterval(hibernation);
      offHandlers();
      app.dispose();
    };
  });
</script>

<div class="window">
  <TitleBar
    entries={app.entries}
    activeId={app.activeId}
    onactivate={(id) => app.activate(id)}
    onclose={(id) => void app.close(id)}
    onnew={() => app.newTab()}
    onreorder={(ids) => app.reorder(ids)}
    onpalette={() => (app.paletteOpen = true)}
    onsettings={() => settingsUi.open()}
  />

  <div id="tabpanel-main" class="main" role="tabpanel" aria-labelledby={app.activeId ? `tab-${app.activeId}` : undefined} tabindex="-1">
   <div class="stage">
    {#if app.status === 'loading'}
      <div class="boot" aria-busy="true" aria-label={t('app.loading')}>
        <div class="boot-col">
          <span class="skeleton" style:width="48px" style:height="48px" style:border-radius="14px"></span>
          <span class="skeleton" style:width="260px" style:height="26px"></span>
          <div class="boot-cards">
            <span class="skeleton" style:height="112px" style:border-radius="14px"></span>
            <span class="skeleton" style:height="112px" style:border-radius="14px"></span>
            <span class="skeleton" style:height="112px" style:border-radius="14px"></span>
          </div>
          <span class="skeleton" style:width="100%" style:height="72px" style:border-radius="12px"></span>
        </div>
      </div>
    {:else if app.status === 'error'}
      <EmptyState
        icon="circle-alert"
        tone="danger"
        title={t('app.errorTitle')}
        body={app.loadError?.message ?? t('app.errorBody')}
      >
        <button type="button" class="btn btn-primary" onclick={() => app.reload()}>
          <Icon name="refresh-cw" size={14} />{t('common.retry')}
        </button>
        {#if app.loadError?.detail}
          <details class="err-detail"><summary>{t('toast.showDetail')}</summary><pre class="mono">{app.loadError.detail}</pre></details>
        {/if}
      </EmptyState>
    {:else if entry?.kind === 'new'}
      {#key entry.id}
        <Welcome {app} entryId={entry.id} />
      {/key}
    {:else if entry?.kind === 'repo'}
      {#key entry.id}
        {@const tab = app.tabState(entry.id)}
        {#if tab}
          <Workspace {tab} onclose={() => app.close(entry.id)} onlocate={() => app.locate(entry.id)} />
        {/if}
      {/key}
    {/if}
   </div>
  </div>
</div>

{#if app.cloneOpen}
  <CloneDialog {app} onclose={() => (app.cloneOpen = false)} />
{/if}

{#if settingsUi.isOpen}
  <SettingsDialog
    api={app.api}
    tabId={entry?.kind === 'repo' ? entry.id : undefined}
    onclose={() => {
      settingsUi.close();
      if (app.activeTab) void app.activeTab.ai.loadState();
    }}
  />
{/if}

{#if app.paletteOpen}
  <CommandPalette actions={buildActions(app)} onclose={() => (app.paletteOpen = false)} />
{/if}

<Toasts />

<style>
  .window {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    background: var(--bg-app);
  }
  .main {
    flex: 1;
    min-height: 0;
    padding: 0 8px 8px;
    background: var(--bg-app);
  }
  /* Tarjeta principal: panel en la capa «superficie» sobre el fondo de la ventana. */
  .stage {
    height: 100%;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg-surface);
    box-shadow: var(--elev-1);
  }
  .boot {
    display: grid;
    place-items: center;
    height: 100%;
  }
  .boot-col {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    width: min(720px, 90%);
  }
  .boot-cards {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--space-3);
  }
  .err-detail {
    max-width: 60ch;
    text-align: left;
    font-size: var(--text-xs);
  }
  pre {
    white-space: pre-wrap;
    -webkit-user-select: text;
    user-select: text;
  }
</style>
