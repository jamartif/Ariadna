<script lang="ts">
  import { onMount } from 'svelte';
  import type { Entry } from '$lib/stores/app.svelte';
  import { t } from '$lib/i18n';
  import { formatCombo, SHORTCUTS } from '$lib/shortcuts/registry';
  import { settings } from '$lib/stores/settings.svelte';
  import { detectPlatform, type Platform } from '$lib/util/platform';
  import { getWindowApi, type WindowApi } from '$lib/util/window';
  import Icon from './Icon.svelte';
  import Logo from './Logo.svelte';
  import TabStrip from './TabStrip.svelte';
  import WindowControls from './WindowControls.svelte';
  import type { IconName } from './icons';

  interface Props {
    entries: Entry[];
    activeId: string | null;
    onactivate: (id: string) => void;
    onclose: (id: string) => void;
    onnew: () => void;
    onreorder: (ids: string[]) => void;
    onpalette: () => void;
    onsettings?: () => void;
    /** Rama actual del repositorio activo (acción de cambio rápido en la barra). */
    branch?: string | null;
    detached?: boolean;
    ahead?: number;
    behind?: number;
    /** Lleva el foco al buscador del riel de ramas (cambio rápido de rama). */
    onbranches?: () => void;
    /** Para pruebas y revisión visual; por defecto se detecta con `navigator`. */
    platform?: Platform;
    /** `undefined` = detectar Tauri; `null` = sin API de ventana (navegador). */
    windowApi?: WindowApi | null;
  }
  let {
    entries,
    activeId,
    onactivate,
    onclose,
    onnew,
    onreorder,
    onpalette,
    onsettings,
    branch = null,
    detached = false,
    ahead = 0,
    behind = 0,
    onbranches,
    platform,
    windowApi
  }: Props = $props();

  const os = $derived(platform ?? detectPlatform());
  const mac = $derived(os === 'mac');
  const paletteCombo = $derived(formatCombo(SHORTCUTS.find((s) => s.id === 'palette.open')?.combo ?? 'Mod+K', mac));
  const settingsCombo = $derived(formatCombo(SHORTCUTS.find((s) => s.id === 'settings.open')?.combo ?? 'Mod+,', mac));
  const branchCombo = $derived(formatCombo(SHORTCUTS.find((s) => s.id === 'rail.focus')?.combo ?? 'Mod+Shift+B', mac));
  const themeIcon = $derived<IconName>(settings.theme === 'light' ? 'sun' : settings.theme === 'dark' ? 'moon' : 'monitor');
  const themeLabel = $derived(t(`settings.theme.${settings.theme}`));

  // Doble clic en la zona vacía: maximizar. Se resuelve aquí (y se corta la propagación)
  // para que el manejador interno de Tauri no lo alterne una segunda vez.
  let win = $state<WindowApi | null>(null);
  onMount(() => {
    void (async () => {
      win = windowApi !== undefined ? windowApi : await getWindowApi();
    })();
  });
  function onDragMouseDown(e: MouseEvent) {
    if (e.detail !== 2 || e.button !== 0) return;
    const el = e.target as HTMLElement | null;
    if (!el?.hasAttribute('data-tauri-drag-region')) return;
    e.stopPropagation();
    void win?.toggleMaximize().catch(() => undefined);
  }
</script>

<!-- Barra de título ÚNICA e integrada: marca, pestañas de repo, paleta y controles de ventana.
     Solo las zonas vacías llevan `data-tauri-drag-region`: botones y pestañas no se arrastran. -->
<!-- svelte-ignore a11y_no_static_element_interactions, a11y_no_noninteractive_element_interactions -->
<header class="titlebar" class:mac data-platform={os} data-tauri-drag-region onmousedown={onDragMouseDown}>
  {#if mac}<div class="mac-inset" data-tauri-drag-region aria-hidden="true"></div>{/if}
  <div class="brand" data-tauri-drag-region>
    <Logo size={22} />
  </div>

  <TabStrip {entries} {activeId} {onactivate} {onclose} {onnew} {onreorder} />

  {#if branch}
    <span class="sep" data-tauri-drag-region aria-hidden="true"></span>
    <button
      type="button"
      class="branch"
      class:detached
      onclick={() => onbranches?.()}
      title={`${t('titlebar.branch')} (${branchCombo})`}
      aria-keyshortcuts="Control+Shift+b Meta+Shift+b"
    >
      <Icon name="git-branch" size={14} />
      <span class="blabel">{t('titlebar.branchLabel')}</span>
      <span class="bname mono">{branch}</span>
      {#if detached}<span class="mini">HEAD</span>{/if}
      {#if ahead > 0}<span class="sync" title={t('sidebar.ahead', { n: ahead })}><Icon name="arrow-up" size={12} />{ahead}</span>{/if}
      {#if behind > 0}<span class="sync" title={t('sidebar.behind', { n: behind })}><Icon name="arrow-down" size={12} />{behind}</span>{/if}
      <span class="caret"><Icon name="chevron-down" size={12} /></span>
    </button>
  {/if}

  <div class="grow" data-tauri-drag-region></div>

  <div class="tools">
    <button type="button" class="palette" onclick={onpalette} title={t('palette.open')}>
      <Icon name="search" size={14} />
      <span class="hint">{t('palette.placeholderShort')}</span>
      <kbd>{paletteCombo}</kbd>
      <span class="sr-only">{t('palette.open')}</span>
    </button>
    <button
      type="button"
      class="btn btn-ghost btn-icon tool"
      onclick={() => settings.cycleTheme()}
      aria-label={t('settings.themeButton', { mode: themeLabel })}
      title={t('settings.themeButton', { mode: themeLabel })}
    >
      <Icon name={themeIcon} size={16} />
    </button>
    <button
      type="button"
      class="btn btn-ghost btn-icon tool"
      onclick={() => settings.toggleDensity()}
      aria-pressed={settings.density === 'compact'}
      aria-label={t('settings.density')}
      title={t(settings.density === 'compact' ? 'settings.density.compact' : 'settings.density.comfortable')}
    >
      <Icon name="rows" size={16} />
    </button>
    {#if onsettings}
      <button
        type="button"
        class="btn btn-ghost btn-icon tool"
        onclick={onsettings}
        aria-label={t('settings.open')}
        title={t('settings.openHint', { combo: settingsCombo })}
        aria-keyshortcuts="Control+, Meta+,"
      >
        <Icon name="sliders" size={16} />
      </button>
    {/if}
  </div>

  {#if !mac}<WindowControls api={windowApi} />{/if}
</header>

<style>
  .titlebar {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--titlebar-h);
    padding-left: var(--space-3);
    padding-right: var(--space-2);
    background: var(--bg-app);
    flex: none;
  }
  .titlebar:not(.mac) {
    padding-right: 0;
  }
  .mac-inset {
    /* Hueco para los semáforos nativos (titleBarStyle: Overlay). */
    width: 68px;
    align-self: stretch;
    flex: none;
  }
  .brand {
    display: flex;
    align-items: center;
    align-self: stretch;
    flex: none;
    padding-right: var(--space-1);
  }
  .grow {
    flex: 1;
    align-self: stretch;
    min-width: 24px;
  }
  .sep {
    width: 1px;
    height: 20px;
    flex: none;
    margin: 0 2px;
    background: var(--border);
  }
  /* Rama actual del repo activo: un clic lleva el foco al buscador del riel. */
  .branch {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    max-width: 320px;
    min-width: 0;
    flex: none;
    padding: 0 var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--pill);
    background: color-mix(in srgb, var(--bg-surface) 70%, transparent);
    color: var(--text);
    font-size: var(--text-sm);
    font-weight: 600;
    transition:
      border-color var(--dur-fast) var(--ease),
      background-color var(--dur-fast) var(--ease);
  }
  .branch:hover {
    border-color: var(--border-hover);
    background: var(--bg-surface);
  }
  .branch :global(svg:first-child) {
    flex: none;
    color: var(--accent);
  }
  .blabel {
    flex: none;
    color: var(--text-subtle);
    font-size: var(--text-2xs);
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .bname {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-xs);
    font-weight: 650;
  }
  .branch .sync {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    flex: none;
    color: var(--text-muted);
    font-size: var(--text-2xs);
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }
  .mini {
    flex: none;
    padding: 0 5px;
    height: 16px;
    line-height: 16px;
    border-radius: var(--pill);
    background: var(--bg-muted);
    color: var(--text-muted);
    font-size: var(--text-2xs);
    font-weight: 650;
  }
  .caret {
    flex: none;
    color: var(--text-subtle);
  }
  @media (max-width: 1180px) {
    .blabel,
    .branch .sync,
    .mini {
      display: none;
    }
  }

  @media (forced-colors: active) {
    .branch {
      border-color: ButtonText;
    }
    .branch :global(svg:first-child) {
      color: CanvasText;
    }
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 2px;
    flex: none;
  }
  .tool {
    width: 28px;
    min-width: 28px;
    min-height: 28px;
  }
  .palette {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    height: 28px;
    min-width: 200px;
    margin-right: var(--space-1);
    padding: 0 var(--space-2) 0 var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--pill);
    background: color-mix(in srgb, var(--bg-surface) 70%, transparent);
    color: var(--text-subtle);
    font-size: var(--text-sm);
    transition:
      border-color var(--dur-fast) var(--ease),
      background-color var(--dur-fast) var(--ease),
      color var(--dur-fast) var(--ease);
  }
  .palette:hover {
    border-color: var(--border-hover);
    background: var(--bg-surface);
    color: var(--text-muted);
  }
  .palette .hint {
    flex: 1;
    text-align: left;
  }
  @media (max-width: 1120px) {
    .palette {
      min-width: 0;
      width: 28px;
      padding: 0;
      justify-content: center;
    }
    .palette .hint,
    .palette kbd {
      display: none;
    }
  }
</style>
