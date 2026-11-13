<script lang="ts">
  import { t, tn } from '$lib/i18n';
  import type { StatusEntry } from '$lib/api/types';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { settings } from '$lib/stores/settings.svelte';
  import { SHORTCUTS, formatCombo, isMac } from '$lib/shortcuts/registry';
  import { displayStatus, SECTION_ORDER, type SectionId } from '$lib/util/changes';
  import { pathBase, pathDir, pathKey, pathText } from '$lib/util/gitpath';
  import { buildTreeRows } from '$lib/util/tree';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';
  import StashPushForm from './StashPushForm.svelte';
  import VirtualList from './VirtualList.svelte';
  import { stashOf } from '$lib/stores/stash.svelte';

  let { tab }: { tab: TabState } = $props();
  const stash = $derived(stashOf(tab));

  type Item =
    | { type: 'header'; section: SectionId; count: number }
    | { type: 'dir'; section: SectionId; key: string; name: string; depth: number; entries: StatusEntry[]; open: boolean }
    | { type: 'file'; section: SectionId; entry: StatusEntry; depth: number };

  let collapsed = $state<Record<string, boolean>>({});
  let closedDirs = $state.raw<ReadonlySet<string>>(new Set());
  let focusIndex = $state(0);
  let list = $state<ReturnType<typeof VirtualList<Item>>>();
  /** Selección múltiple (Mayús/Ctrl): claves `sección:ruta`. La fila «activa» del diff es `tab.selected`. */
  let multi = $state.raw<ReadonlySet<string>>(new Set());
  let anchor = $state<string | null>(null);
  const gridId = `changes-${Math.random().toString(36).slice(2, 7)}`;

  const q = $derived(tab.filter.trim().toLowerCase());
  const matches = (e: StatusEntry) => q === '' || pathText(e.path).toLowerCase().includes(q);
  const tree = $derived(settings.changesView === 'tree');

  const items = $derived.by<Item[]>(() => {
    const out: Item[] = [];
    for (const section of SECTION_ORDER) {
      if (section === 'ignored' && !tab.showIgnored) continue;
      const entries = tab.sections[section].filter(matches);
      if (entries.length === 0) continue;
      out.push({ type: 'header', section, count: entries.length });
      if (collapsed[section]) continue;
      if (tree) out.push(...buildTreeRows(section, entries, closedDirs));
      else for (const entry of entries) out.push({ type: 'file', section, entry, depth: 0 });
    }
    return out;
  });

  const total = $derived(SECTION_ORDER.reduce((n, s) => n + (s === 'ignored' && !tab.showIgnored ? 0 : tab.sections[s].length), 0));
  const clamped = $derived(Math.min(focusIndex, Math.max(0, items.length - 1)));

  const heightOf = (it: Item) => (it.type === 'header' ? settings.headerHeight : settings.rowHeight);
  const rowId = (i: number) => `${gridId}-r${i}`;
  const keyOf = (section: SectionId, e: StatusEntry) => `${section}:${pathKey(e.path)}`;

  const SECTION_LABEL: Record<SectionId, 'changes.staged' | 'changes.unstaged' | 'changes.untracked' | 'changes.ignored' | 'changes.conflicts'> = {
    staged: 'changes.staged',
    unstaged: 'changes.unstaged',
    untracked: 'changes.untracked',
    ignored: 'changes.ignored',
    conflicts: 'changes.conflicts'
  };

  function toggleEntry(section: SectionId, entry: StatusEntry) {
    if (section === 'staged') void tab.unstage([entry]);
    else if (section === 'unstaged' || section === 'untracked') void tab.stage(section, [entry]);
  }

  function sectionEntries(section: SectionId): StatusEntry[] {
    return tab.sections[section].filter(matches);
  }

  // ---- Selección -------------------------------------------------------------------------

  /** Entradas seleccionadas (varias) agrupadas por sección. */
  const picked = $derived.by(() => {
    const out: { section: SectionId; entry: StatusEntry }[] = [];
    if (multi.size < 2) return out;
    for (const it of items) if (it.type === 'file' && multi.has(keyOf(it.section, it.entry))) out.push({ section: it.section, entry: it.entry });
    return out;
  });
  const pickedStageable = $derived(picked.filter((p) => p.section === 'unstaged' || p.section === 'untracked'));
  const pickedStaged = $derived(picked.filter((p) => p.section === 'staged'));

  function activate(i: number) {
    const it = items[i];
    if (!it) return;
    focusIndex = i;
    if (it.type === 'file') tab.select(it.section, it.entry);
  }

  function rangeSelect(from: string, toIndex: number) {
    const to = items[toIndex];
    if (!to || to.type !== 'file') return;
    const a = items.findIndex((x) => x.type === 'file' && keyOf(x.section, x.entry) === from);
    if (a < 0) return;
    const [lo, hi] = a < toIndex ? [a, toIndex] : [toIndex, a];
    const next = new Set<string>();
    for (let k = lo; k <= hi; k++) {
      const x = items[k];
      if (x && x.type === 'file' && x.section === to.section) next.add(keyOf(x.section, x.entry));
    }
    multi = next;
  }

  function onRowClick(i: number, ev: MouseEvent) {
    const it = items[i];
    if (!it || it.type !== 'file') return activate(i);
    const key = keyOf(it.section, it.entry);
    if (ev.shiftKey && anchor) {
      rangeSelect(anchor, i);
      activate(i);
      return;
    }
    if (ev.ctrlKey || ev.metaKey) {
      const next = new Set(multi);
      const cur = tab.selected;
      if (next.size === 0 && cur) next.add(`${cur.section}:${cur.key}`);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      multi = next;
      anchor = key;
      activate(i);
      return;
    }
    multi = new Set();
    anchor = key;
    activate(i);
  }

  function clearSelection() {
    multi = new Set();
  }

  async function stagePicked() {
    const by = new Map<SectionId, StatusEntry[]>();
    for (const p of pickedStageable) by.set(p.section, [...(by.get(p.section) ?? []), p.entry]);
    clearSelection();
    for (const [section, entries] of by) await tab.stage(section, entries);
  }

  async function unstagePicked() {
    const entries = pickedStaged.map((p) => p.entry);
    clearSelection();
    await tab.unstage(entries);
  }

  async function discardPicked() {
    const by = new Map<SectionId, StatusEntry[]>();
    for (const p of pickedStageable) by.set(p.section, [...(by.get(p.section) ?? []), p.entry]);
    for (const [section, entries] of by) await tab.discard(section, entries);
    clearSelection();
  }

  function toggleDir(key: string) {
    const next = new Set(closedDirs);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    closedDirs = next;
  }

  function dirAction(section: SectionId, entries: StatusEntry[]) {
    if (section === 'staged') void tab.unstage(entries);
    else void tab.stage(section, entries);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const last = items.length - 1;
    let next = clamped;
    switch (e.key) {
      case 'ArrowDown':
        next = Math.min(last, clamped + 1);
        break;
      case 'ArrowUp':
        next = Math.max(0, clamped - 1);
        break;
      case 'Home':
        next = 0;
        break;
      case 'End':
        next = last;
        break;
      case 'PageDown':
        next = Math.min(last, clamped + 10);
        break;
      case 'PageUp':
        next = Math.max(0, clamped - 10);
        break;
      case ' ':
      case 'Enter': {
        const it = items[clamped];
        if (!it) return;
        e.preventDefault();
        if (it.type === 'header') collapsed = { ...collapsed, [it.section]: !collapsed[it.section] };
        else if (it.type === 'dir') toggleDir(it.key);
        else if (e.key === ' ' && picked.length > 1) {
          if (pickedStageable.length > 0) void stagePicked();
          else void unstagePicked();
        } else if (e.key === ' ') toggleEntry(it.section, it.entry);
        else tab.select(it.section, it.entry);
        return;
      }
      case 'Escape':
        if (multi.size > 0) {
          e.preventDefault();
          clearSelection();
        }
        return;
      default:
        return;
    }
    e.preventDefault();
    if (e.shiftKey) {
      const cur = items[clamped];
      if (cur && cur.type === 'file' && !anchor) anchor = keyOf(cur.section, cur.entry);
      activate(next);
      if (anchor) rangeSelect(anchor, next);
    } else activate(next);
    list?.scrollToIndex(next);
  }

  function statusKey(letter: string): string {
    switch (letter) {
      case 'A':
        return 'added';
      case 'M':
        return 'modified';
      case 'D':
        return 'deleted';
      case 'R':
      case 'C':
        return 'renamed';
      case 'U':
        return 'unmerged';
      case '?':
      case '!':
        return 'untracked';
      default:
        return 'modified';
    }
  }

  function statusName(letter: string): string {
    switch (letter) {
      case 'A':
        return t('status.A');
      case 'M':
        return t('status.M');
      case 'D':
        return t('status.D');
      case 'R':
        return t('status.R');
      case 'C':
        return t('status.C');
      case 'U':
        return t('status.U');
      case 'T':
        return t('status.T');
      case '?':
        return t('status.untracked');
      case '!':
        return t('status.ignored');
      default:
        return t('status.M');
    }
  }

  const glyph = (letter: string) => (letter === '?' ? '+' : letter);
  const stashDef = SHORTCUTS.find((x) => x.id === 'stash.push');
  const stashCombo = stashDef ? formatCombo(stashDef.combo, isMac()) : '';
</script>

<div class="changes">
  <div class="toolbar">
    <div class="search">
      <Icon name="search" size={14} />
      <input class="field" type="search" placeholder={t('changes.filter')} aria-label={t('changes.filter')} bind:value={tab.filter} />
    </div>
    <div class="seg" role="group" aria-label={t('changes.view')}>
      <button type="button" class="seg-btn" class:on={!tree} aria-pressed={!tree} aria-label={t('changes.viewList')} title={t('changes.viewList')} onclick={() => settings.setChangesView('list')}>
        <Icon name="list" size={14} />
      </button>
      <button type="button" class="seg-btn" class:on={tree} aria-pressed={tree} aria-label={t('changes.viewTree')} title={t('changes.viewTree')} onclick={() => settings.setChangesView('tree')}>
        <Icon name="folder-tree" size={14} />
      </button>
    </div>
    <label class="switch" title={t('changes.showIgnored')}>
      <input type="checkbox" role="switch" bind:checked={tab.showIgnored} />
      <span class="track" aria-hidden="true"></span>
      <span class="sw-label">{t('changes.showIgnoredShort')}</span>
    </label>
    <button type="button" class="btn btn-ghost btn-icon btn-sm" onclick={() => tab.refresh()} aria-label={t('changes.refresh')} title={t('changes.refresh')}>
      <Icon name="refresh-cw" size={14} />
    </button>
    <button
      type="button"
      class="btn btn-ghost btn-icon btn-sm"
      onclick={() => (stash.pushOpen ? stash.closePush() : stash.openPush())}
      aria-expanded={stash.pushOpen}
      aria-label={t('stash.save')}
      title={`${t('stash.save')} (${stashCombo})`}
    >
      <Icon name="archive" size={14} />
    </button>
  </div>

  {#if stash.pushOpen}<StashPushForm {tab} />{/if}

  {#if picked.length > 1}
    <div class="selbar" role="toolbar" aria-label={t('changes.selectionBar')}>
      <span class="sel-count num">{tn('changes.selected', picked.length)}</span>
      <span class="spacer"></span>
      {#if pickedStageable.length > 0}
        <button type="button" class="btn btn-sm" onclick={stagePicked}><Icon name="plus" size={12} />{t('changes.stageSelected')}</button>
      {/if}
      {#if pickedStaged.length > 0}
        <button type="button" class="btn btn-sm" onclick={unstagePicked}><Icon name="minus" size={12} />{t('changes.unstageSelected')}</button>
      {/if}
      {#if pickedStageable.length > 0}
        <button type="button" class="btn btn-sm btn-danger" onclick={discardPicked}><Icon name="undo" size={12} />{t('changes.discardSelected')}</button>
      {/if}
      <button type="button" class="btn btn-ghost btn-icon btn-sm" onclick={clearSelection} aria-label={t('changes.clearSelection')} title={t('changes.clearSelection')}>
        <Icon name="x" size={13} />
      </button>
    </div>
  {/if}

  <div class="listwrap">
    {#if tab.loading && !tab.snapshot}
      <div class="skeletons" aria-busy="true" aria-label={t('changes.loading')}>
        {#each [70, 52, 64, 45, 58, 40, 66, 50] as w, i (i)}
          <div class="sk-row" style:opacity={1 - i * 0.08}>
            <span class="skeleton" style:width="14px" style:height="14px"></span>
            <span class="skeleton" style:width="16px" style:height="16px" style:border-radius="5px"></span>
            <span class="skeleton" style:width="{w}%" style:height="11px"></span>
          </div>
        {/each}
      </div>
    {:else if items.length === 0}
      {#if total === 0}
        <EmptyState icon="circle-check" title={t('changes.cleanTitle')} body={t('changes.cleanBody')} />
      {:else}
        <EmptyState icon="search" title={t('changes.noMatchTitle')} body={t('changes.noMatchBody', { q: tab.filter })}>
          <button type="button" class="btn" onclick={() => (tab.filter = '')}>{t('changes.clearFilter')}</button>
        </EmptyState>
      {/if}
    {:else}
      <VirtualList
        bind:this={list}
        {items}
        {heightOf}
        role="grid"
        label={t('changes.gridLabel')}
        rowCount={items.length}
        multiselectable={true}
        activeDescendant={rowId(clamped)}
        tabindex={0}
        onkeydown={onKeydown}
      >
        {#snippet row(it, i)}
          {#if it.type === 'header'}
            <div
              class="row header"
              data-section={it.section}
              role="row"
              id={rowId(i)}
              aria-rowindex={i + 1}
              class:focused={i === clamped}
              style:height="{settings.headerHeight}px"
            >
              <!-- aria-expanded no es válido en role=row de un grid: va en la celda -->
              <div role="gridcell" class="cell head-cell" aria-expanded={!collapsed[it.section]}>
                <button
                  type="button"
                  class="collapse"
                  tabindex="-1"
                  aria-label={t('changes.toggleSection', { name: t(SECTION_LABEL[it.section]) })}
                  onclick={() => {
                    collapsed = { ...collapsed, [it.section]: !collapsed[it.section] };
                    focusIndex = i;
                  }}
                >
                  <Icon name={collapsed[it.section] ? 'chevron-right' : 'chevron-down'} size={13} />
                </button>
                <span class="bullet" aria-hidden="true"></span>
                <span class="title">{t(SECTION_LABEL[it.section])}</span>
                <span class="chip num">{it.count}</span>
                <span class="spacer"></span>
                {#if it.section === 'staged'}
                  <button type="button" class="btn btn-ghost btn-sm head-act" tabindex="-1" onclick={() => tab.unstage(sectionEntries('staged'))}>
                    {t('changes.unstageAll')}
                  </button>
                {:else if it.section === 'unstaged' || it.section === 'untracked'}
                  <button type="button" class="btn btn-ghost btn-sm head-act" tabindex="-1" onclick={() => tab.stage(it.section, sectionEntries(it.section))}>
                    {t('changes.stageAll')}
                  </button>
                {/if}
              </div>
            </div>
          {:else if it.type === 'dir'}
            <div
              class="row dir"
              role="row"
              id={rowId(i)}
              aria-rowindex={i + 1}
              class:focused={i === clamped}
              style:height="{settings.rowHeight}px"
              style:--depth={it.depth}
              onclick={() => {
                focusIndex = i;
                toggleDir(it.key);
              }}
              onkeydown={() => {}}
              tabindex="-1"
            >
              <div role="gridcell" class="cell dir-cell" aria-expanded={it.open}>
                <Icon name={it.open ? 'chevron-down' : 'chevron-right'} size={12} />
                <Icon name="folder" size={13} />
                <span class="dir-name">{it.name}</span>
                <span class="dir-count num">{it.entries.length}</span>
                <span class="spacer"></span>
                <button
                  type="button"
                  class="btn btn-ghost btn-icon btn-sm hover-act"
                  tabindex="-1"
                  aria-label={it.section === 'staged' ? t('changes.unstageDir', { name: it.name }) : t('changes.stageDir', { name: it.name })}
                  title={it.section === 'staged' ? t('changes.unstageAll') : t('changes.stageAll')}
                  onclick={(ev) => {
                    ev.stopPropagation();
                    dirAction(it.section, it.entries);
                  }}
                >
                  <Icon name={it.section === 'staged' ? 'minus' : 'plus'} size={13} />
                </button>
              </div>
            </div>
          {:else}
            {@const e = it.entry}
            {@const letter = displayStatus(e, it.section)}
            {@const key = keyOf(it.section, e)}
            {@const busy = tab.pending.has(key)}
            {@const isStagedSection = it.section === 'staged'}
            {@const canToggle = it.section === 'staged' || it.section === 'unstaged' || it.section === 'untracked'}
            {@const picked1 = tab.isSelected(it.section, e) || multi.has(key)}
            <div
              class="row file"
              role="row"
              id={rowId(i)}
              aria-rowindex={i + 1}
              aria-selected={picked1}
              aria-busy={busy}
              class:selected={picked1}
              class:primary={tab.isSelected(it.section, e)}
              class:focused={i === clamped}
              class:busy
              style:height="{settings.rowHeight}px"
              style:--depth={it.depth}
              onclick={(ev) => onRowClick(i, ev)}
              onkeydown={() => {}}
              tabindex="-1"
            >
              <div role="gridcell" class="cell check">
                {#if canToggle}
                  <label class="cb">
                    <input
                      type="checkbox"
                      tabindex="-1"
                      checked={isStagedSection}
                      disabled={busy}
                      aria-label={isStagedSection ? t('changes.unstageFile', { path: pathText(e.path) }) : t('changes.stageFile', { path: pathText(e.path) })}
                      onclick={(ev) => ev.stopPropagation()}
                      onchange={() => toggleEntry(it.section, e)}
                    />
                    <span class="box" aria-hidden="true"><Icon name="check" size={11} /></span>
                  </label>
                {:else}
                  <span class="cb-spacer"></span>
                {/if}
              </div>
              <div role="gridcell" class="cell status">
                <span class="st st-{statusKey(letter)}" title={statusName(letter)} aria-label={statusName(letter)}>{glyph(letter)}</span>
              </div>
              <div role="gridcell" class="cell name" title={pathText(e.path)}>
                <span class="base">{pathBase(e.path)}</span>
                {#if !tree || e.originalPath}
                  <span class="dir mono">
                    {#if e.originalPath}{pathText(e.originalPath)} → {/if}{!tree ? pathDir(e.path) || t('changes.root') : ''}
                  </span>
                {/if}
              </div>
              <div role="gridcell" class="cell extra">
                {#if e.conflictKind}
                  <span class="chip conflict">{t(`conflict.${e.conflictKind}`)}</span>
                {/if}
                {#if it.section === 'unstaged' || it.section === 'untracked'}
                  <button
                    type="button"
                    class="btn btn-ghost btn-icon btn-sm hover-act"
                    tabindex="-1"
                    aria-label={t('changes.discardFile', { path: pathText(e.path) })}
                    title={t('changes.discard')}
                    disabled={busy}
                    onclick={(ev) => {
                      ev.stopPropagation();
                      void tab.discard(it.section, [e]);
                    }}
                  >
                    <Icon name="undo" size={13} />
                  </button>
                {/if}
                {#if canToggle}
                  <button
                    type="button"
                    class="btn btn-ghost btn-icon btn-sm hover-act"
                    tabindex="-1"
                    aria-label={isStagedSection ? t('changes.unstageFile', { path: pathText(e.path) }) : t('changes.stageFile', { path: pathText(e.path) })}
                    title={isStagedSection ? t('changes.unstageOne') : t('changes.stageOne')}
                    disabled={busy}
                    onclick={(ev) => {
                      ev.stopPropagation();
                      toggleEntry(it.section, e);
                    }}
                  >
                    <Icon name={isStagedSection ? 'minus' : 'plus'} size={13} />
                  </button>
                {/if}
              </div>
            </div>
          {/if}
        {/snippet}
      </VirtualList>
    {/if}
  </div>

  <p class="summary num" aria-live="off">{tn('changes.summary', total)}</p>
</div>

<style>
  .changes {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg-surface);
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
  }
  .search {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    color: var(--text-subtle);
  }
  .search :global(svg) {
    position: absolute;
    left: 9px;
    pointer-events: none;
  }
  .search .field {
    padding-left: 28px;
    min-height: 28px;
    font-size: var(--text-sm);
    border-radius: var(--radius-md);
  }
  .seg {
    display: inline-flex;
    padding: 2px;
    gap: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
  }
  .seg-btn {
    display: grid;
    place-items: center;
    width: 24px;
    height: 22px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-subtle);
    transition:
      background-color var(--dur-fast) var(--ease),
      color var(--dur-fast) var(--ease),
      box-shadow var(--dur-fast) var(--ease);
  }
  .seg-btn:hover {
    color: var(--text);
  }
  .seg-btn.on {
    background: var(--bg-raised);
    color: var(--accent);
    box-shadow: var(--elev-1);
  }
  .switch {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: var(--target-min);
    color: var(--text-muted);
    font-size: var(--text-xs);
    white-space: nowrap;
    cursor: pointer;
  }
  .switch input {
    position: absolute;
    opacity: 0;
    width: 1px;
    height: 1px;
  }
  .track {
    position: relative;
    width: 26px;
    height: 15px;
    border-radius: var(--pill);
    background: var(--bg-muted);
    border: 1px solid var(--border-strong);
    transition: background var(--dur) var(--ease), border-color var(--dur) var(--ease);
  }
  .track::after {
    content: '';
    position: absolute;
    top: 1px;
    left: 1px;
    width: 11px;
    height: 11px;
    border-radius: 50%;
    background: var(--text-muted);
    transition: transform var(--dur) var(--ease-out), background var(--dur) var(--ease);
  }
  .switch input:checked + .track {
    background: var(--grad-accent);
    border-color: transparent;
  }
  .switch input:checked + .track::after {
    transform: translateX(11px);
    background: var(--accent-fg);
  }
  .switch input:focus-visible + .track {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .selbar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0 var(--space-3) var(--space-2);
    padding: 4px 4px 4px var(--space-3);
    border: 1px solid color-mix(in srgb, var(--accent) 35%, var(--border));
    border-radius: var(--radius-md);
    background: var(--grad-accent-soft), var(--bg-raised);
    animation: rise-in var(--dur) var(--ease-out);
  }
  .sel-count {
    color: var(--accent);
    font-size: var(--text-xs);
    font-weight: 650;
  }
  .spacer {
    flex: 1;
  }
  .listwrap {
    flex: 1;
    min-height: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-3);
    position: relative;
  }
  .row.header {
    margin-top: 2px;
    padding-left: var(--space-2);
    font-size: var(--text-xs);
    font-weight: 650;
    color: var(--text-muted);
  }
  .head-cell {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
  }
  .bullet {
    width: 7px;
    height: 7px;
    flex: none;
    border-radius: 50%;
    background: var(--text-subtle);
  }
  [data-section='staged'] .bullet {
    background: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-glow);
  }
  [data-section='unstaged'] .bullet {
    background: var(--st-modified-fg);
  }
  [data-section='untracked'] .bullet {
    background: var(--st-untracked-fg);
  }
  [data-section='conflicts'] .bullet {
    background: var(--st-unmerged-fg);
  }
  .head-cell .title {
    color: var(--text);
    font-weight: 650;
    letter-spacing: -0.005em;
  }
  .collapse {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border: none;
    background: none;
    color: var(--text-subtle);
    border-radius: 5px;
  }
  .collapse:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .head-act {
    color: var(--text-subtle);
  }
  .row.file,
  .row.dir {
    margin: 0 var(--space-2);
    padding: 0 var(--space-2) 0 calc(var(--space-2) + var(--depth, 0) * 14px);
    border-radius: var(--radius-sm);
    cursor: default;
    transition: background-color var(--dur-fast) var(--ease);
  }
  .row.dir {
    color: var(--text-muted);
  }
  .row.dir:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .dir-cell {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    min-width: 0;
  }
  .dir-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-sm);
    font-weight: 550;
  }
  .dir-count {
    color: var(--text-subtle);
    font-size: var(--text-2xs);
  }
  .row.file:hover {
    background: var(--bg-hover);
  }
  .row.file.selected {
    background: var(--bg-selected);
  }
  .row.file.selected::before {
    content: '';
    position: absolute;
    left: 0;
    top: 5px;
    bottom: 5px;
    width: 2px;
    border-radius: 2px;
    background: var(--grad-accent);
    opacity: 0;
  }
  .row.file.primary::before {
    opacity: 1;
  }
  :global(.vl:focus-visible) .row.focused {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
    border-radius: var(--radius-sm);
  }
  .row.busy {
    opacity: 0.55;
  }
  .cell {
    display: flex;
    align-items: center;
  }
  .cell.name {
    flex: 1;
    min-width: 0;
    gap: 8px;
    align-items: baseline;
  }
  .base {
    flex: 0 1 auto;
    min-width: 0;
    max-width: 75%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
    font-size: var(--text-sm);
    font-weight: 560;
  }
  .dir {
    flex: 1 1 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-subtle);
    font-size: var(--text-2xs);
  }
  :global([data-density='compact']) .dir {
    display: none;
  }
  .cell.extra {
    gap: 2px;
    flex: none;
  }
  .hover-act {
    opacity: 0;
    transform: scale(0.9);
    transition: opacity var(--dur-fast) var(--ease), transform var(--dur-fast) var(--ease), background-color var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
  }
  .row:hover .hover-act,
  :global(.vl:focus-visible) .row.focused .hover-act,
  .hover-act:focus-visible {
    opacity: 1;
    transform: none;
  }
  .chip.conflict {
    background: var(--st-unmerged-bg);
    color: var(--st-unmerged-fg);
    font-weight: 600;
  }
  /* Casilla de preparar con objetivo ≥ 24 px. */
  .cb {
    position: relative;
    display: grid;
    place-items: center;
    width: var(--target-min);
    height: var(--target-min);
    cursor: pointer;
  }
  .cb input {
    position: absolute;
    inset: 0;
    margin: 0;
    opacity: 0;
    cursor: pointer;
  }
  .box {
    display: grid;
    place-items: center;
    width: 15px;
    height: 15px;
    border: 1.5px solid var(--border-strong);
    border-radius: 5px;
    background: transparent;
    color: transparent;
    transition:
      background var(--dur-fast) var(--ease),
      border-color var(--dur-fast) var(--ease),
      transform var(--dur-fast) var(--ease-out);
  }
  .cb input:checked + .box {
    background: var(--grad-accent);
    border-color: transparent;
    color: var(--accent-fg);
  }
  .cb:hover .box {
    border-color: var(--accent);
  }
  .cb input:active + .box {
    transform: scale(0.88);
  }
  .cb input:focus-visible + .box {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .cb-spacer {
    width: var(--target-min);
  }
  .st {
    display: inline-grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 5px;
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    font-weight: 700;
  }
  .st-added {
    background: var(--st-added-bg);
    color: var(--st-added-fg);
  }
  .st-modified {
    background: var(--st-modified-bg);
    color: var(--st-modified-fg);
  }
  .st-deleted {
    background: var(--st-deleted-bg);
    color: var(--st-deleted-fg);
  }
  .st-renamed {
    background: var(--st-renamed-bg);
    color: var(--st-renamed-fg);
  }
  .st-unmerged {
    background: var(--st-unmerged-bg);
    color: var(--st-unmerged-fg);
  }
  .st-untracked {
    background: var(--st-untracked-bg);
    color: var(--st-untracked-fg);
  }
  .skeletons {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
  }
  .sk-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .summary {
    padding: 4px var(--space-4);
    color: var(--text-subtle);
    font-size: var(--text-2xs);
  }
  @media (forced-colors: active) {
    .row.selected {
      outline: 2px solid Highlight;
      outline-offset: -2px;
    }
    .box {
      border-color: ButtonText;
    }
    .cb input:checked + .box {
      background: Highlight;
      color: HighlightText;
    }
  }
</style>
