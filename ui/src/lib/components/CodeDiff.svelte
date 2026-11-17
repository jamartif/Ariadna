<script lang="ts">
  import { intlLocale, t, tn } from '$lib/i18n';
  import type { FileDiff } from '$lib/api/types';
  import { settings } from '$lib/stores/settings.svelte';
  import { countLines, diffStats, flattenDiff, hunkHeader, isGiant, splitDiff, truncateLine, type DiffRow, type SplitRow } from '$lib/util/diff';
  import { pathText } from '$lib/util/gitpath';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';
  import VirtualList from './VirtualList.svelte';

  /**
   * Diff de solo lectura (historial, pull requests): sin acciones de preparar. Con `onlineaction`
   * cada línea ofrece un botón para comentar (PR) y `marks` indica cuántos hilos hay en cada
   * línea (`old:N` / `new:N`). Sin esas propiedades el componente es el de siempre.
   */
  interface Props {
    file: FileDiff;
    onlineaction?: (a: { line: number; side: 'old' | 'new'; threads: number }) => void;
    marks?: ReadonlyMap<string, number>;
  }
  let { file, onlineaction, marks }: Props = $props();

  function anchorOf(line: { kind: string; oldLineNumber: number | null; newLineNumber: number | null }): { line: number; side: 'old' | 'new' } | null {
    if (line.kind === 'deletion' && line.oldLineNumber !== null) return { line: line.oldLineNumber, side: 'old' };
    if (line.newLineNumber !== null) return { line: line.newLineNumber, side: 'new' };
    return null;
  }

  let forceShow = $state(false);
  let expanded = $state<Record<string, boolean>>({});

  // Un archivo distinto reinicia el plegado.
  $effect(() => {
    void file;
    forceShow = false;
    expanded = {};
  });

  const giant = $derived(isGiant(file));
  const show = $derived(!file.isBinary && (!giant || forceShow));
  const rows = $derived<DiffRow[]>(show ? flattenDiff(file) : []);
  // En las PR la vista partida se desactiva: los comentarios se anclan a una línea concreta.
  const split = $derived(settings.diffLayout === 'split' && !onlineaction);
  const splitRows = $derived<SplitRow[]>(show && split ? splitDiff(file) : []);
  const stats = $derived(diffStats(file));
  const lineH = $derived(settings.codeLineHeight);
  const hunkH = $derived(Math.round(lineH * 1.5) + 4);
  const heightOf = (r: DiffRow) => (r.type === 'hunk' ? hunkH : lineH);
  const heightOfSplit = (r: SplitRow) => (r.type === 'hunk' ? hunkH : lineH);

  function marker(kind: string): string {
    return kind === 'addition' ? '+' : kind === 'deletion' ? '−' : ' ';
  }
  function kindName(kind: string): string {
    return kind === 'addition' ? t('diff.added') : kind === 'deletion' ? t('diff.removed') : '';
  }
</script>

{#if file.isBinary}
  <EmptyState icon="file" compact title={t('diff.binaryTitle')} body={t('history.binaryBody')} />
{:else if giant && !show}
  <EmptyState icon="triangle-alert" compact title={t('diff.giantTitle')} body={t('diff.giantBody', { lines: countLines(file).toLocaleString(intlLocale()) })}>
    <button type="button" class="btn btn-primary" onclick={() => (forceShow = true)}>{t('diff.loadGiant')}</button>
  </EmptyState>
{:else if file.hunks.length === 0}
  <EmptyState icon="file-diff" compact title={t('diff.emptyTitle')} body={t('diff.emptyBody')} />
{:else}
  <div class="code" style:--lh="{lineH}px">
    <span class="sr-only">{tn('diff.additions', stats.additions)}, {tn('diff.deletions', stats.deletions)}</span>
    {#if !onlineaction}
      <div class="layout" role="group" aria-label={t('diff.layout.label')}>
        <button type="button" class="btn btn-sm" class:on={!split} aria-pressed={!split} title={t('diff.layout.unifiedHint')} onclick={() => settings.setDiffLayout('unified')}>
          <Icon name="rows" size={13} />{t('diff.layout.unified')}
        </button>
        <button type="button" class="btn btn-sm" class:on={split} aria-pressed={split} title={t('diff.layout.splitHint')} onclick={() => settings.setDiffLayout('split')}>
          <Icon name="columns" size={13} />{t('diff.layout.split')}
        </button>
      </div>
    {/if}
    {#if split}
      <VirtualList items={splitRows} heightOf={heightOfSplit} horizontal role="region" label={t('diff.codeLabel', { path: pathText(file.path) })} tabindex={0} overscan={12}>
        {#snippet row(r)}
          {#if r.type === 'hunk'}
            <div class="hunk-row" style:height="{hunkH}px">
              <span class="mono">{hunkHeader(r.hunk)}</span>
              {#if r.hunk.heading}<span class="hunk-heading mono">{r.hunk.heading}</span>{/if}
            </div>
          {:else}
            {@const izq = r.left}
            {@const der = r.right}
            <div class="srow" style:height="{lineH}px">
              <span class="snum" aria-hidden="true">{izq?.oldLineNumber ?? ''}</span>
              <span class="scell mono {izq?.kind ?? 'void'}" title={izq?.text ?? ''}>
                {#if izq}{#if kindName(izq.kind)}<span class="sr-only">{kindName(izq.kind)}: </span>{/if}{truncateLine(izq.text, expanded[izq.id] === true).text}{/if}
              </span>
              <span class="snum sep" aria-hidden="true">{der?.newLineNumber ?? ''}</span>
              <span class="scell mono {der?.kind ?? 'void'}" title={der?.text ?? ''}>
                {#if der}{#if kindName(der.kind)}<span class="sr-only">{kindName(der.kind)}: </span>{/if}{truncateLine(der.text, expanded[der.id] === true).text}{/if}
              </span>
            </div>
          {/if}
        {/snippet}
      </VirtualList>
    {:else}
    <VirtualList items={rows} {heightOf} horizontal role="region" label={t('diff.codeLabel', { path: pathText(file.path) })} tabindex={0} overscan={12}>
      {#snippet row(r, _i)}
        {#if r.type === 'hunk'}
          <div class="hunk-row" style:height="{hunkH}px">
            <span class="mono">{hunkHeader(r.hunk)}</span>
            {#if r.hunk.heading}<span class="hunk-heading mono">{r.hunk.heading}</span>{/if}
          </div>
        {:else}
          {@const line = r.line}
          {@const tr = truncateLine(line.text, expanded[line.id] === true)}
          <div class="line {line.kind}" style:height="{lineH}px">
            <span class="gut old" aria-hidden="true">{line.oldLineNumber ?? ''}</span>
            <span class="gut new" aria-hidden="true">{line.newLineNumber ?? ''}</span>
            <span class="mark" aria-hidden="true">{marker(line.kind)}</span>
            {#if onlineaction}
              {@const anchor = anchorOf(line)}
              {@const count = anchor ? (marks?.get(`${anchor.side}:${anchor.line}`) ?? 0) : 0}
              {#if anchor}
                <button
                  type="button"
                  class="cbtn"
                  class:has={count > 0}
                  tabindex="-1"
                  aria-label={count > 0 ? tn('pr.diff.threadsOnLine', count, { line: anchor.line }) : t('pr.diff.commentLine', { line: anchor.line })}
                  onclick={() => onlineaction({ ...anchor, threads: count })}
                >{count > 0 ? count : '+'}</button>
              {:else}
                <span class="cbtn-gap"></span>
              {/if}
            {/if}
            {#if kindName(line.kind)}<span class="sr-only">{kindName(line.kind)}: </span>{/if}
            <span class="text mono">{tr.text}</span>
            {#if tr.truncated}
              <button type="button" class="more btn btn-sm" onclick={() => (expanded = { ...expanded, [line.id]: true })}>
                {t('diff.seeMore', { n: line.text.length.toLocaleString(intlLocale()) })}
              </button>
            {/if}
          </div>
        {/if}
      {/snippet}
    </VirtualList>
    {/if}
  </div>
{/if}

<style>
  .code {
    position: relative;
    height: 100%;
    min-height: 0;
    font-family: var(--font-mono);
    font-size: var(--diff-font-size, 13px);
  }
  /* Flota sobre el código: así no le quita altura a la lista virtualizada. */
  .layout {
    position: absolute;
    top: var(--space-2);
    right: var(--space-2);
    z-index: 4;
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-subtle);
  }
  .layout .btn {
    gap: 4px;
    color: var(--text-muted);
  }
  .layout .btn.on {
    background: var(--bg-surface);
    color: var(--text);
    box-shadow: var(--elev-1);
  }
  .srow {
    display: grid;
    grid-template-columns: 52px minmax(0, 1fr) 52px minmax(0, 1fr);
    align-items: stretch;
    width: 100%;
    line-height: var(--lh);
    white-space: pre;
    tab-size: 4;
    -webkit-user-select: text;
    user-select: text;
  }
  .snum {
    padding-right: var(--space-2);
    text-align: right;
    color: var(--text-muted);
    background: var(--bg-subtle);
    font-size: 0.75rem;
    font-variant-numeric: tabular-nums;
    -webkit-user-select: none;
    user-select: none;
  }
  .snum.sep {
    border-left: 1px solid var(--border);
  }
  .scell {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    padding-left: var(--space-2);
  }
  .scell.addition {
    background: var(--diff-add-bg);
  }
  .scell.deletion {
    background: var(--diff-del-bg);
  }
  .scell.void {
    background: var(--bg-subtle);
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
  }
  .hunk-heading {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-subtle);
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
    left: 0;
  }
  .gut.new {
    left: 52px;
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
  .cbtn,
  .cbtn-gap {
    flex: none;
    width: 22px;
    margin: 0 2px;
  }
  .cbtn {
    align-self: center;
    height: calc(var(--lh) - 4px);
    min-height: 0;
    min-width: 0;
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--accent);
    font-family: var(--font-ui);
    font-size: 0.75rem;
    font-weight: 700;
    line-height: 1;
    opacity: 0;
  }
  .line:hover .cbtn,
  .cbtn:focus-visible,
  .cbtn.has {
    opacity: 1;
  }
  .cbtn.has {
    background: var(--accent-soft);
    border-color: var(--accent);
  }
  .cbtn:hover {
    background: var(--accent-soft);
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
