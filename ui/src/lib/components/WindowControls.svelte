<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n';
  import { getWindowApi, type WindowApi } from '$lib/util/window';
  import Icon from './Icon.svelte';

  /** `undefined` = detectar (Tauri); `null` = sin API (navegador/mock: botones inertes). */
  let { api }: { api?: WindowApi | null } = $props();

  let win = $state<WindowApi | null>(null);
  let maximized = $state(false);

  async function sync() {
    try {
      maximized = (await win?.isMaximized()) ?? false;
    } catch {
      /* la ventana puede estar cerrándose */
    }
  }

  onMount(() => {
    let off: (() => void) | undefined;
    let alive = true;
    void (async () => {
      win = api !== undefined ? api : await getWindowApi();
      if (!alive || !win) return;
      await sync();
      try {
        off = await win.onResized(() => void sync());
      } catch {
        /* sin eventos de tamaño: el icono se actualiza al pulsar */
      }
    })();
    return () => {
      alive = false;
      off?.();
    };
  });

  async function run(fn: ((w: WindowApi) => Promise<void>) | undefined) {
    if (!win || !fn) return;
    try {
      await fn(win);
    } catch {
      /* ignorar: la API de ventana puede no estar permitida */
    }
    await sync();
  }
</script>

<div class="controls" role="group" aria-label={t('window.controls')}>
  <button type="button" class="wc" aria-label={t('window.minimize')} title={t('window.minimize')} onclick={() => run((w) => w.minimize())}>
    <Icon name="minus" size={14} />
  </button>
  <button
    type="button"
    class="wc"
    aria-label={maximized ? t('window.restore') : t('window.maximize')}
    title={maximized ? t('window.restore') : t('window.maximize')}
    onclick={() => run((w) => w.toggleMaximize())}
  >
    <Icon name={maximized ? 'win-restore' : 'win-max'} size={13} />
  </button>
  <button type="button" class="wc close" aria-label={t('window.close')} title={t('window.close')} onclick={() => run((w) => w.close())}>
    <Icon name="x" size={15} />
  </button>
</div>

<style>
  .controls {
    display: flex;
    align-self: stretch;
    margin-left: var(--space-2);
  }
  .wc {
    display: grid;
    place-items: center;
    width: 46px;
    border: none;
    border-radius: 0;
    background: transparent;
    color: var(--text-muted);
    transition:
      background-color var(--dur-fast) var(--ease),
      color var(--dur-fast) var(--ease);
  }
  .wc:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .wc:active {
    background: var(--bg-pressed);
  }
  .wc.close:hover {
    background: #e5334a;
    color: #fff;
  }
  .wc:focus-visible {
    outline-offset: -3px;
    border-radius: 0;
  }
</style>
