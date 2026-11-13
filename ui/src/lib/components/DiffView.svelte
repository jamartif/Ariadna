<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import { registerHandlers } from '$lib/shortcuts/registry';
  import { toasts } from '$lib/stores/toasts.svelte';
  import { t, tn } from '$lib/i18n';
  import { toApiError, type ApiError, type DiffHunk, type FileDiff } from '$lib/api/types';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { settings } from '$lib/stores/settings.svelte';
  import { countLines, diffStats, flattenDiff, hunkHeader, isGiant, truncateLine, type DiffRow } from '$lib/util/diff';
  import { conflictsOf } from '$lib/stores/conflicts.svelte';
  import { pathBase, pathDir, pathKey, pathText } from '$lib/util/gitpath';
  import { buildOffsets, indexAt } from '$lib/util/virtual';
  import Banner from './Banner.svelte';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';
  import VirtualList from './VirtualList.svelte';

  let { tab }: { tab: TabState } = $props();

  const sel = $derived(tab.selected);
  const staged = $derived(sel?.section === 'staged');
  const loadable = $derived(sel !== null && sel.section !== 'conflicts' && sel.section !== 'ignored');

  let file = $state.raw<FileDiff | null>(null);
  let loadedFor = $state<string | null>(null);
  let loading = $state(false);
  let error = $state.raw<ApiError | null>(null);
  let forceShow = $state<Record<string, boolean>>({});
  let expandedLines = $state<Record<string, boolean>>({});
  let hoverHunk = $state<number | null>(null);
  let scrollTop = $state(0);
  let reqCounter = 0;

  const selKey = $derived(sel ? `${sel.section}:${sel.key}` : null);

  // Carga (o recarga tras cada snapshot) el diff del archivo seleccionado.
  $effect(() => {
    const s = sel;
    void tab.revision;
    if (!s || !loadable) {
      file = null;
      loadedFor = null;
      error = null;
      return;
    }
    const key = `${s.section}:${s.key}`;
    if (untrack(() => loadedFor) !== key) {
      file = null;
      scrollTop = 0;
    }
    const mine = ++reqCounter;
    loading = true;
    tab.api
      .diff(tab.id, s.section === 'staged' ? 'staged' : 'unstaged', [s.path])
      .then((res) => {
        if (mine !== reqCounter) return;
        file = res[0] ?? null;
        loadedFor = key;
        error = null;
      })
      .catch((e) => {
        if (mine !== reqCounter) return;
        error = toApiError(e);
        file = null;
      })
      .finally(() => {
        if (mine === reqCounter) loading = false;
      });
  });

  const giant = $derived(file ? isGiant(file) : false);
  const showContent = $derived(!!file && !file.isBinary && (!giant || (selKey !== null && forceShow[selKey] === true)));
  const rows = $derived<DiffRow[]>(file && showContent ? flattenDiff(file) : []);
  const stats = $derived(file ? diffStats(file) : { additions: 0, deletions: 0 });

  const lineH = $derived(settings.codeLineHeight);
  const hunkH = $derived(Math.round(lineH * 1.5) + 4);
  const heightOf = (r: DiffRow) => (r.type === 'hunk' ? hunkH : lineH);

  // Cabecera pegajosa: el hunk que contiene la fila superior visible.
  const offsets = $derived(buildOffsets(rows.length, (i) => heightOf(rows[i] as DiffRow)));
  const hunkStarts = $derived.by(() => {
    const starts: number[] = [];
    rows.forEach((r, i) => {
      if (r.type === 'hunk') starts[r.hunkIndex] = i;
    });
    return starts;
  });
  const topRow = $derived(rows.length ? (rows[indexAt(offsets, scrollTop)] ?? null) : null);
  const stickyHunk = $derived.by(() => {
    if (!topRow || !file) return null;
    const start = hunkStarts[topRow.hunkIndex];
    if (start === undefined || scrollTop <= (offsets[start] ?? 0)) return null;
    const h = file.hunks[topRow.hunkIndex];
    return h ? { hunk: h, index: topRow.hunkIndex } : null;
  });

  // ---- Líneas sueltas ------------------------------------------------------------------
  // Se elige dentro de UN hunk a la vez: al preparar líneas de un hunk, los siguientes cambian de
  // posición en el índice, así que no se mezclan hunks en una misma acción.

  const isChange = (k: string) => k === 'addition' || k === 'deletion';
  const canPick = $derived(
    !!file && !file.isBinary && sel !== null && (sel.section === 'staged' || sel.section === 'unstaged' || sel.section === 'untracked')
  );
  let linePick = $state.raw<{ hunk: number; lines: number[] } | null>(null);
  let anchor = $state.raw<{ hunk: number; line: number } | null>(null);
  let cursor = $state.raw<{ hunk: number; line: number } | null>(null);
  let listEl = $state<ReturnType<typeof VirtualList<DiffRow>>>();

  /** Líneas que cambian, en orden, con su fila en la lista virtual. */
  const changeable = $derived.by(() => {
    const out: { hunk: number; line: number; row: number }[] = [];
    rows.forEach((r, i) => {
      if (r.type === 'line' && isChange(r.line.kind)) out.push({ hunk: r.hunkIndex, line: r.lineIndex, row: i });
    });
    return out;
  });
  /** Línea con el tabindex 0 (roving): la última usada o la primera que cambia. */
  const activePick = $derived.by(() => {
    const c = cursor;
    if (c && changeable.some((x) => x.hunk === c.hunk && x.line === c.line)) return c;
    const first = changeable[0];
    return first ? { hunk: first.hunk, line: first.line } : null;
  });
  const pickedSet = $derived(new Set(linePick?.lines ?? []));
  const pickedCount = $derived(linePick?.lines.length ?? 0);

  // Un diff recién cargado (otro archivo o nuevo snapshot) invalida la selección: los índices cambian.
  $effect(() => {
    void file;
    untrack(() => {
      linePick = null;
      anchor = null;
      cursor = null;
    });
  });

  function pick(hunk: number, line: number, extend: boolean) {
    const h = file?.hunks[hunk];
    if (!canPick || !h || !isChange(h.lines[line]?.kind ?? '')) return;
    const cur = new Set(linePick && linePick.hunk === hunk ? linePick.lines : []);
    if (extend && anchor && anchor.hunk === hunk) {
      const [a, b] = anchor.line <= line ? [anchor.line, line] : [line, anchor.line];
      for (let i = a; i <= b; i++) if (isChange(h.lines[i]?.kind ?? '')) cur.add(i);
    } else if (cur.has(line)) cur.delete(line);
    else cur.add(line);
    anchor = { hunk, line };
    cursor = { hunk, line };
    linePick = cur.size > 0 ? { hunk, lines: [...cur].sort((x, y) => x - y) } : null;
  }

  function clearPick() {
    linePick = null;
    anchor = null;
    toasts.announce(t('diff.linesCleared'));
  }

  async function applyLines(): Promise<boolean> {
    const f = file;
    const p = linePick;
    const hunk = f && p ? f.hunks[p.hunk] : undefined;
    if (!f || !p || !hunk) return false;
    const ok = staged ? await tab.unstageLines(f.path, hunk, p.lines) : await tab.stageLines(f.path, hunk, p.lines);
    if (ok) {
      linePick = null;
      anchor = null;
    }
    return ok;
  }

  async function moveCursor(delta: number, extend: boolean) {
    const from = activePick;
    if (changeable.length === 0 || !from) return;
    const at = changeable.findIndex((x) => x.hunk === from.hunk && x.line === from.line);
    const next = changeable[Math.min(changeable.length - 1, Math.max(0, at + delta))];
    if (!next) return;
    if (extend) {
      if (!anchor || anchor.hunk !== from.hunk) anchor = { hunk: from.hunk, line: from.line };
      pick(next.hunk, next.line, next.hunk === anchor.hunk);
    } else {
      cursor = { hunk: next.hunk, line: next.line };
    }
    listEl?.scrollToIndex(next.row);
    await tick();
    document.getElementById(`dl-${next.hunk}-${next.line}`)?.focus();
  }

  function pickKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      void moveCursor(e.key === 'ArrowDown' ? 1 : -1, e.shiftKey);
    } else if (e.key === 'Escape' && linePick) {
      e.preventDefault();
      clearPick();
    } else if (e.key === 'Enter' && linePick) {
      e.preventDefault();
      void applyLines();
    }
  }

  /** Con el foco en la propia lista (no en una casilla), ↓/Intro llevan al cursor de líneas. */
  function regionKey(e: KeyboardEvent) {
    if (e.target !== e.currentTarget || !canPick) return;
    if (e.key === 'ArrowDown' || e.key === 'Enter') {
      e.preventDefault();
      void moveCursor(0, false);
    }
  }

  function onPickClick(e: MouseEvent, hunk: number, line: number) {
    e.stopPropagation();
    pick(hunk, line, e.shiftKey);
  }

  /** Clic en la fila (fuera del texto seleccionable): alterna la línea. Mayús+clic, rango. */
  function onRowClick(e: MouseEvent, hunk: number, line: number) {
    if (!canPick) return;
    const selected = typeof window !== 'undefined' ? window.getSelection()?.toString() ?? '' : '';
    if (selected !== '' && !e.shiftKey) return;
    pick(hunk, line, e.shiftKey);
  }

  onMount(() =>
    registerHandlers({
      'lines.apply': () => (linePick ? void applyLines() : false)
    })
  );

  function pickLabel(line: { kind: string; text: string; oldLineNumber: number | null; newLineNumber: number | null }): string {
    const text = line.text.replace(/\r$/, '');
    return t('diff.pickLine', {
      kind: line.kind === 'addition' ? t('diff.pickLineNew') : t('diff.pickLineOld'),
      n: line.newLineNumber ?? line.oldLineNumber ?? 0,
      text: text.length > 60 ? `${text.slice(0, 60)}…` : text
    });
  }

  function toggleHunk(h: DiffHunk) {
    if (!file || !sel) return;
    if (staged) void tab.unstageHunk(file.path, h);
    else void tab.stageHunk(file.path, h);
  }

  function marker(kind: string): string {
    return kind === 'addition' ? '+' : kind === 'deletion' ? '−' : ' ';
  }
  function kindName(kind: string): string {
    return kind === 'addition' ? t('diff.added') : kind === 'deletion' ? t('diff.removed') : '';
  }

  /** Lleva el archivo seleccionado al estudio de conflictos. */
  function openConflict() {
    const c = conflictsOf(tab);
    const f = c.files.find((x) => sel && pathKey(x.path) === sel.key);
    if (f) c.select(f);
    tab.panel = 'conflicts';
  }
</script>

{#snippet hunkBar(h: DiffHunk, index: number, total: number)}
  <span class="hunk-title mono">{hunkHeader(h)}</span>
  {#if h.heading}<span class="hunk-heading mono">{h.heading}</span>{/if}
  <span class="spacer"></span>
  <span class="bar-actions">
  {#if sel && (sel.section === 'staged' || sel.section === 'unstaged' || sel.section === 'untracked')}
    <button
      type="button"
      class="btn btn-sm hunk-btn"
      class:show={hoverHunk === index}
      aria-label={staged
        ? t('diff.unstageHunkOf', { n: index + 1, total, path: sel ? pathText(sel.path) : '' })
        : t('diff.stageHunkOf', { n: index + 1, total, path: sel ? pathText(sel.path) : '' })}
      onclick={() => toggleHunk(h)}
    >
      <Icon name={staged ? 'minus' : 'plus'} size={12} />
      {staged ? t('diff.unstageHunk') : t('diff.stageHunk')}
    </button>
  {/if}
  {#if canPick && linePick && linePick.hunk === index}
    <button type="button" class="btn btn-sm btn-primary lines-btn" onclick={applyLines}>
      <Icon name={staged ? 'minus' : 'plus'} size={12} />
      {staged ? t('diff.unstageLines', { n: pickedCount }) : t('diff.stageLines', { n: pickedCount })}
    </button>
    <button type="button" class="btn btn-ghost btn-icon btn-sm lines-btn" onclick={clearPick} aria-label={t('diff.clearLines')} title={t('diff.clearLines')}>
      <Icon name="x" size={12} />
    </button>
  {/if}
  </span>
{/snippet}

<section class="diff" aria-label={t('diff.region')}>
  {#if !sel}
    <EmptyState icon="file-diff" title={t('diff.noneTitle')} body={t('diff.noneBody')} />
  {:else if sel.section === 'conflicts'}
    <EmptyState icon="git-merge" tone="accent" title={t('diff.conflictTitle')} body={t('diff.conflictBody')}>
      <button type="button" class="btn btn-primary" onclick={openConflict}>
        <Icon name="git-merge" size={14} />{t('diff.conflictOpen')}
      </button>
    </EmptyState>
  {:else if sel.section === 'ignored'}
    <EmptyState icon="eye" title={t('diff.ignoredTitle')} body={t('diff.ignoredBody')} />
  {:else if error}
    <div class="pad">
      <Banner kind="error" title={t('diff.errorTitle')} message={error.message} detail={error.detail}>
        {#snippet actions()}
          <button type="button" class="btn" onclick={() => tab.refresh()}>{t('common.retry')}</button>
        {/snippet}
      </Banner>
    </div>
  {:else if !file}
    <div class="skeleton-wrap" aria-busy="true" aria-label={t('diff.loading')}>
      {#each [90, 70, 82, 40, 76, 64, 88, 55] as w, i (i)}
        <span class="skeleton" style:width="{w}%" style:height="14px"></span>
      {/each}
    </div>
  {:else}
    <header class="file-head">
      <Icon name="file" size={16} />
      <div class="file-name">
        <span class="b">{pathBase(file.path)}</span>
        {#if pathDir(file.path)}<span class="d mono">{pathDir(file.path)}</span>{/if}
      </div>
      <span class="chip scope" class:is-staged={staged}>{staged ? t('diff.scopeStaged') : t('diff.scopeUnstaged')}</span>
      {#if file.isNewFile}<span class="chip">{t('diff.newFile')}</span>{/if}
      {#if file.isDeletedFile}<span class="chip">{t('diff.deletedFile')}</span>{/if}
      <span class="spacer"></span>
      {#if loading}<span class="refreshing" title={t('diff.loading')}><Icon name="loader" size={14} spin /></span>{/if}
      {#if !file.isBinary}
        <span class="stat add" aria-label={tn('diff.additions', stats.additions)}>+{stats.additions}</span>
        <span class="stat del" aria-label={tn('diff.deletions', stats.deletions)}>−{stats.deletions}</span>
      {/if}
    </header>

    {#if file.isBinary}
      <EmptyState icon="file" compact title={t('diff.binaryTitle')} body={t('diff.binaryBody')} />
    {:else if giant && !showContent}
      <EmptyState icon="triangle-alert" tone="neutral" title={t('diff.giantTitle')} body={t('diff.giantBody', { lines: countLines(file).toLocaleString('es-ES') })}>
        <button
          type="button"
          class="btn btn-primary"
          onclick={() => selKey && (forceShow = { ...forceShow, [selKey]: true })}
        >
          {t('diff.loadGiant')}
        </button>
      </EmptyState>
    {:else if file.hunks.length === 0}
      <EmptyState icon="file-diff" compact title={t('diff.emptyTitle')} body={t('diff.emptyBody')} />
    {:else}
      <div class="code" style:--lh="{lineH}px" style:--pick-w={canPick ? '28px' : '0px'}>
        {#if canPick}
          <p id="diff-pick-hint" class="sr-only">{t('diff.pickHint')}</p>
          <p class="sr-only" role="status">{pickedCount > 0 ? tn('diff.linesSelected', pickedCount) : ''}</p>
        {/if}
        {#if stickyHunk}
          <div class="hunk-row sticky" style:height="{hunkH}px" role="presentation">
            {@render hunkBar(stickyHunk.hunk, stickyHunk.index, file.hunks.length)}
          </div>
        {/if}
        <VirtualList
          bind:this={listEl}
          items={rows}
          {heightOf}
          horizontal
          role="region"
          label={t('diff.codeLabel', { path: pathText(file.path) })}
          tabindex={0}
          overscan={12}
          onscroll={(top) => (scrollTop = top)}
          onkeydown={regionKey}
        >
          {#snippet row(r, _i)}
            {#if r.type === 'hunk'}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                class="hunk-row"
                style:height="{hunkH}px"
                onmouseenter={() => (hoverHunk = r.hunkIndex)}
                onmouseleave={() => (hoverHunk = null)}
              >
                {@render hunkBar(r.hunk, r.hunkIndex, file?.hunks.length ?? 0)}
              </div>
            {:else}
              {@const line = r.line}
              {@const tr = truncateLine(line.text, expandedLines[line.id] === true)}
              <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
              <div
                class="line {line.kind}"
                class:pickable={canPick && isChange(line.kind)}
                class:picked={linePick?.hunk === r.hunkIndex && pickedSet.has(r.lineIndex)}
                style:height="{lineH}px"
                onmouseenter={() => (hoverHunk = r.hunkIndex)}
                onmouseleave={() => (hoverHunk = null)}
                onclick={(e) => isChange(line.kind) && onRowClick(e, r.hunkIndex, r.lineIndex)}
              >
                {#if canPick}
                  <span class="pick">
                    {#if isChange(line.kind)}
                      <input
                        type="checkbox"
                        id="dl-{r.hunkIndex}-{r.lineIndex}"
                        class="pick-cb"
                        checked={linePick?.hunk === r.hunkIndex && pickedSet.has(r.lineIndex)}
                        tabindex={activePick && activePick.hunk === r.hunkIndex && activePick.line === r.lineIndex ? 0 : -1}
                        aria-label={pickLabel(line)}
                        aria-describedby="diff-pick-hint"
                        onclick={(e) => onPickClick(e, r.hunkIndex, r.lineIndex)}
                        onkeydown={pickKey}
                        onfocus={() => (cursor = { hunk: r.hunkIndex, line: r.lineIndex })}
                      />
                    {/if}
                  </span>
                {/if}
                <span class="gut old" aria-hidden="true">{line.oldLineNumber ?? ''}</span>
                <span class="gut new" aria-hidden="true">{line.newLineNumber ?? ''}</span>
                <span class="mark" aria-hidden="true">{marker(line.kind)}</span>
                {#if kindName(line.kind)}<span class="sr-only">{kindName(line.kind)}: </span>{/if}
                <span class="text mono">{tr.text}</span>
                {#if tr.truncated}
                  <button
                    type="button"
                    class="more btn btn-sm"
                    onclick={() => (expandedLines = { ...expandedLines, [line.id]: true })}
                  >
                    {t('diff.seeMore', { n: line.text.length.toLocaleString('es-ES') })}
                  </button>
                {/if}
              </div>
            {/if}
          {/snippet}
        </VirtualList>
      </div>
    {/if}
  {/if}
</section>

<style>
  .diff {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg-surface);
  }
  .pad {
    padding: var(--space-4);
  }
  .skeleton-wrap {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-5) var(--space-4);
  }
  .file-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4);
    min-height: 44px;
    border-bottom: 1px solid var(--border);
    color: var(--text-muted);
  }
  .file-name {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    min-width: 0;
    overflow: hidden;
  }
  .b {
    color: var(--text);
    font-weight: 650;
    white-space: nowrap;
  }
  .d {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-subtle);
    font-size: var(--text-xs);
  }
  .spacer {
    flex: 1;
  }
  .scope.is-staged {
    background: var(--success-soft);
    color: var(--success);
  }
  .scope:not(.is-staged) {
    background: var(--warning-soft);
    color: var(--warning);
  }
  .stat {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .stat.add {
    color: var(--success);
  }
  .stat.del {
    color: var(--danger);
  }
  .refreshing {
    display: inline-flex;
    color: var(--accent);
  }
  .code {
    position: relative;
    flex: 1;
    min-height: 0;
    font-family: var(--font-mono);
    font-size: 0.8125rem;
  }
  .hunk-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: 0 var(--space-3);
    background: var(--diff-hunk-bg);
    color: var(--text-muted);
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    font-size: var(--text-xs);
    position: relative;
    z-index: 1;
  }
  .hunk-row.sticky {
    position: absolute;
    top: 0;
    left: 0;
    right: 12px;
    z-index: 4;
    box-shadow: var(--elev-1);
  }
  .hunk-heading {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-subtle);
  }
  .hunk-btn {
    opacity: 0;
    transition: opacity var(--dur) var(--ease);
    font-family: var(--font-ui);
    flex: none;
  }
  .bar-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: none;
    position: sticky;
    right: var(--space-3);
  }
  .hunk-row:hover .hunk-btn,
  .hunk-row:focus-within .hunk-btn,
  .hunk-btn.show,
  .hunk-btn:focus-visible {
    opacity: 1;
  }
  @media (hover: none) {
    .hunk-btn {
      opacity: 1;
    }
  }
  .line {
    display: flex;
    align-items: stretch;
    width: 100%;
    line-height: var(--lh);
    white-space: pre;
    tab-size: 4;
    -webkit-user-select: text;
    user-select: text;
  }
  .line.addition {
    background: var(--diff-add-bg);
  }
  .line.deletion {
    background: var(--diff-del-bg);
  }
  .gut {
    position: sticky;
    flex: none;
    width: 52px;
    padding-right: var(--space-2);
    text-align: right;
    color: var(--text-muted);
    background: var(--bg-subtle);
    font-size: 0.75rem;
    font-variant-numeric: tabular-nums;
    -webkit-user-select: none;
    user-select: none;
    z-index: 2;
  }
  .gut.old {
    left: var(--pick-w, 0px);
  }
  .gut.new {
    left: calc(var(--pick-w, 0px) + 52px);
  }
  .pick {
    position: sticky;
    left: 0;
    flex: none;
    width: var(--pick-w, 28px);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-subtle);
    z-index: 3;
  }
  .addition .pick {
    background: var(--diff-add-gutter);
  }
  .deletion .pick {
    background: var(--diff-del-gutter);
  }
  .pick-cb {
    margin: 0;
    width: 14px;
    height: 14px;
    accent-color: var(--accent);
    cursor: pointer;
  }
  .pick-cb:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 1px;
  }
  .line.pickable {
    cursor: pointer;
  }
  .line.picked {
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .line.picked .text {
    background: var(--accent-soft);
  }
  .lines-btn {
    flex: none;
    font-family: var(--font-ui);
  }
  .addition .gut {
    background: var(--diff-add-gutter);
  }
  .deletion .gut {
    background: var(--diff-del-gutter);
  }
  .mark {
    flex: none;
    width: 20px;
    text-align: center;
    color: var(--text-muted);
    -webkit-user-select: none;
    user-select: none;
  }
  .addition .mark {
    color: var(--success);
  }
  .deletion .mark {
    color: var(--danger);
  }
  .text {
    flex: none;
    padding-right: var(--space-4);
  }
  .more {
    align-self: center;
    margin-right: var(--space-4);
    font-family: var(--font-ui);
    height: calc(var(--lh) - 2px);
    min-height: 0;
  }
  @media (forced-colors: active) {
    .line.addition .mark,
    .line.deletion .mark {
      forced-color-adjust: none;
      color: CanvasText;
    }
    .line.addition {
      border-left: 3px solid Highlight;
    }
    .line.deletion {
      border-left: 3px dashed GrayText;
    }
  }
</style>
