<script lang="ts">
  import { ICONS, type IconName, type Shape } from './icons';

  let {
    name,
    size = 16,
    label,
    filled = false,
    spin = false
  }: { name: IconName; size?: number; label?: string; filled?: boolean; spin?: boolean } = $props();

  const shapes = $derived<readonly Shape[]>(ICONS[name]);
</script>

<svg
  class="icon"
  class:spin
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill={filled ? 'currentColor' : 'none'}
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  role={label ? 'img' : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : 'true'}
  focusable="false"
>
  {#each shapes as s, i (i)}
    {#if typeof s === 'string'}
      <path d={s} />
    {:else if 'c' in s}
      <circle cx={s.c[0]} cy={s.c[1]} r={s.c[2]} />
    {:else if 'r' in s}
      <rect x={s.r[0]} y={s.r[1]} width={s.r[2]} height={s.r[3]} rx={s.r[4]} />
    {:else}
      <line x1={s.l[0]} y1={s.l[1]} x2={s.l[2]} y2={s.l[3]} />
    {/if}
  {/each}
</svg>

<style>
  .icon {
    flex: none;
  }
</style>
