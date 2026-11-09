<script lang="ts">
  import { t } from '$lib/i18n';
  import { formatCombo, isMac, SHORTCUTS } from '$lib/shortcuts/registry';
  import { settings } from '$lib/stores/settings.svelte';
  import Icon from './Icon.svelte';
  import Logo from './Logo.svelte';
  import type { IconName } from './icons';

  let { title = '', onpalette }: { title?: string; onpalette: () => void } = $props();

  const mac = isMac();
  const paletteCombo = formatCombo(SHORTCUTS.find((s) => s.id === 'palette.open')?.combo ?? 'Mod+K', mac);
  const themeIcon = $derived<IconName>(settings.theme === 'light' ? 'sun' : settings.theme === 'dark' ? 'moon' : 'monitor');
  const themeLabel = $derived(t(`settings.theme.${settings.theme}`));
</script>

<!-- Barra de título integrada: la región arrastrable es el propio contenedor; los botones no lo son. -->
<header class="titlebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <Logo size={22} />
    <span class="app" data-tauri-drag-region>Ariadna</span>
  </div>
  <div class="title" data-tauri-drag-region>{title}</div>
  <div class="tools">
    <button type="button" class="btn palette" onclick={onpalette} aria-label={t('palette.open')} title={t('palette.open')}>
      <Icon name="search" size={14} />
      <span class="hint">{t('palette.placeholderShort')}</span>
      <kbd>{paletteCombo}</kbd>
    </button>
    <button
      type="button"
      class="btn btn-ghost btn-icon"
      onclick={() => settings.cycleTheme()}
      aria-label={t('settings.themeButton', { mode: themeLabel })}
      title={t('settings.themeButton', { mode: themeLabel })}
    >
      <Icon name={themeIcon} size={16} />
    </button>
    <button
      type="button"
      class="btn btn-ghost btn-icon"
      onclick={() => settings.toggleDensity()}
      aria-pressed={settings.density === 'compact'}
      aria-label={t('settings.density')}
      title={t(settings.density === 'compact' ? 'settings.density.compact' : 'settings.density.comfortable')}
    >
      <Icon name="rows" size={16} />
    </button>
  </div>
</header>

<style>
  .titlebar {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: var(--titlebar-h);
    padding: 0 var(--space-3);
    /* Reserva el hueco de los controles de ventana nativos si el SO los superpone. */
    padding-left: max(var(--space-3), var(--window-controls-inset-left, 0px));
    padding-right: max(var(--space-3), var(--window-controls-inset-right, 0px));
    background: var(--bg-app);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-weight: 650;
    letter-spacing: -0.01em;
  }
  .app {
    font-size: var(--text-md);
  }
  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: center;
    color: var(--text-subtle);
    font-size: var(--text-sm);
  }
  .tools {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }
  .palette {
    gap: var(--space-2);
    color: var(--text-muted);
    background: var(--bg-surface);
    border-color: var(--border);
    min-width: 200px;
    justify-content: flex-start;
  }
  .palette .hint {
    flex: 1;
    text-align: left;
    font-weight: 400;
  }
  @media (max-width: 760px) {
    .palette {
      min-width: 0;
    }
    .palette .hint,
    .title {
      display: none;
    }
  }
</style>
