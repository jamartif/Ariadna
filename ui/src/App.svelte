<script lang="ts">
  import { onMount } from 'svelte';
  import CommandPalette from '$lib/components/CommandPalette.svelte';
  import EmptyState from '$lib/components/EmptyState.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import TabStrip from '$lib/components/TabStrip.svelte';
  import TitleBar from '$lib/components/TitleBar.svelte';
  import Toasts from '$lib/components/Toasts.svelte';
  import Welcome from '$lib/components/Welcome.svelte';
  import Workspace from '$lib/components/Workspace.svelte';
  import { t } from '$lib/i18n';
  import { installShortcuts, registerHandlers } from '$lib/shortcuts/registry';
  import type { AppStore } from '$lib/stores/app.svelte';
  import { buildActions } from '$lib/stores/commands';
  import { settings } from '$lib/stores/settings.svelte';

  let { app }: { app: AppStore } = $props();

  const entry = $derived(app.activeEntry);
  const title = $derived(entry?.kind === 'repo' ? `${entry.info.name}${entry.info.branch ? ` — ${entry.info.branch}` : ''}` : '');

  onMount(() => {
    settings.apply();
    void app.init();
    const off = installShortcuts();
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
      'changes.refresh': () => {
        const tab = entry?.kind === 'repo' ? app.activeTab : null;
        if (!tab) return false;
        void tab.refresh();
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
      offHandlers();
      app.dispose();
    };
  });
</script>

<div class="window">
  <TitleBar {title} onpalette={() => (app.paletteOpen = true)} />
  <TabStrip
    entries={app.entries}
    activeId={app.activeId}
    onactivate={(id) => app.activate(id)}
    onclose={(id) => void app.close(id)}
    onnew={() => app.newTab()}
    onreorder={(ids) => app.reorder(ids)}
  />

  <div id="tabpanel-main" class="main" role="tabpanel" aria-labelledby={app.activeId ? `tab-${app.activeId}` : undefined} tabindex="-1">
    {#if app.status === 'loading'}
      <div class="boot" aria-busy="true" aria-label={t('app.loading')}>
        <div class="boot-col">
          <span class="skeleton" style:width="56px" style:height="56px"></span>
          <span class="skeleton" style:width="260px" style:height="24px"></span>
          <span class="skeleton" style:width="100%" style:height="64px"></span>
          <span class="skeleton" style:width="100%" style:height="160px"></span>
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
  }
  .main {
    flex: 1;
    min-height: 0;
    background: var(--bg-surface);
    border-top: 0;
    overflow: hidden;
  }
  .boot {
    display: grid;
    place-items: center;
    height: 100%;
    background: var(--bg-app);
  }
  .boot-col {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    width: min(560px, 90%);
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
