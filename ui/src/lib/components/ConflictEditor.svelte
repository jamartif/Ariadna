<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { t, tn } from '$lib/i18n';
  import type { ConflictFile } from '$lib/api/types';
  import { registerHandlers } from '$lib/shortcuts/registry';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { conflictsOf } from '$lib/stores/conflicts.svelte';
  import { dominantEol, locateLines, parseMarkers, splitLines, withEol, type HunkChoiceSide } from '$lib/util/conflictMarkers';
  import { pathText } from '$lib/util/gitpath';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';

  /** Columna derecha del estudio de conflictos: tres paneles (nuestro | base | suyo) + resultado. */
  let { tab }: { tab: TabState } = $props();

  const c = $derived(conflictsOf(tab));
  const f = $derived(c.selected);
  const mode = $derived(f ? c.mode(f) : null);
  const text = $derived(f ? c.text(f) : '');
  const parsed = $derived(parseMarkers(text));
  const pending = $derived(parsed.regions.length + (parsed.unterminated ? 1 : 0));
  const current = $derived(parsed.regions[c.currentHunk] ?? null);
  const into = $derived(tab.snapshot?.status.branchName ?? tab.info.branch ?? 'HEAD');
  const hasBase = $derived(f ? f.base.content !== null : false);

  /** Rango de líneas de cada bloque dentro del archivo completo de cada lado (para resaltar). */
  function ranges(content: string | null, pick: 'ours' | 'base' | 'theirs'): ({ start: number; end: number } | null)[] {
    if (content === null) return [];
    const out: ({ start: number; end: number } | null)[] = [];
    let from = 0;
    for (const r of parsed.regions) {
      const snippet = pick === 'ours' ? r.ours : pick === 'theirs' ? r.theirs : (r.base ?? '');
      const loc = locateLines(content, snippet, from);
      if (loc) from = loc.end;
      out.push(loc);
    }
    return out;
  }
  const marks = $derived({
    ours: ranges(f?.ours.content ?? null, 'ours'),
    base: ranges(f?.base.content ?? null, 'base'),
    theirs: ranges(f?.theirs.content ?? null, 'theirs')
  });

  let area = $state<HTMLTextAreaElement>();
  let root = $state<HTMLElement>();

  /** El `textarea` normaliza a `\n`: al escribir se restituye el final de línea del archivo. */
  function onInput(e: Event) {
    if (!f) return;
    const v = (e.currentTarget as HTMLTextAreaElement).value;
    c.setText(f, withEol(v, dominantEol(f.workingTree ?? '')));
  }

  /** Offset en el texto con `\n` que ve el `textarea` para un offset del texto original. */
  function lfOffset(full: string, off: number): number {
    let n = 0;
    for (let i = 0; i < off; i++) if (full[i] === '\r' && full[i + 1] === '\n') n++;
    return off - n;
  }

  function reveal() {
    const file = untrack(() => c.selected);
    if (!file) return;
    const full = untrack(() => c.text(file));
    const r = parseMarkers(full).regions[untrack(() => c.currentHunk)];
    if (area && r) {
      const lh = parseFloat(getComputedStyle(area).lineHeight) || 18;
      area.scrollTop = Math.max(0, r.line * lh - area.clientHeight / 3);
      if (document.activeElement === area) area.setSelectionRange(lfOffset(full, r.start), lfOffset(full, r.end));
    }
    root?.querySelectorAll<HTMLElement>('.ln.hl').forEach((el, i) => {
      if (i < 3) el.scrollIntoView?.({ block: 'nearest' });
    });
  }

  $effect(() => {
    void c.revealTick;
    void c.currentHunk;
    queueMicrotask(reveal);
  });

  function apply(side: HunkChoiceSide) {
    if (f && current) c.resolveHunk(f, side);
  }

  onMount(() =>
    registerHandlers({
      'conflicts.next': () => {
        if (!f || mode !== 'editor') return false;
        c.nextHunk(f);
      },
      'conflicts.prev': () => {
        if (!f || mode !== 'editor') return false;
        c.prevHunk(f);
      },
      'conflicts.ours': () => (current ? void apply('ours') : false),
      'conflicts.theirs': () => (current ? void apply('theirs') : false),
      'conflicts.both': () => (current ? void apply('both') : false)
    })
  );

  const lineCount = (s: string | null) => (s === null ? 0 : splitLines(s).length);
</script>

<section class="editor" data-scope="editor" bind:this={root} aria-label={t('conflicts.editorRegion')}>
  {#if !c.active}
    <EmptyState icon="git-merge" title={t('conflicts.noIntegrationTitle')} body={t('conflicts.noIntegrationBody')} />
  {:else if !f}
    {#if c.files.length === 0}
      <EmptyState icon="circle-check" tone="accent" title={t('conflicts.doneTitle')} body={t('conflicts.doneBody')} />
    {:else}
      <EmptyState icon="git-merge" title={t('conflicts.pickTitle')} body={t('conflicts.pickBody')} />
    {/if}
  {:else}
    <header class="bar">
      <div class="who">
        <h2 class="mono" title={pathText(f.path)}>{pathText(f.path)}</h2>
        <span class="chip kind">{t(`conflict.${f.kind}`)}</span>
      </div>

      {#if mode === 'editor'}
        <div class="nav" role="group" aria-label={t('conflicts.hunkOf', { i: pending === 0 ? 0 : c.currentHunk + 1, n: parsed.regions.length })}>
          <span class="count" class:ok={pending === 0} aria-live="polite" role="status">
            <Icon name={pending === 0 ? 'circle-check' : 'triangle-alert'} size={13} />
            {pending === 0 ? t('conflicts.allResolved') : tn('conflicts.pending', pending)}
          </span>
          <button type="button" class="btn btn-sm btn-icon" disabled={parsed.regions.length < 2} onclick={() => f && c.prevHunk(f)} title="{t('conflicts.prev')} (Alt+↑)" aria-label={t('conflicts.prev')}>
            <Icon name="arrow-up" size={14} />
          </button>
          <button type="button" class="btn btn-sm btn-icon" disabled={parsed.regions.length < 2} onclick={() => f && c.nextHunk(f)} title="{t('conflicts.next')} (Alt+↓)" aria-label={t('conflicts.next')}>
            <Icon name="arrow-down" size={14} />
          </button>
        </div>
      {/if}
    </header>

    {#if c.isStale(f)}
      <div class="stale" role="alert">
        <Icon name="triangle-alert" size={14} /><span>{t('conflicts.stale')}</span>
        <button type="button" class="btn btn-sm" onclick={() => f && c.keepDraft(f)}>{t('conflicts.staleKeep')}</button>
        <button type="button" class="btn btn-sm" onclick={() => f && c.discardDraft(f)}>{t('conflicts.staleReload')}</button>
      </div>
    {/if}

    {#if mode === 'editor'}
      <div class="panes" class:two={!hasBase}>
        {#each [{ key: 'ours', label: t('conflicts.ours'), sub: t('conflicts.oursSub', { name: into }), side: f.ours }, ...(hasBase ? [{ key: 'base', label: t('conflicts.base'), sub: '', side: f.base }] : []), { key: 'theirs', label: t('conflicts.theirs'), sub: t('conflicts.theirsSub', { name: c.sourceLabel ?? '…' }), side: f.theirs }] as p (p.key)}
          {@const key = p.key as 'ours' | 'base' | 'theirs'}
          {@const range = marks[key][c.currentHunk] ?? null}
          <div class="pane" data-side={key}>
            <div class="phead"><strong>{p.label}</strong>{#if p.sub}<span class="psub">{p.sub}</span>{/if}</div>
            <div class="code mono" role="textbox" aria-readonly="true" aria-multiline="true" tabindex="0" aria-label={t('conflicts.paneLabel', { side: p.label, path: pathText(f.path) })}>
              {#if p.side.content === null}
                <p class="absent">{t('conflicts.absent')}</p>
              {:else}
                {#each splitLines(p.side.content) as line, i (i)}
                  <div class="ln" class:hl={range !== null && i >= range.start && i < range.end}>
                    <span class="no" aria-hidden="true">{i + 1}</span><span class="tx">{line}</span>
                  </div>
                {/each}
              {/if}
            </div>
          </div>
        {/each}
      </div>

      <div class="result">
        <div class="rhead">
          <strong>{t('conflicts.result')}</strong>
          {#if current}
            <span class="hunkbar" role="group" aria-label={t('conflicts.hunkOf', { i: c.currentHunk + 1, n: parsed.regions.length })}>
              <span class="hn">{t('conflicts.hunkOf', { i: c.currentHunk + 1, n: parsed.regions.length })}</span>
              <button type="button" class="btn btn-sm" onclick={() => apply('ours')} title="Alt+1">{t('conflicts.acceptOurs')}</button>
              <button type="button" class="btn btn-sm" onclick={() => apply('theirs')} title="Alt+2">{t('conflicts.acceptTheirs')}</button>
              <button type="button" class="btn btn-sm" onclick={() => apply('both')} title="Alt+3">{t('conflicts.acceptBoth')}</button>
              {#if current.base !== null}
                <button type="button" class="btn btn-sm" onclick={() => apply('base')}>{t('conflicts.acceptBase')}</button>
              {/if}
            </span>
          {/if}
          <span class="grow"></span>
          <button type="button" class="btn btn-sm btn-ghost" disabled={pending === 0} onclick={() => f && c.resolveEverything(f, 'ours')}>{t('conflicts.allOurs')}</button>
          <button type="button" class="btn btn-sm btn-ghost" disabled={pending === 0} onclick={() => f && c.resolveEverything(f, 'theirs')}>{t('conflicts.allTheirs')}</button>
          {#if c.isDirty(f)}
            <button type="button" class="btn btn-sm btn-ghost" onclick={() => f && c.discardDraft(f)}>{t('conflicts.revert')}</button>
          {/if}
        </div>
        <textarea
          bind:this={area}
          class="area mono"
          wrap="off"
          spellcheck="false"
          autocapitalize="off"
          autocomplete="off"
          aria-label={t('conflicts.resultLabel', { path: pathText(f.path) })}
          aria-describedby="mark-hint"
          value={text}
          oninput={onInput}
        ></textarea>
      </div>

      <footer class="actions">
        <p id="mark-hint" class="hint" class:bad={pending > 0}>
          {#if pending > 0}<Icon name="circle-alert" size={12} />{t('conflicts.markersRemain')}{:else}{t('conflicts.markResolvedHint')}{/if}
        </p>
        <button type="button" class="btn btn-primary" disabled={!c.canMarkResolved(f)} onclick={() => f && c.markResolved(f)}>
          {#if c.busy === 'resolve'}<Icon name="loader" size={14} spin />{:else}<Icon name="check" size={14} />{/if}
          {t('conflicts.markResolved')}
        </button>
      </footer>
    {:else}
      {@const deleted = !f.ours.oid || !f.theirs.oid}
      <div class="sides">
        <p class="intro">
          <Icon name="info" size={14} />
          {c.isBinary(f) ? t('conflicts.binary') + '. ' : !c.isTextSafe(f) ? t('conflicts.notText') + ' ' : ''}{deleted ? t('conflicts.sidesIntroDeleted') : t('conflicts.sidesIntro')}
        </p>
        <div class="cards">
          {#each [{ key: 'ours' as const, label: t('conflicts.ours'), sub: t('conflicts.oursSub', { name: into }), side: f.ours, other: f.theirs }, { key: 'theirs' as const, label: t('conflicts.theirs'), sub: t('conflicts.theirsSub', { name: c.sourceLabel ?? '…' }), side: f.theirs, other: f.ours }] as p (p.key)}
            <article class="card" data-side={p.key}>
              <header><strong>{p.label}</strong><span class="psub">{p.sub}</span></header>
              <div class="cbody">
                {#if !p.side.oid}
                  <p class="absent"><Icon name="x" size={14} />{t('conflicts.absent')}</p>
                {:else if p.side.isBinary || p.side.content === null}
                  <p class="absent"><Icon name="file" size={14} />{t('conflicts.binary')}</p>
                {:else}
                  <p class="size">{tn('conflicts.lines', lineCount(p.side.content))}</p>
                  <div class="preview mono" role="textbox" aria-readonly="true" aria-multiline="true" tabindex="0" aria-label={t('conflicts.paneLabel', { side: p.label, path: pathText(f.path) })}>{p.side.content.split('\n').slice(0, 24).join('\n')}</div>
                {/if}
              </div>
              <button type="button" class="btn" disabled={c.busy !== null} onclick={() => f && c.acceptSide(f, p.key)}>
                {#if c.busy === 'resolve'}<Icon name="loader" size={14} spin />{:else}<Icon name="check" size={14} />{/if}
                {!p.side.oid ? t('conflicts.acceptDeletion') : p.key === 'ours' ? t('conflicts.keepOurs') : t('conflicts.keepTheirs')}
              </button>
            </article>
          {/each}
        </div>
      </div>
    {/if}
  {/if}
</section>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg-surface);
  }
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-4);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--border);
  }
  .who {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: var(--text-md);
    overflow-wrap: anywhere;
  }
  .chip.kind {
    background: var(--st-unmerged-bg);
    color: var(--st-unmerged-fg);
    flex: none;
  }
  .nav {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .count {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-right: var(--space-2);
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--warning);
  }
  .count.ok {
    color: var(--success);
  }
  .stale {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4);
    background: var(--warning-soft);
    border-bottom: 1px solid var(--warning);
    font-size: var(--text-sm);
  }
  .stale span {
    flex: 1;
  }
  .panes {
    flex: 0 0 38%;
    min-height: 120px;
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    border-bottom: 1px solid var(--border);
  }
  .panes.two {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .pane {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    border-right: 1px solid var(--border);
  }
  .pane:last-child {
    border-right: none;
  }
  .phead {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    padding: 4px var(--space-3);
    background: var(--bg-subtle);
    border-bottom: 1px solid var(--border);
    font-size: var(--text-xs);
  }
  .pane[data-side='ours'] .phead strong {
    color: var(--info);
  }
  .pane[data-side='theirs'] .phead strong {
    color: var(--success);
  }
  .psub {
    color: var(--text-subtle);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .code {
    flex: 1;
    min-height: 0;
    overflow: auto;
    font-size: var(--text-xs);
    line-height: var(--code-lh, 1.5);
  }
  .ln {
    display: flex;
    min-width: max-content;
  }
  .no {
    flex: none;
    width: 3.2em;
    padding-right: var(--space-2);
    text-align: right;
    color: var(--text-subtle);
    user-select: none;
  }
  .tx {
    white-space: pre;
    padding-right: var(--space-3);
  }
  .pane[data-side='ours'] .ln.hl {
    background: var(--info-soft);
  }
  .pane[data-side='theirs'] .ln.hl {
    background: var(--success-soft);
  }
  .pane[data-side='base'] .ln.hl {
    background: var(--warning-soft);
  }
  .absent {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: var(--space-3);
    font-size: var(--text-sm);
    color: var(--text-subtle);
  }
  .result {
    flex: 1;
    min-height: 140px;
    display: flex;
    flex-direction: column;
  }
  .rhead {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding: 4px var(--space-3);
    background: var(--bg-subtle);
    border-bottom: 1px solid var(--border);
    font-size: var(--text-xs);
  }
  .hunkbar {
    display: inline-flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .hn {
    margin: 0 var(--space-1);
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .grow {
    flex: 1;
  }
  .area {
    flex: 1;
    min-height: 0;
    width: 100%;
    resize: none;
    border: none;
    outline: none;
    padding: var(--space-2) var(--space-3);
    background: var(--bg-surface);
    color: var(--text);
    font-size: var(--text-xs);
    line-height: var(--code-lh, 1.5);
    white-space: pre;
    overflow: auto;
    tab-size: 4;
  }
  .area:focus-visible {
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-top: 1px solid var(--border);
    background: var(--bg-app);
  }
  .actions .btn {
    flex: none;
    white-space: nowrap;
  }
  .hint {
    display: flex;
    align-items: center;
    flex: 1 1 14rem;
    min-width: 0;
    gap: 6px;
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
  .hint.bad {
    color: var(--danger);
  }
  .sides {
    display: grid;
    gap: var(--space-3);
    padding: var(--space-4);
    overflow: auto;
    align-content: start;
  }
  .intro {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: var(--space-3);
  }
  .card {
    display: grid;
    gap: var(--space-2);
    align-content: start;
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
  }
  .card header {
    display: flex;
    gap: var(--space-2);
    align-items: baseline;
    font-size: var(--text-sm);
  }
  .card[data-side='ours'] strong {
    color: var(--info);
  }
  .card[data-side='theirs'] strong {
    color: var(--success);
  }
  .size {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
  .preview {
    margin: 0;
    max-height: 260px;
    overflow: auto;
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--bg-surface);
    font-size: var(--text-xs);
    white-space: pre;
  }

  /* Colores forzados: la selección no puede depender solo del color de fondo. */
  @media (forced-colors: active) {
    .ln.hl {
      outline: 1px solid Highlight;
      outline-offset: -1px;
    }
  }
</style>
