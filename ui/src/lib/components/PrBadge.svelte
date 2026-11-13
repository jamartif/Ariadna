<script lang="ts">
  import type { Tone } from '$lib/util/pr';
  import Icon from './Icon.svelte';
  import type { IconName } from './icons';

  /** Etiqueta con icono y texto: el estado nunca depende solo del color. */
  let { icon, tone = 'muted', label, title }: { icon?: IconName; tone?: Tone; label: string; title?: string } = $props();
</script>

<span class="badge" data-tone={tone} {title}>
  {#if icon}<Icon name={icon} size={11} />{/if}{label}
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex: none;
    height: 18px;
    padding: 0 7px;
    border-radius: var(--pill);
    border: 1px solid transparent;
    background: var(--bg-muted);
    color: var(--text-muted);
    font-size: var(--text-2xs);
    font-weight: 650;
    white-space: nowrap;
  }
  .badge[data-tone='success'] {
    background: var(--success-soft);
    color: var(--success);
  }
  .badge[data-tone='danger'] {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .badge[data-tone='warning'] {
    background: var(--warning-soft);
    color: var(--warning);
  }
  .badge[data-tone='info'] {
    background: var(--info-soft);
    color: var(--info);
  }
  .badge[data-tone='accent'] {
    background: var(--accent-soft);
    color: var(--accent);
  }
  @media (forced-colors: active) {
    .badge {
      border-color: CanvasText;
    }
  }
</style>
