<script lang="ts">
  import { t, tn } from '$lib/i18n';
  import type { PrReviewThread } from '$lib/api/prTypes';
  import type { PullRequestsController } from '$lib/stores/pullRequests.svelte';
  import { anchorKey, FILE_STATUS_LABEL, FILE_STATUS_LETTER } from '$lib/util/pr';
  import { pathText } from '$lib/util/gitpath';
  import CodeDiff from './CodeDiff.svelte';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';
  import PrComposer from './PrComposer.svelte';
  import PrError from './PrError.svelte';
  import PrThread from './PrThread.svelte';

  /** Pestaña «Archivos»: lista de archivos, su diff (visor de solo lectura) y los hilos en línea. */
  let { pr }: { pr: PullRequestsController } = $props();

  let picked = $state<string | null>(null);
  let compose = $state<{ line: number; side: 'old' | 'new'; n: number } | null>(null);
  let highlighted = $state<string | null>(null);

  const files = $derived(pr.diff?.files ?? pr.detail?.files ?? []);
  const path = $derived(files.some((f) => f.path === picked) ? picked : (files[0]?.path ?? null));
  const file = $derived(files.find((f) => f.path === path) ?? null);
  const diff = $derived(pr.diff?.diffs.find((d) => pathText(d.path) === path) ?? null);
  const omitted = $derived(pr.diff?.omitted.find((o) => o.path === path) ?? null);

  const inline = $derived(pr.threads.filter((th) => th.anchor !== null));
  const here = $derived(inline.filter((th) => th.anchor?.path === path).sort((a, b) => (a.anchor?.line ?? 0) - (b.anchor?.line ?? 0)));
  const marks = $derived.by(() => {
    const m = new Map<string, number>();
    for (const th of here) if (th.anchor) m.set(anchorKey(th.anchor), (m.get(anchorKey(th.anchor)) ?? 0) + 1);
    return m as ReadonlyMap<string, number>;
  });
  const perFile = $derived.by(() => {
    const m = new Map<string, number>();
    for (const th of inline) if (th.anchor && th.status === 'active') m.set(th.anchor.path, (m.get(th.anchor.path) ?? 0) + 1);
    return m;
  });

  // Llegada desde la pestaña de conversación («Ver en archivos»).
  $effect(() => {
    const f = pr.focus;
    if (!f) return;
    picked = f.path;
    highlight(f.threadId);
    pr.focus = null;
  });

  function highlight(id: string) {
    highlighted = id;
    queueMicrotask(() => document.getElementById(`pr-thread-${id}`)?.scrollIntoView?.({ block: 'nearest' }));
  }

  function lineAction(a: { line: number; side: 'old' | 'new'; threads: number }) {
    const existing = here.find((th) => th.anchor && th.anchor.line === a.line && th.anchor.side === a.side);
    if (existing && a.threads > 0) {
      compose = null;
      highlight(existing.id);
    } else {
      highlighted = null;
      compose = { line: a.line, side: a.side, n: (compose?.n ?? 0) + 1 };
    }
  }

  function startManual() {
    compose = { line: compose?.line ?? 1, side: compose?.side ?? 'new', n: (compose?.n ?? 0) + 1 };
  }

  function fileKey(e: KeyboardEvent) {
    const rows = [...(e.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>('button.file')];
    const i = rows.indexOf(document.activeElement as HTMLButtonElement);
    let n = i;
    if (e.key === 'ArrowDown') n = Math.min(rows.length - 1, i + 1);
    else if (e.key === 'ArrowUp') n = Math.max(0, i - 1);
    else if (e.key === 'Home') n = 0;
    else if (e.key === 'End') n = rows.length - 1;
    else return;
    e.preventDefault();
    rows[n]?.focus();
  }

  function pick(p: string) {
    picked = p;
    compose = null;
    highlighted = null;
  }

  const threadLabel = (th: PrReviewThread) => th.id;
</script>

<div class="files">
  {#if pr.diffError}
    <div class="pad"><PrError error={pr.diffError} onretry={() => pr.selectedId !== null && pr.loadDiff(pr.selectedId)} /></div>
  {:else if pr.diffLoading && !pr.diff}
    <div class="loading" role="status" aria-busy="true"><Icon name="loader" size={16} spin />{t('pr.files.loading')}</div>
  {:else if files.length === 0}
    <EmptyState compact icon="file-diff" title={t('pr.files.emptyTitle')} body={t('pr.files.emptyBody')} />
  {:else}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <ul class="filelist" aria-label={t('pr.files.list')} onkeydown={fileKey}>
      {#each files as f (f.path)}
        {@const n = perFile.get(f.path) ?? 0}
        <li>
          <button type="button" class="file" class:sel={f.path === path} aria-current={f.path === path ? 'true' : undefined} onclick={() => pick(f.path)}>
            <span class="st mono" data-status={f.status} title={t(FILE_STATUS_LABEL[f.status])}>{FILE_STATUS_LETTER[f.status]}<span class="sr-only">{t(FILE_STATUS_LABEL[f.status])}</span></span>
            <span class="fp mono" title={f.oldPath ? `${f.oldPath} → ${f.path}` : f.path}>{f.path}</span>
            {#if f.additions !== null || f.deletions !== null}
              <span class="stat mono"><span class="add">+{f.additions ?? 0}</span> <span class="del">−{f.deletions ?? 0}</span></span>
            {/if}
            {#if n > 0}<span class="chip" title={tn('pr.files.threads', n)}><Icon name="info" size={10} />{n}<span class="sr-only"> {tn('pr.files.threads', n)}</span></span>{/if}
          </button>
        </li>
      {/each}
    </ul>
    {#if pr.diff && pr.diff.omitted.length > 0}
      <p class="omitted"><Icon name="info" size={12} />{tn('pr.files.omitted', pr.diff.omitted.length)}</p>
    {/if}

    <div class="bar">
      <h3 class="cur mono" title={path ?? ''}>{path}</h3>
      {#if file}<span class="badge">{t(FILE_STATUS_LABEL[file.status])}</span>{/if}
      <span class="grow"></span>
      <button type="button" class="btn btn-sm" onclick={startManual} disabled={path === null || pr.busyAny}><Icon name="plus" size={12} />{t('pr.files.comment')}</button>
    </div>

    <div class="diffbox">
      {#if diff}
        <CodeDiff file={diff} onlineaction={lineAction} {marks} />
      {:else if omitted}
        <EmptyState compact icon="file" title={t('pr.files.omittedTitle')} body={omitted.reason} />
      {:else if file?.binary}
        <EmptyState compact icon="file" title={t('diff.binaryTitle')} body={t('history.binaryBody')} />
      {:else}
        <EmptyState compact icon="file-diff" title={t('pr.files.noDiff')} body={t('pr.files.noDiffBody')} />
      {/if}
    </div>

    <div class="threads" role="region" aria-label={t('pr.files.threadsRegion')}>
      {#if compose && path}
        {#key compose.n}
          <PrComposer {pr} {path} line={compose.line} side={compose.side} onclose={() => (compose = null)} />
        {/key}
      {/if}
      {#each here as th (threadLabel(th))}
        <PrThread thread={th} {pr} highlighted={highlighted === th.id} showAnchor={false} />
      {:else}
        {#if !compose}<p class="none">{t('pr.files.noThreads')}</p>{/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .files {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .pad {
    padding: var(--space-3);
  }
  .loading {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-4);
    color: var(--text-muted);
    font-size: var(--text-sm);
  }
  .filelist {
    flex: none;
    max-height: 28%;
    overflow: auto;
    margin: 0;
    padding: var(--space-1) var(--space-2);
    list-style: none;
    border-bottom: 1px solid var(--border);
  }
  .file {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: 28px;
    padding: 0 var(--space-2);
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text);
    text-align: left;
    font-size: var(--text-sm);
  }
  .file:hover {
    background: var(--bg-hover);
  }
  .file.sel {
    background: var(--bg-selected);
  }
  .st {
    flex: none;
    width: 18px;
    height: 18px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    font-size: 10px;
    font-weight: 700;
    background: var(--st-modified-bg);
    color: var(--st-modified-fg);
  }
  .st[data-status='added'] {
    background: var(--st-added-bg);
    color: var(--st-added-fg);
  }
  .st[data-status='deleted'] {
    background: var(--st-deleted-bg);
    color: var(--st-deleted-fg);
  }
  .st[data-status='renamed'],
  .st[data-status='copied'] {
    background: var(--st-renamed-bg);
    color: var(--st-renamed-fg);
  }
  .fp {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .stat {
    flex: none;
    font-size: var(--text-xs);
  }
  .add {
    color: var(--success);
  }
  .del {
    color: var(--danger);
  }
  .omitted {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    padding: var(--space-1) var(--space-3);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--border);
    background: var(--bg-subtle);
  }
  .cur {
    margin: 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-sm);
    font-weight: 600;
  }
  .badge {
    flex: none;
    padding: 0 7px;
    height: 18px;
    line-height: 18px;
    border-radius: var(--pill);
    background: var(--bg-muted);
    color: var(--text-muted);
    font-size: var(--text-2xs);
    font-weight: 650;
  }
  .grow {
    flex: 1;
  }
  .diffbox {
    flex: 1 1 240px;
    min-height: 160px;
    overflow: hidden;
  }
  .threads {
    flex: 0 1 auto;
    max-height: 42%;
    min-height: 56px;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-top: 1px solid var(--border);
    background: var(--bg-app);
  }
  .none {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-subtle);
  }
</style>
