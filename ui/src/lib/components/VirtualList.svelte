<script lang="ts" generics="T">
  import type { Snippet } from 'svelte';
  import { buildOffsets, visibleRange } from '$lib/util/virtual';

  interface Props {
    items: readonly T[];
    /** Altura en px de cada elemento (fija por elemento, conocida de antemano). */
    heightOf: (item: T, index: number) => number;
    row: Snippet<[T, number]>;
    overscan?: number;
    /** Alto de la ventana cuando el contenedor no se ha medido todavía (jsdom, primer pintado). */
    fallbackHeight?: number;
    role?: string;
    label?: string;
    rowCount?: number;
    multiselectable?: boolean;
    activeDescendant?: string;
    /** Contenido más ancho que la ventana (diff): desplazamiento horizontal. */
    horizontal?: boolean;
    tabindex?: number;
    class?: string;
    onscroll?: (top: number) => void;
    onkeydown?: (e: KeyboardEvent) => void;
  }

  let {
    items,
    heightOf,
    row,
    overscan = 8,
    fallbackHeight = 480,
    role,
    label,
    rowCount,
    multiselectable,
    activeDescendant,
    horizontal = false,
    tabindex,
    class: cls = '',
    onscroll,
    onkeydown
  }: Props = $props();

  let el = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let clientHeight = $state(0);

  const offsets = $derived(buildOffsets(items.length, (i) => heightOf(items[i] as T, i)));
  const total = $derived(offsets[items.length] ?? 0);
  const viewport = $derived(clientHeight > 0 ? clientHeight : fallbackHeight);
  const range = $derived(visibleRange(offsets, scrollTop, viewport, overscan));
  const slice = $derived(items.slice(range.start, range.end));
  const topPad = $derived(offsets[range.start] ?? 0);
  const bottomPad = $derived(Math.max(0, total - (offsets[range.end] ?? total)));

  function handleScroll() {
    if (!el) return;
    scrollTop = el.scrollTop;
    onscroll?.(el.scrollTop);
  }

  /** Desplaza lo mínimo para que el elemento `index` quede visible. */
  export function scrollToIndex(index: number): void {
    if (!el) return;
    const top = offsets[index] ?? 0;
    const bottom = offsets[index + 1] ?? top;
    const h = el.clientHeight || fallbackHeight;
    if (top < el.scrollTop) el.scrollTop = top;
    else if (bottom > el.scrollTop + h) el.scrollTop = bottom - h;
    scrollTop = el.scrollTop;
  }

  export function scrollToTop(): void {
    if (el) el.scrollTop = 0;
    scrollTop = 0;
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  bind:this={el}
  bind:clientHeight
  class="vl {cls}"
  {role}
  aria-label={label}
  aria-rowcount={role === 'grid' || role === 'table' || role === 'treegrid' ? rowCount : undefined}
  aria-multiselectable={multiselectable}
  aria-activedescendant={activeDescendant}
  tabindex={tabindex ?? -1}
  onscroll={handleScroll}
  {onkeydown}
>
  <div class="inner" class:horizontal>
    <div class="pad" style:height="{topPad}px" aria-hidden="true"></div>
    {#each slice as item, i (range.start + i)}
      {@render row(item, range.start + i)}
    {/each}
    <div class="pad" style:height="{bottomPad}px" aria-hidden="true"></div>
  </div>
</div>

<style>
  .vl {
    height: 100%;
    overflow: auto;
    position: relative;
    outline-offset: -2px;
  }
  .inner.horizontal {
    width: max-content;
    min-width: 100%;
  }
  .pad {
    flex: none;
  }
</style>
