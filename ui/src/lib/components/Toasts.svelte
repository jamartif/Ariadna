<script lang="ts">
  import { t } from '$lib/i18n';
  import { toasts, type Toast } from '$lib/stores/toasts.svelte';
  import Icon from './Icon.svelte';
  import type { IconName } from './icons';

  let open = $state<Record<number, boolean>>({});

  const icons: Record<Toast['kind'], IconName> = {
    error: 'circle-alert',
    success: 'circle-check',
    info: 'info',
    warning: 'triangle-alert'
  };
</script>

<!-- Región viva oculta para operaciones (aria-live, spec §7 accesibilidad). -->
<div class="sr-only" role="status" aria-live="polite" aria-atomic="true">{toasts.announcement}</div>

<section class="toasts" aria-label={t('toast.region')}>
  {#each toasts.items as toast (toast.id)}
    <div class="toast" data-kind={toast.kind} role={toast.kind === 'error' ? 'alert' : 'status'}>
      <span class="ico"><Icon name={icons[toast.kind]} size={18} /></span>
      <div class="body">
        <strong>{toast.title}</strong>
        {#if toast.message}<p>{toast.message}</p>{/if}
        {#if toast.detail}
          <button
            type="button"
            class="more btn btn-ghost btn-sm"
            aria-expanded={!!open[toast.id]}
            aria-controls="toast-detail-{toast.id}"
            onclick={() => (open = { ...open, [toast.id]: !open[toast.id] })}
          >
            <Icon name={open[toast.id] ? 'chevron-down' : 'chevron-right'} size={12} />
            {open[toast.id] ? t('toast.hideDetail') : t('toast.showDetail')}
          </button>
          {#if open[toast.id]}
            <pre id="toast-detail-{toast.id}" class="mono">{toast.detail}</pre>
          {/if}
        {/if}
      </div>
      <button
        type="button"
        class="btn btn-ghost btn-icon btn-sm"
        aria-label={t('toast.dismiss')}
        onclick={() => toasts.dismiss(toast.id)}
      >
        <Icon name="x" size={14} />
      </button>
    </div>
  {/each}
</section>

<style>
  .toasts {
    position: fixed;
    right: var(--space-4);
    bottom: var(--space-4);
    z-index: 100;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    width: min(380px, calc(100vw - 2 * var(--space-4)));
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    background: var(--bg-surface);
    box-shadow: var(--elev-2);
    animation: pop var(--dur) var(--ease);
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
  .toast[data-kind='error'] .ico {
    color: var(--danger);
  }
  .toast[data-kind='success'] .ico {
    color: var(--success);
  }
  .toast[data-kind='warning'] .ico {
    color: var(--warning);
  }
  .toast[data-kind='info'] .ico {
    color: var(--info);
  }
  .ico {
    display: inline-flex;
    margin-top: 1px;
  }
  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: var(--text-md);
  }
  .body p {
    color: var(--text-muted);
    overflow-wrap: anywhere;
  }
  .more {
    align-self: flex-start;
    margin-left: -6px;
  }
  pre {
    margin: 0;
    padding: var(--space-2);
    max-height: 180px;
    overflow: auto;
    border-radius: var(--radius-sm);
    background: var(--bg-subtle);
    color: var(--text);
    font-size: var(--text-xs);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    -webkit-user-select: text;
    user-select: text;
  }
</style>
