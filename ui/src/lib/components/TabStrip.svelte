<script lang="ts">
  import { onMount } from 'svelte';
  import { flip } from 'svelte/animate';
  import type { Entry } from '$lib/stores/app.svelte';
  import { SOFT_TAB_LIMIT } from '$lib/stores/app.svelte';
  import { t, tn } from '$lib/i18n';
  import { registerHandlers } from '$lib/shortcuts/registry';
  import { repoHue } from '$lib/util/hue';
  import { tabIn, tabOut } from '$lib/util/motion';
  import { moveItem } from '$lib/util/virtual';
  import Icon from './Icon.svelte';

  interface Props {
    entries: Entry[];
    activeId: string | null;
    onactivate: (id: string) => void;
    onclose: (id: string) => void;
    onnew: () => void;
    onreorder: (ids: string[]) => void;
  }
  let { entries, activeId, onactivate, onclose, onnew, onreorder }: Props = $props();

  let scroller = $state<HTMLDivElement>();
  let focusedId = $state<string | null>(null);
  let dragId = $state<string | null>(null);

  const ids = $derived(entries.map((e) => e.id));
  /** Pestaña con tabindex 0 (roving): la enfocada, o la activa. */
  const tabStop = $derived(focusedId && ids.includes(focusedId) ? focusedId : (activeId ?? ids[0] ?? null));

  /**
   * Detalle accesible de la pestaña. El nombre accesible sale del contenido visible (repo, rama y
   * contador) más este texto oculto: un `aria-label` que reordenara el texto rompería
   * «Label in Name» (axe `label-content-name-mismatch`). Se conserva además como tooltip.
   */
  function details(e: Entry): string {
    if (e.kind === 'new') return '';
    const i = e.info;
    const parts: string[] = [];
    if (i.missing) parts.push(t('tabs.a11y.missing'));
    if (i.branch) parts.push(t('tabs.a11y.branch', { branch: i.branch }));
    if (i.detached) parts.push(t('tabs.a11y.detached'));
    if (i.changeCount > 0) parts.push(tn('tabs.a11y.changes', i.changeCount));
    if (i.conflictCount > 0) parts.push(tn('tabs.a11y.conflicts', i.conflictCount));
    if (i.busy) parts.push(t('tabs.a11y.busy', { op: i.busy }));
    return parts.join(', ');
  }

  function label(e: Entry): string {
    return e.kind === 'new' ? t('tabs.newTab') : [e.info.name, details(e)].filter(Boolean).join(', ');
  }

  function tabEls(): HTMLElement[] {
    return scroller ? Array.from(scroller.querySelectorAll<HTMLElement>('[role="tab"]')) : [];
  }

  function focusTab(id: string) {
    focusedId = id;
    tabEls()
      .find((el) => el.dataset.tabId === id)
      ?.focus();
  }

  function currentTabId(): string | null {
    const active = document.activeElement as HTMLElement | null;
    if (active?.getAttribute('role') === 'tab' && scroller?.contains(active)) return active.dataset.tabId ?? null;
    return activeId;
  }

  /** Foco en un campo de texto fuera de la barra: Alt+Mayús+←/→ es «seleccionar por palabras». */
  function typingElsewhere(): boolean {
    const el = document.activeElement as HTMLElement | null;
    if (!el || (scroller && scroller.contains(el))) return false;
    return el.isContentEditable || el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || !!el.closest?.('[data-scope~="editor"]');
  }

  function move(delta: number): boolean {
    if (typingElsewhere()) return false;
    const id = currentTabId();
    if (!id) return false;
    const from = ids.indexOf(id);
    const next = moveItem(ids, from, delta);
    if (next.join('|') !== ids.join('|')) {
      onreorder(next);
      // Tras el reordenado el DOM se actualiza: conserva el foco en la misma pestaña.
      queueMicrotask(() => focusTab(id));
    }
    return true;
  }

  onMount(() =>
    registerHandlers({
      'tab.moveLeft': () => move(-1),
      'tab.moveRight': () => move(1)
    })
  );

  $effect(() => {
    // Mantiene visible la pestaña activa cuando hay scroll horizontal.
    const id = activeId;
    if (!id) return;
    const el = tabEls().find((x) => x.dataset.tabId === id);
    el?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
  });

  function onKeydown(e: KeyboardEvent, id: string) {
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    const i = ids.indexOf(id);
    let target: number | null = null;
    switch (e.key) {
      case 'ArrowRight':
        target = (i + 1) % ids.length;
        break;
      case 'ArrowLeft':
        target = (i - 1 + ids.length) % ids.length;
        break;
      case 'Home':
        target = 0;
        break;
      case 'End':
        target = ids.length - 1;
        break;
      case 'Enter':
      case ' ':
        e.preventDefault();
        onactivate(id);
        return;
      case 'Delete':
        e.preventDefault();
        onclose(id);
        return;
      default:
        return;
    }
    e.preventDefault();
    const tid = ids[target as number];
    if (tid) focusTab(tid);
  }

  // ---- Reordenar arrastrando (pointer events: funciona con la WebView sin DnD HTML5) ----
  let drag: { id: string; startX: number; active: boolean; pointerId: number } | null = null;
  let suppressClick = false;

  function onPointerDown(e: PointerEvent, id: string) {
    if (e.button !== 0) return;
    drag = { id, startX: e.clientX, active: false, pointerId: e.pointerId };
  }

  function onPointerMove(e: PointerEvent) {
    if (!drag) return;
    if (!drag.active) {
      if (Math.abs(e.clientX - drag.startX) < 6) return;
      drag.active = true;
      dragId = drag.id;
      (e.currentTarget as HTMLElement).setPointerCapture?.(drag.pointerId);
    }
    const els = tabEls();
    const from = ids.indexOf(drag.id);
    const over = els.findIndex((el) => {
      const r = el.parentElement?.getBoundingClientRect();
      return r ? e.clientX >= r.left && e.clientX <= r.right : false;
    });
    if (over >= 0 && over !== from) {
      onreorder(moveItem(ids, from, over - from));
    }
  }

  function endDrag() {
    if (drag?.active) suppressClick = true;
    drag = null;
    dragId = null;
    setTimeout(() => (suppressClick = false), 0);
  }

  function onClick(id: string) {
    if (suppressClick) return;
    onactivate(id);
  }

  function onAux(e: MouseEvent, id: string) {
    if (e.button === 1) {
      e.preventDefault();
      onclose(id);
    }
  }
</script>

<div class="strip" data-tauri-drag-region>
  <div class="scroller" role="tablist" aria-label={t('tabs.list')} aria-orientation="horizontal" bind:this={scroller}>
    {#each entries as entry (entry.id)}
      {@const isActive = entry.id === activeId}
      <div
        class="tab"
        class:active={isActive}
        class:dragging={dragId === entry.id}
        class:missing={entry.kind === 'repo' && entry.info.missing}
        role="presentation"
        style:--hue={entry.kind === 'repo' ? repoHue(entry.info.name) : 0}
        animate:flip={{ duration: 180 }}
        in:tabIn
        out:tabOut
      >
        <button
          type="button"
          role="tab"
          id="tab-{entry.id}"
          data-tab-id={entry.id}
          data-tab-kind={entry.kind}
          aria-selected={isActive}
          aria-controls={isActive ? 'tabpanel-main' : undefined}
          title={label(entry)}
          tabindex={tabStop === entry.id ? 0 : -1}
          class="tab-btn"
          onclick={() => onClick(entry.id)}
          onkeydown={(e) => onKeydown(e, entry.id)}
          onfocus={() => (focusedId = entry.id)}
          onpointerdown={(e) => onPointerDown(e, entry.id)}
          onpointermove={onPointerMove}
          onpointerup={endDrag}
          onpointercancel={endDrag}
          onauxclick={(e) => onAux(e, entry.id)}
        >
          {#if entry.kind === 'new'}
            <span class="glyph new"><Icon name="plus" size={13} /></span>
            <span class="name">{t('tabs.newTab')}</span>
          {:else}
            {@const i = entry.info}
            <span class="glyph" aria-hidden="true">
              {#if i.busy}
                <Icon name="loader" size={13} spin />
              {:else if i.missing}
                <Icon name="triangle-alert" size={13} />
              {:else}
                <span class="dot"></span>
              {/if}
            </span>
            <span class="name">{i.name}</span>
            {#if i.branch && !i.missing}
              <span class="branch mono">{i.branch}</span>
            {/if}
            {#if i.conflictCount > 0}
              <span class="pill conflict" title={tn('tabs.a11y.conflicts', i.conflictCount)}>
                <Icon name="triangle-alert" size={10} />{i.conflictCount}
              </span>
            {:else if i.changeCount > 0}
              <span class="pill count" title={tn('tabs.a11y.changes', i.changeCount)}>{i.changeCount}</span>
            {/if}
            <span class="sr-only">{details(entry)}</span>
          {/if}
        </button>
        <!-- Un `tablist` solo admite hijos `tab`: el botón de cerrar queda fuera del árbol de
             accesibilidad (el teclado cierra con Supr o Mod+W sobre la pestaña enfocada). -->
        <button
          type="button"
          class="close"
          tabindex="-1"
          aria-hidden="true"
          data-testid="tab-close"
          title={`${t('tabs.closeHint')} — ${entry.kind === 'new' ? t('tabs.newTab') : entry.info.name}`}
          onclick={() => onclose(entry.id)}
        >
          <Icon name="x" size={12} />
        </button>
      </div>
    {/each}
  </div>
  <button
    type="button"
    class="new"
    data-testid="tab-new"
    aria-label={t('tabs.new')}
    title={entries.length >= SOFT_TAB_LIMIT ? t('tabs.limitBody', { n: SOFT_TAB_LIMIT }) : t('tabs.new')}
    onclick={onnew}
  >
    <Icon name="plus" size={15} />
  </button>
  {#if entries.length >= SOFT_TAB_LIMIT - 4}
    <span class="count chip" class:over={entries.length >= SOFT_TAB_LIMIT} aria-hidden="true">{entries.length}/{SOFT_TAB_LIMIT}</span>
  {/if}
</div>

<style>
  .strip {
    display: flex;
    align-items: center;
    gap: 2px;
    min-width: 0;
    flex: 0 1 auto;
    align-self: stretch;
  }
  .scroller {
    display: flex;
    align-items: center;
    gap: 2px;
    min-width: 0;
    height: 100%;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }
  .scroller::-webkit-scrollbar {
    display: none;
  }
  .tab {
    --tone: hsl(var(--hue) 78% 66%);
    position: relative;
    display: flex;
    align-items: center;
    flex: 0 1 auto;
    min-width: 112px;
    max-width: 264px;
    height: 30px;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    color: var(--text-muted);
    transition:
      background-color var(--dur-fast) var(--ease),
      border-color var(--dur-fast) var(--ease),
      color var(--dur-fast) var(--ease),
      box-shadow var(--dur) var(--ease),
      transform var(--dur) var(--ease);
  }
  :global(:root[data-theme='light']) .tab {
    --tone: hsl(var(--hue) 70% 46%);
  }
  @media (prefers-color-scheme: light) {
    :global(:root:not([data-theme])) .tab {
      --tone: hsl(var(--hue) 70% 46%);
    }
  }
  .tab:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .tab.active {
    background: var(--bg-surface);
    border-color: var(--border-hover);
    box-shadow: var(--elev-1);
    color: var(--text);
  }
  .tab.dragging {
    z-index: 2;
    transform: translateY(-1px) scale(1.02);
    box-shadow: var(--elev-2);
  }
  .tab.missing {
    color: var(--text-subtle);
  }
  .tab-btn {
    display: flex;
    align-items: center;
    gap: 7px;
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0 0 0 var(--space-3);
    border: none;
    border-radius: var(--radius-md);
    background: transparent;
    color: inherit;
    font-size: var(--text-sm);
    font-weight: 550;
    text-align: left;
    touch-action: none;
  }
  .tab-btn:focus-visible {
    outline-offset: -2px;
    border-radius: var(--radius-md);
  }
  .glyph {
    display: inline-grid;
    place-items: center;
    width: 14px;
    height: 14px;
    flex: none;
    color: var(--accent);
  }
  .tab.missing .glyph {
    color: var(--warning);
  }
  .glyph.new {
    color: var(--text-subtle);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--tone);
    box-shadow:
      0 0 0 2px color-mix(in srgb, var(--tone) 22%, transparent),
      0 0 10px -1px color-mix(in srgb, var(--tone) 70%, transparent);
    transition: transform var(--dur) var(--ease-out);
  }
  .tab.active .dot {
    transform: scale(1.15);
  }
  .name {
    flex: 0 0 auto;
    min-width: 3ch;
    max-width: 18ch;
    padding-right: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .branch {
    flex: 0 20 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-subtle);
    font-size: var(--text-2xs);
    font-weight: 400;
    opacity: 0.85;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    flex: none;
    height: 16px;
    padding: 0 6px;
    border-radius: var(--pill);
    font-size: var(--text-2xs);
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }
  .pill.count {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .pill.conflict {
    background: var(--st-unmerged-bg);
    color: var(--st-unmerged-fg);
  }
  .close {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    margin: 0 5px 0 2px;
    padding: 0;
    flex: none;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--text-subtle);
    opacity: 0;
    transform: scale(0.85);
    transition:
      opacity var(--dur-fast) var(--ease),
      transform var(--dur-fast) var(--ease),
      background-color var(--dur-fast) var(--ease);
  }
  .tab:hover .close,
  .tab.active .close,
  .close:focus-visible {
    opacity: 1;
    transform: none;
  }
  .close:hover {
    background: var(--bg-pressed);
    color: var(--text);
  }
  .tab.active:not(:hover) .close {
    opacity: 0.55;
  }
  .new {
    display: grid;
    place-items: center;
    flex: none;
    width: 28px;
    height: 28px;
    margin-left: 2px;
    border: none;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-subtle);
    transition:
      background-color var(--dur-fast) var(--ease),
      color var(--dur-fast) var(--ease),
      transform var(--dur-fast) var(--ease);
  }
  .new:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .new:active {
    transform: scale(0.94);
  }
  .count {
    flex: none;
  }
  .count.over {
    background: var(--warning-soft);
    color: var(--warning);
  }
  @media (forced-colors: active) {
    .tab.active {
      border-color: Highlight;
    }
    .dot {
      forced-color-adjust: none;
      background: CanvasText;
      box-shadow: none;
    }
  }
</style>
