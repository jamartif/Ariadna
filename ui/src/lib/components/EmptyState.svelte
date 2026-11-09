<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';
  import type { IconName } from './icons';

  let {
    icon = 'folder',
    title,
    body,
    tone = 'neutral',
    compact = false,
    children
  }: {
    icon?: IconName;
    title: string;
    body?: string;
    tone?: 'neutral' | 'danger' | 'accent';
    compact?: boolean;
    children?: Snippet;
  } = $props();
</script>

<div class="empty" class:compact data-tone={tone}>
  <div class="art" aria-hidden="true">
    <span class="ring"></span>
    <Icon name={icon} size={compact ? 20 : 28} />
  </div>
  <h2>{title}</h2>
  {#if body}<p>{body}</p>{/if}
  {#if children}<div class="actions">{@render children()}</div>{/if}
</div>

<style>
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    padding: var(--space-6) var(--space-5);
    text-align: center;
    height: 100%;
    min-height: 0;
    color: var(--text-muted);
  }
  .empty.compact {
    padding: var(--space-5) var(--space-4);
  }
  .art {
    position: relative;
    display: grid;
    place-items: center;
    width: 64px;
    height: 64px;
    margin-bottom: var(--space-2);
    color: var(--accent);
  }
  .compact .art {
    width: 48px;
    height: 48px;
  }
  .ring {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    background: var(--accent-soft);
    box-shadow: 0 0 0 6px color-mix(in srgb, var(--accent-soft) 45%, transparent);
  }
  .art :global(svg) {
    position: relative;
  }
  [data-tone='danger'] .art {
    color: var(--danger);
  }
  [data-tone='danger'] .ring {
    background: var(--danger-soft);
    box-shadow: 0 0 0 6px color-mix(in srgb, var(--danger-soft) 45%, transparent);
  }
  [data-tone='neutral'] .art {
    color: var(--text-muted);
  }
  [data-tone='neutral'] .ring {
    background: var(--bg-muted);
    box-shadow: 0 0 0 6px color-mix(in srgb, var(--bg-muted) 45%, transparent);
  }
  h2 {
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--text);
  }
  p {
    max-width: 44ch;
    font-size: var(--text-md);
    line-height: 1.5;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    justify-content: center;
    margin-top: var(--space-3);
  }
</style>
