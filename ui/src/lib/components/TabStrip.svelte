<script lang="ts">
  import { onMount } from 'svelte';
  import type { Entry } from '$lib/stores/app.svelte';
  import { SOFT_TAB_LIMIT } from '$lib/stores/app.svelte';
  import { t, tn } from '$lib/i18n';
  import { registerHandlers } from '$lib/shortcuts/registry';
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

  function label(e: Entry): string {
    if (e.kind === 'new') return t('tabs.newTab');
    const i = e.info;
    const parts = [i.name];
    if (i.missing) parts.push(t('tabs.a11y.missing'));
    if (i.branch) parts.push(t('tabs.a11y.branch', { branch: i.branch }));
    if (i.detached) parts.push(t('tabs.a11y.detached'));
    if (i.changeCount > 0) parts.push(tn('tabs.a11y.changes', i.changeCount));
    if (i.conflictCount > 0) parts.push(tn('tabs.a11y.conflicts', i.conflictCount));
    if (i.busy) parts.push(t('tabs.a11y.busy', { op: i.busy }));
    return parts.join(', ');
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

  function move(delta: number): boolean {
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
  <div
    class="scroller"
    role="tablist"
    aria-label={t('tabs.list')}
    aria-orientation="horizontal"
    bind:this={scroller}
  >
    {#each entries as entry (entry.id)}
      {@const isActive = entry.id === activeId}
      <div
        class="tab"
        class:active={isActive}
        class:dragging={dragId === entry.id}
        role="presentation"
      >
        <button
          type="button"
          role="tab"
          id="tab-{entry.id}"
          data-tab-id={entry.id}
          aria-selected={isActive}
          aria-controls={isActive ? 'tabpanel-main' : undefined}
          aria-label={label(entry)}
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
            <Icon name="plus" size={14} />
            <span class="name">{t('tabs.newTab')}</span>
          {:else}
            {@const i = entry.info}
            <Icon name={i.missing ? 'triangle-alert' : 'git-branch'} size={14} />
            <span class="name">{i.name}</span>
            {#if i.branch && !i.missing}
              <span class="branch mono">{i.branch}</span>
            {/if}
            {#if i.busy}
              <span class="ind" title={i.busy}><Icon name="loader" size={13} spin /></span>
            {/if}
            {#if i.conflictCount > 0}
              <span class="chip conflict" title={tn('tabs.a11y.conflicts', i.conflictCount)}>
                <Icon name="triangle-alert" size={11} />{i.conflictCount}
              </span>
            {:else if i.changeCount > 0}
              <span class="chip count" title={tn('tabs.a11y.changes', i.changeCount)}>{i.changeCount}</span>
            {/if}
          {/if}
        </button>
        <button
          type="button"
          class="close btn btn-ghost btn-icon btn-sm"
          tabindex="-1"
          aria-label={t('tabs.close', { name: entry.kind === 'new' ? t('tabs.newTab') : entry.info.name })}
          title={t('tabs.closeHint')}
          onclick={() => onclose(entry.id)}
        >
          <Icon name="x" size={14} />
        </button>
      </div>
    {/each}
  </div>
  <button
    type="button"
    class="new btn btn-ghost btn-icon"
    aria-label={t('tabs.new')}
    title={entries.length >= SOFT_TAB_LIMIT ? t('tabs.limitBody', { n: SOFT_TAB_LIMIT }) : t('tabs.new')}
    onclick={onnew}
  >
    <Icon name="plus" size={16} />
  </button>
  {#if entries.length >= SOFT_TAB_LIMIT - 4}
    <span class="count chip" class:over={entries.length >= SOFT_TAB_LIMIT} aria-hidden="true">
      {entries.length}/{SOFT_TAB_LIMIT}
    </span>
  {/if}
</div>

<style>
  .strip {
    display: flex;
    align-items: stretch;
    gap: var(--space-1);
    height: var(--tabstrip-h);
    padding: 0 var(--space-2);
    background: var(--bg-app);
    border-bottom: 1px solid var(--border);
  }
  .scroller {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    min-width: 0;
    overflow-x: auto;
    overflow-y: hidden;
    padding-top: 6px;
    scrollbar-width: none;
  }
  .scroller::-webkit-scrollbar {
    display: none;
  }
  .tab {
    position: relative;
    display: flex;
    align-items: center;
    flex: 0 1 auto;
    min-width: 168px;
    max-width: 280px;
    height: calc(var(--tabstrip-h) - 6px);
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: var(--radius-md) var(--radius-md) 0 0;
    color: var(--text-muted);
  }
  .tab:hover {
    background: var(--bg-subtle);
  }
  .tab.active {
    background: var(--bg-surface);
    border-color: var(--border);
    color: var(--text);
    margin-bottom: -1px;
    height: calc(var(--tabstrip-h) - 5px);
  }
  .tab.active::before {
    content: '';
    position: absolute;
    left: 8px;
    right: 8px;
    top: 0;
    height: 2px;
    border-radius: 0 0 2px 2px;
    background: var(--accent-brand);
  }
  .tab.dragging {
    box-shadow: var(--elev-1);
    z-index: 2;
  }
  .tab-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0 0 0 var(--space-3);
    border: none;
    background: transparent;
    color: inherit;
    font-size: var(--text-sm);
    font-weight: 500;
    text-align: left;
    touch-action: none;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .branch {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-subtle);
    font-size: 0.6875rem;
    font-weight: 400;
  }
  .ind {
    display: inline-flex;
    color: var(--accent);
  }
  .chip {
    height: 18px;
    font-size: 0.6875rem;
  }
  .chip.count {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .chip.conflict {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .close {
    margin: 0 var(--space-1);
    opacity: 0;
  }
  .tab:hover .close,
  .tab.active .close,
  .close:focus-visible {
    opacity: 1;
  }
  .new {
    align-self: center;
    flex: none;
  }
  .count {
    align-self: center;
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
    .tab.active::before {
      background: Highlight;
    }
  }
</style>
