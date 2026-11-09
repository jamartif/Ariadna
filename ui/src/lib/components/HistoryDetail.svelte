<script lang="ts">
  import { t, tn } from '$lib/i18n';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { historyOf } from '$lib/stores/history.svelte';
  import { toasts } from '$lib/stores/toasts.svelte';
  import { diffStats } from '$lib/util/diff';
  import { pathBase, pathDir, pathKey, pathText } from '$lib/util/gitpath';
  import Banner from './Banner.svelte';
  import CodeDiff from './CodeDiff.svelte';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';

  /** Detalle del commit seleccionado: cabecera, archivos y diff (columna derecha). */
  let { tab }: { tab: TabState } = $props();

  const h = $derived(historyOf(tab));
  const c = $derived(h.selected);
  const date = $derived(c ? new Date(c.authorDateIso) : null);

  const totals = $derived.by(() => {
    let add = 0;
    let del = 0;
    for (const f of h.files) {
      const s = diffStats(f);
      add += s.additions;
      del += s.deletions;
    }
    return { add, del };
  });

  async function copySha() {
    if (!c) return;
    try {
      await navigator.clipboard.writeText(c.sha);
      toasts.success(t('history.shaCopied'));
    } catch {
      toasts.info(c.sha);
    }
  }

  function fileKey(e: KeyboardEvent, i: number) {
    let n = i;
    if (e.key === 'ArrowDown') n = i + 1;
    else if (e.key === 'ArrowUp') n = i - 1;
    else return;
    e.preventDefault();
    h.selectFile(n);
    queueMicrotask(() => document.getElementById(`hfile-${h.fileIndex}`)?.focus());
  }
</script>

<section class="detail" aria-label={t('history.detail')}>
  {#if !c}
    <EmptyState icon="history" title={t('history.pickTitle')} body={t('history.pickBody')} />
  {:else}
    <header class="head">
      <h2 class="subject">{c.subject}</h2>
      {#if c.body.trim() !== ''}<pre class="body">{c.body.trim()}</pre>{/if}
      <dl class="meta">
        <div><dt>{t('history.author')}</dt><dd>{c.authorName} <span class="mail">&lt;{c.authorEmail}&gt;</span></dd></div>
        {#if date && !Number.isNaN(date.getTime())}
          <div><dt>{t('history.date')}</dt><dd><time datetime={c.authorDateIso}>{date.toLocaleString('es-ES', { dateStyle: 'long', timeStyle: 'short' })}</time></dd></div>
        {/if}
        <div>
          <dt>{t('history.commit')}</dt>
          <dd>
            <code class="mono">{c.sha}</code>
            <button type="button" class="btn btn-ghost btn-icon btn-sm" onclick={copySha} aria-label={t('history.copySha')} title={t('history.copySha')}>
              <Icon name="copy" size={13} />
            </button>
          </dd>
        </div>
        {#if c.parents.length > 0}
          <div><dt>{tn('history.parents', c.parents.length)}</dt><dd>{#each c.parents as p (p)}<code class="mono p">{p.slice(0, 7)}</code>{/each}</dd></div>
        {/if}
      </dl>
    </header>

    <div class="files-bar">
      <strong>{tn('history.files', h.files.length)}</strong>
      {#if h.files.length > 0}
        <span class="add">+{totals.add}</span><span class="del">−{totals.del}</span>
      {/if}
      {#if h.filesLoading}<Icon name="loader" size={13} spin />{/if}
    </div>

    {#if h.filesError}
      <div class="pad">
        <Banner kind="error" title={t('history.filesErrorTitle')} message={h.filesError.message} detail={h.filesError.detail}>
          {#snippet actions()}
            <button type="button" class="btn" onclick={() => c && h.select(c)}>{t('common.retry')}</button>
          {/snippet}
        </Banner>
      </div>
    {:else if h.files.length === 0 && !h.filesLoading}
      <EmptyState compact icon="file-diff" title={t('history.noFilesTitle')} body={t('history.noFilesBody')} />
    {:else}
      <ul class="files" role="listbox" aria-label={t('history.fileList')}>
        {#each h.files as f, i (pathKey(f.path) + ':' + i)}
          {@const s = diffStats(f)}
          <li>
            <button
              type="button"
              id="hfile-{i}"
              role="option"
              aria-selected={h.fileIndex === i}
              class="file"
              class:sel={h.fileIndex === i}
              onclick={() => h.selectFile(i)}
              onkeydown={(e) => fileKey(e, i)}
            >
              <Icon name="file" size={14} />
              <span class="name">{pathBase(f.path)}</span>
              {#if pathDir(f.path)}<span class="dir mono">{pathDir(f.path)}</span>{/if}
              {#if f.isNewFile}<span class="tag add">{t('diff.newFile')}</span>{/if}
              {#if f.isDeletedFile}<span class="tag del">{t('diff.deletedFile')}</span>{/if}
              <span class="spacer"></span>
              {#if !f.isBinary}<span class="add">+{s.additions}</span><span class="del">−{s.deletions}</span>{:else}<span class="bin">{t('history.binary')}</span>{/if}
            </button>
          </li>
        {/each}
      </ul>
      {#if h.file}
        <div class="path-head mono" title={pathText(h.file.path)}>{pathText(h.file.path)}</div>
        <div class="diff"><CodeDiff file={h.file} /></div>
      {/if}
    {/if}
  {/if}
</section>

<style>
  .detail {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg-surface);
  }
  .head {
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--border);
    max-height: 40%;
    overflow: auto;
  }
  .subject {
    margin: 0 0 var(--space-2);
    font-size: var(--text-md);
    line-height: 1.35;
    overflow-wrap: anywhere;
  }
  .body {
    margin: 0 0 var(--space-2);
    white-space: pre-wrap;
    font: inherit;
    font-size: var(--text-sm);
    color: var(--text-muted);
    overflow-wrap: anywhere;
  }
  .meta {
    display: grid;
    gap: 2px;
    margin: 0;
    font-size: var(--text-xs);
  }
  .meta > div {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    min-width: 0;
  }
  dt {
    flex: none;
    width: 64px;
    color: var(--text-subtle);
  }
  dd {
    margin: 0;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
    flex-wrap: wrap;
    color: var(--text);
  }
  .mail {
    color: var(--text-subtle);
  }
  .p + .p {
    margin-left: var(--space-2);
  }
  .files-bar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4);
    font-size: var(--text-xs);
    color: var(--text-muted);
    border-bottom: 1px solid var(--border);
  }
  .add {
    color: var(--success);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }
  .del {
    color: var(--danger);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }
  .files {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 30%;
    overflow: auto;
    border-bottom: 1px solid var(--border);
  }
  .file {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: 30px;
    padding: 0 var(--space-4);
    border: none;
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
  .name {
    font-weight: 600;
    white-space: nowrap;
  }
  .dir {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-subtle);
    font-size: var(--text-xs);
    min-width: 0;
  }
  .tag {
    font-size: var(--text-2xs);
    padding: 0 6px;
    border-radius: var(--pill);
    background: var(--bg-muted);
  }
  .tag.add {
    background: var(--success-soft);
  }
  .tag.del {
    background: var(--danger-soft);
  }
  .spacer {
    flex: 1;
  }
  .bin {
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
  .path-head {
    padding: var(--space-1) var(--space-4);
    font-size: var(--text-xs);
    color: var(--text-muted);
    background: var(--bg-subtle);
    border-bottom: 1px solid var(--border);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .diff {
    flex: 1;
    min-height: 0;
  }
  .pad {
    padding: var(--space-4);
  }
</style>
