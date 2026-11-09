<script lang="ts">
  interface Props {
    /** Ancho actual (px) de la columna que controla. */
    value: number;
    min: number;
    max: number;
    /** `1` si arrastrar a la derecha agranda la columna (barra izquierda); `-1` si la encoge. */
    direction?: 1 | -1;
    label: string;
    onchange: (v: number) => void;
  }
  let { value, min, max, direction = 1, label, onchange }: Props = $props();

  let dragging = $state(false);
  let startX = 0;
  let startValue = 0;

  const clamp = (v: number) => Math.max(min, Math.min(max, Math.round(v)));

  function down(e: PointerEvent) {
    dragging = true;
    startX = e.clientX;
    startValue = value;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function move(e: PointerEvent) {
    if (!dragging) return;
    onchange(clamp(startValue + (e.clientX - startX) * direction));
  }
  function up() {
    dragging = false;
  }
  function key(e: KeyboardEvent) {
    const step = e.shiftKey ? 48 : 16;
    if (e.key === 'ArrowLeft') onchange(clamp(value - step * direction));
    else if (e.key === 'ArrowRight') onchange(clamp(value + step * direction));
    else if (e.key === 'Home') onchange(min);
    else if (e.key === 'End') onchange(max);
    else return;
    e.preventDefault();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions, a11y_no_noninteractive_tabindex -->
<div
  class="splitter"
  class:dragging
  role="separator"
  aria-orientation="vertical"
  aria-label={label}
  aria-valuenow={value}
  aria-valuemin={min}
  aria-valuemax={max}
  tabindex="0"
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  onkeydown={key}
>
  <span class="line"></span>
</div>

<style>
  .splitter {
    position: relative;
    width: 8px;
    margin: 0 -4px;
    z-index: 3;
    cursor: col-resize;
    touch-action: none;
    display: flex;
    justify-content: center;
  }
  .line {
    width: 1px;
    height: 100%;
    background: var(--border);
    transition: background-color var(--dur) var(--ease);
  }
  .splitter:hover .line,
  .splitter.dragging .line,
  .splitter:focus-visible .line {
    width: 3px;
    background: var(--accent);
  }
  .splitter:focus-visible {
    outline-offset: -2px;
  }
</style>
