<script lang="ts">
  import { t, tn } from '$lib/i18n';
  import type { StatusEntry } from '$lib/api/types';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { settings } from '$lib/stores/settings.svelte';
  import { displayStatus, SECTION_ORDER, type SectionId } from '$lib/util/changes';
  import { pathBase, pathDir, pathKey, pathText } from '$lib/util/gitpath';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';
  import VirtualList from './VirtualList.svelte';

  let { tab }: { tab: TabState } = $props();

  type Item =
    | { type: 'header'; section: SectionId; count: number }
    | { type: 'file'; section: SectionId; entry: StatusEntry };

  let collapsed = $state<Record<string, boolean>>({});
  let focusIndex = $state(0);
  let list = $state<ReturnType<typeof VirtualList<Item>>>();
  const gridId = `changes-${Math.random().toString(36).slice(2, 7)}`;

  const q = $derived(tab.filter.trim().toLowerCase());
  const matches = (e: StatusEntry) => q === '' || pathText(e.path).toLowerCase().includes(q);

  const items = $derived.by<Item[]>(() => {
    const out: Item[] = [];
    for (const section of SECTION_ORDER) {
      if (section === 'ignored' && !tab.showIgnored) continue;
      const entries = tab.sections[section].filter(matches);
      if (entries.length === 0) continue;
      out.push({ type: 'header', section, count: entries.length });
      if (!collapsed[section]) for (const entry of entries) out.push({ type: 'file', section, entry });
    }
    return out;
  });

  const total = $derived(
    SECTION_ORDER.reduce((n, s) => n + (s === 'ignored' && !tab.showIgnored ? 0 : tab.sections[s].length), 0)
  );
  const clamped = $derived(Math.min(focusIndex, Math.max(0, items.length - 1)));

  const heightOf = (it: Item) => (it.type === 'header' ? settings.headerHeight : settings.rowHeight);
  const rowId = (i: number) => `${gridId}-r${i}`;

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

  function activate(i: number) {
    const it = items[i];
    if (!it) return;
    focusIndex = i;
    if (it.type === 'file') tab.select(it.section, it.entry);
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
        else if (e.key === ' ') toggleEntry(it.section, it.entry);
        else tab.select(it.section, it.entry);
        return;
      }
      default:
        return;
    }
    e.preventDefault();
    activate(next);
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
</script>

<div class="changes">
  <div class="toolbar">
    <div class="search">
      <Icon name="search" size={14} />
      <input
        class="field"
        type="search"
        placeholder={t('changes.filter')}
        aria-label={t('changes.filter')}
        bind:value={tab.filter}
      />
    </div>
    <label class="switch" title={t('changes.showIgnored')}>
      <input type="checkbox" role="switch" bind:checked={tab.showIgnored} />
      <span class="track" aria-hidden="true"></span>
      <span class="sw-label">{t('changes.showIgnoredShort')}</span>
    </label>
    <button
      type="button"
      class="btn btn-ghost btn-icon"
      onclick={() => tab.refresh()}
      aria-label={t('changes.refresh')}
      title={t('changes.refresh')}
    >
      <Icon name="refresh-cw" size={15} />
    </button>
  </div>

  <div class="listwrap">
    {#if tab.loading && !tab.snapshot}
      <div class="skeletons" aria-busy="true" aria-label={t('changes.loading')}>
        {#each [70, 52, 64, 45, 58, 40] as w, i (i)}
          <div class="sk-row">
            <span class="skeleton" style:width="16px" style:height="16px"></span>
            <span class="skeleton" style:width="22px" style:height="22px"></span>
            <span class="skeleton" style:width="{w}%" style:height="12px"></span>
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
        multiselectable={false}
        activeDescendant={rowId(clamped)}
        tabindex={0}
        onkeydown={onKeydown}
      >
        {#snippet row(it, i)}
          {#if it.type === 'header'}
            <div
              class="row header"
              role="row"
              id={rowId(i)}
              aria-rowindex={i + 1}
              aria-expanded={!collapsed[it.section]}
              class:focused={i === clamped}
              style:height="{settings.headerHeight}px"
            >
              <div role="gridcell" class="cell head-cell">
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
                  <Icon name={collapsed[it.section] ? 'chevron-right' : 'chevron-down'} size={14} />
                </button>
                <span class="title">{t(SECTION_LABEL[it.section])}</span>
                <span class="chip">{it.count}</span>
                <span class="spacer"></span>
                {#if it.section === 'staged'}
                  <button type="button" class="btn btn-ghost btn-sm" tabindex="-1" onclick={() => tab.unstage(sectionEntries('staged'))}>
                    {t('changes.unstageAll')}
                  </button>
                {:else if it.section === 'unstaged' || it.section === 'untracked'}
                  <button type="button" class="btn btn-ghost btn-sm" tabindex="-1" onclick={() => tab.stage(it.section, sectionEntries(it.section))}>
                    {t('changes.stageAll')}
                  </button>
                {/if}
              </div>
            </div>
          {:else}
            {@const e = it.entry}
            {@const letter = displayStatus(e, it.section)}
            {@const key = `${it.section}:${pathKey(e.path)}`}
            {@const busy = tab.pending.has(key)}
            {@const isStagedSection = it.section === 'staged'}
            {@const canToggle = it.section === 'staged' || it.section === 'unstaged' || it.section === 'untracked'}
            <div
              class="row file"
              role="row"
              id={rowId(i)}
              aria-rowindex={i + 1}
              aria-selected={tab.isSelected(it.section, e)}
              aria-busy={busy}
              class:selected={tab.isSelected(it.section, e)}
              class:focused={i === clamped}
              class:busy
              style:height="{settings.rowHeight}px"
              onclick={() => activate(i)}
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
                      aria-label={isStagedSection
                        ? t('changes.unstageFile', { path: pathText(e.path) })
                        : t('changes.stageFile', { path: pathText(e.path) })}
                      onclick={(ev) => ev.stopPropagation()}
                      onchange={() => toggleEntry(it.section, e)}
                    />
                    <span class="box" aria-hidden="true"><Icon name="check" size={12} /></span>
                  </label>
                {:else}
                  <span class="cb-spacer"></span>
                {/if}
              </div>
              <div role="gridcell" class="cell status">
                <span class="st st-{statusKey(letter)}" title={statusName(letter)} aria-label={statusName(letter)}>{letter}</span>
              </div>
              <div role="gridcell" class="cell name">
                <span class="base">{pathBase(e.path)}</span>
                <span class="dir mono">
                  {#if e.originalPath}{pathText(e.originalPath)} → {/if}{pathDir(e.path) || t('changes.root')}
                </span>
              </div>
              <div role="gridcell" class="cell extra">
                {#if e.conflictKind}
                  <span class="chip conflict">{t(`conflict.${e.conflictKind}`)}</span>
                {/if}
                {#if it.section === 'unstaged' || it.section === 'untracked'}
                  <button
                    type="button"
                    class="btn btn-ghost btn-icon btn-sm discard"
                    tabindex="-1"
                    aria-label={t('changes.discardFile', { path: pathText(e.path) })}
                    title={t('changes.discard')}
                    disabled={busy}
                    onclick={(ev) => {
                      ev.stopPropagation();
                      void tab.discard(it.section, [e]);
                    }}
                  >
                    <Icon name="undo" size={14} />
                  </button>
                {/if}
              </div>
            </div>
          {/if}
        {/snippet}
      </VirtualList>
    {/if}
  </div>

  <p class="summary" aria-live="off">{tn('changes.summary', total)}</p>
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
    border-bottom: 1px solid var(--border);
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
    left: 10px;
    pointer-events: none;
  }
  .search .field {
    padding-left: 30px;
    min-height: 28px;
    font-size: var(--text-sm);
  }
  .switch {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
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
    width: 28px;
    height: 16px;
    border-radius: 8px;
    background: var(--bg-muted);
    border: 1px solid var(--border-strong);
    transition: background-color var(--dur) var(--ease);
  }
  .track::after {
    content: '';
    position: absolute;
    top: 1px;
    left: 1px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--text-muted);
    transition: transform var(--dur) var(--ease);
  }
  .switch input:checked + .track {
    background: var(--accent);
    border-color: var(--accent);
  }
  .switch input:checked + .track::after {
    transform: translateX(12px);
    background: var(--accent-fg);
  }
  .switch input:focus-visible + .track {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
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
    background: var(--bg-subtle);
    border-bottom: 1px solid var(--border);
    border-top: 1px solid var(--border);
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .head-cell {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
  }
  .head-cell .title {
    font-weight: 650;
  }
  .collapse {
    display: grid;
    place-items: center;
    width: var(--target-min);
    height: var(--target-min);
    margin-left: -6px;
    border: none;
    background: none;
    color: inherit;
    border-radius: var(--radius-sm);
  }
  .spacer {
    flex: 1;
  }
  .head-cell .btn {
    text-transform: none;
    letter-spacing: 0;
  }
  .row.file {
    border-bottom: 1px solid transparent;
    cursor: default;
  }
  .row.file:hover {
    background: var(--bg-subtle);
  }
  .row.file.selected {
    background: var(--bg-selected);
  }
  :global(.vl:focus-visible) .row.focused {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
    border-radius: var(--radius-sm);
  }
  .row.busy {
    opacity: 0.6;
  }
  .cell {
    display: flex;
    align-items: center;
  }
  .cell.name {
    flex: 1;
    min-width: 0;
    flex-direction: column;
    align-items: flex-start;
    justify-content: center;
    line-height: 1.25;
  }
  .base {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 550;
    font-size: var(--text-md);
  }
  .dir {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-subtle);
    font-size: 0.6875rem;
  }
  :global([data-density='compact']) .dir {
    display: none;
  }
  .cell.extra {
    gap: var(--space-1);
  }
  .discard {
    opacity: 0;
  }
  .row.file:hover .discard,
  :global(.vl:focus-visible) .row.focused .discard,
  .discard:focus-visible {
    opacity: 1;
  }
  .chip.conflict {
    background: var(--st-unmerged-bg);
    color: var(--st-unmerged-fg);
    font-weight: 500;
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
    width: 16px;
    height: 16px;
    border: 1.5px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--bg-surface);
    color: transparent;
    transition:
      background-color var(--dur) var(--ease),
      border-color var(--dur) var(--ease);
  }
  .cb input:checked + .box {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-fg);
  }
  .cb:hover .box {
    border-color: var(--accent);
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
    width: 22px;
    height: 22px;
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 0.6875rem;
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
    padding: var(--space-4) var(--space-3);
  }
  .sk-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .summary {
    padding: var(--space-1) var(--space-3);
    border-top: 1px solid var(--border);
    color: var(--text-subtle);
    font-size: var(--text-xs);
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
