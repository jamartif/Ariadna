<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t } from '$lib/i18n';
  import Icon from './Icon.svelte';

  let {
    kind = 'error',
    title,
    message,
    detail,
    actions,
    children
  }: {
    kind?: 'error' | 'warning' | 'info';
    title: string;
    message?: string;
    detail?: string | null;
    actions?: Snippet;
    children?: Snippet;
  } = $props();
</script>

<div class="banner" data-kind={kind} role={kind === 'error' ? 'alert' : 'status'}>
  <span class="icon"><Icon name={kind === 'info' ? 'info' : kind === 'warning' ? 'triangle-alert' : 'circle-alert'} size={18} /></span>
  <div class="body">
    <strong>{title}</strong>
    {#if message}<p>{message}</p>{/if}
    {#if children}{@render children()}{/if}
    {#if detail}
      <details>
        <summary>{t('toast.showDetail')}</summary>
        <pre class="mono">{detail}</pre>
      </details>
    {/if}
  </div>
  {#if actions}<div class="actions">{@render actions()}</div>{/if}
</div>

<style>
  .banner {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--border-strong);
    border-left-width: 4px;
    border-radius: var(--radius-md);
    background: var(--bg-surface);
    font-size: var(--text-md);
  }
  .banner[data-kind='error'] {
    background: var(--danger-soft);
    border-color: var(--danger);
  }
  .banner[data-kind='warning'] {
    background: var(--warning-soft);
    border-color: var(--warning);
  }
  .banner[data-kind='info'] {
    background: var(--info-soft);
    border-color: var(--info);
  }
  .icon {
    display: inline-flex;
    margin-top: 1px;
  }
  [data-kind='error'] .icon {
    color: var(--danger);
  }
  [data-kind='warning'] .icon {
    color: var(--warning);
  }
  [data-kind='info'] .icon {
    color: var(--info);
  }
  .body {
    /* Base mínima: en columnas estrechas las acciones bajan en vez de aplastar el mensaje. */
    flex: 1 1 14rem;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .body p {
    color: var(--text);
    overflow-wrap: anywhere;
  }
  details {
    margin-top: var(--space-1);
    font-size: var(--text-sm);
  }
  summary {
    cursor: pointer;
    color: var(--text-muted);
    min-height: var(--target-min);
    display: flex;
    align-items: center;
  }
  pre {
    margin: var(--space-1) 0 0;
    padding: var(--space-2);
    max-height: 160px;
    overflow: auto;
    background: var(--bg-surface);
    border-radius: var(--radius-sm);
    font-size: var(--text-xs);
    white-space: pre-wrap;
    -webkit-user-select: text;
    user-select: text;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-self: center;
    min-width: 0;
  }
  @media (forced-colors: active) {
    .banner {
      border-color: CanvasText;
    }
  }
</style>
