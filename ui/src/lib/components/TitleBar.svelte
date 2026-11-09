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
    /** Para pruebas y revisión visual; por defecto se detecta con `navigator`. */
    platform?: Platform;
    /** `undefined` = detectar Tauri; `null` = sin API de ventana (navegador). */
    windowApi?: WindowApi | null;
  }
  let { entries, activeId, onactivate, onclose, onnew, onreorder, onpalette, onsettings, platform, windowApi }: Props = $props();

  const os = $derived(platform ?? detectPlatform());
  const mac = $derived(os === 'mac');
  const paletteCombo = $derived(formatCombo(SHORTCUTS.find((s) => s.id === 'palette.open')?.combo ?? 'Mod+K', mac));
  const settingsCombo = $derived(formatCombo(SHORTCUTS.find((s) => s.id === 'settings.open')?.combo ?? 'Mod+,', mac));
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

  <div class="grow" data-tauri-drag-region></div>

  <div class="tools">
    <button type="button" class="palette" onclick={onpalette} aria-label={t('palette.open')} title={t('palette.open')}>
      <Icon name="search" size={14} />
      <span class="hint">{t('palette.placeholderShort')}</span>
      <kbd>{paletteCombo}</kbd>
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
  @media (max-width: 900px) {
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
