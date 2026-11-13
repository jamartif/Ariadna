<script lang="ts">
  import { intlLocale, t, tn } from '$lib/i18n';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { stashOf } from '$lib/stores/stash.svelte';
  import { pathBase, pathDir, pathText } from '$lib/util/gitpath';
  import Banner from './Banner.svelte';
  import CodeDiff from './CodeDiff.svelte';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';

  /** Detalle del stash seleccionado (columna derecha): acciones y diff de solo lectura. */
  let { tab }: { tab: TabState } = $props();

  const s = $derived(stashOf(tab));
  const e = $derived(s.selected);
  const files = $derived(s.diff ?? []);
  const file = $derived(s.selectedDiff);
  const busy = $derived(s.busy !== null);

  function when(iso: string): string {
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? '' : d.toLocaleString(intlLocale(), { dateStyle: 'long', timeStyle: 'short' });
  }
</script>

<section class="detail" aria-label={t('stash.detailRegion')}>
  {#if !e}
    <EmptyState icon="archive" title={t('stash.pickTitle')} body={t('stash.pickBody')} />
  {:else}
    <header class="head">
      <Icon name="archive" size={18} />
      <div class="titles">
        <h2 class:auto={e.autoMessage}>{e.message}</h2>
        <p class="sub">
          <span class="mono">stash@&#123;{e.index}&#125;</span>
          · {e.branch ? t('stash.onBranch', { branch: e.branch }) : t('stash.detached')}
          {#if when(e.createdAtIso)}· {when(e.createdAtIso)}{/if}
          {#if e.autoMessage}· {t('stash.auto')}{/if}
        </p>
        <p class="sub">{t('stash.commit')} <code class="mono">{e.oid.slice(0, 10)}</code></p>
      </div>
    </header>

    <div class="buttons">
      <button type="button" class="btn btn-primary" disabled={busy} title={t('stash.applyHint')} onclick={() => s.apply(e.oid)}>
        {#if s.busy === 'apply'}<Icon name="loader" size={14} spin />{:else}<Icon name="download" size={14} />{/if}
        {t('stash.apply')}
      </button>
      <button type="button" class="btn" disabled={busy} title={t('stash.popHint')} onclick={() => s.pop(e.oid)}>
        {#if s.busy === 'pop'}<Icon name="loader" size={14} spin />{:else}<Icon name="arrow-up" size={14} />{/if}
        {t('stash.pop')}
      </button>
      <button type="button" class="btn btn-danger" disabled={busy} title={t('stash.dropHint')} onclick={() => s.drop(e.oid)}>
        {#if s.busy === 'drop'}<Icon name="loader" size={14} spin />{:else}<Icon name="trash" size={14} />{/if}
        {t('stash.drop')}
      </button>
    </div>

    {#if s.diffError}
      <div class="pad">
        <Banner kind="error" title={t('stash.diffError')} message={s.diffError.message} detail={s.diffError.detail} />
      </div>
    {:else if s.diffLoading && !s.diff}
      <div class="sk" aria-busy="true" aria-label={t('diff.loading')}>
        {#each [90, 70, 82, 40] as w, i (i)}<span class="skeleton" style:width="{w}%" style:height="14px"></span>{/each}
      </div>
    {:else if files.length === 0}
      <EmptyState compact icon="file-diff" title={t('stash.noFiles')} body="" />
    {:else}
      <div class="files-head">
        <h3 id="stash-files-h">{t('stash.filesLabel')}</h3>
        <span class="chip">{tn('stash.files', files.length)}</span>
      </div>
      <ul class="files" aria-labelledby="stash-files-h">
        {#each files as f (pathText(f.path))}
          {@const on = file !== null && pathText(f.path) === pathText(file.path)}
          <li>
            <button type="button" class="file" class:sel={on} aria-pressed={on} onclick={() => (s.selectedFile = pathText(f.path))}>
              <Icon name="file" size={14} />
              <span class="name">{pathBase(f.path)}</span>
              {#if pathDir(f.path)}<span class="dir mono">{pathDir(f.path)}</span>{/if}
              {#if f.isNewFile}<span class="chip">{t('diff.newFile')}</span>{/if}
              {#if f.isDeletedFile}<span class="chip">{t('diff.deletedFile')}</span>{/if}
            </button>
          </li>
        {/each}
      </ul>
      {#if file}
        <div class="diff"><CodeDiff {file} /></div>
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
    display: flex;
    gap: var(--space-3);
    padding: var(--space-4);
    border-bottom: 1px solid var(--border);
    color: var(--text-muted);
  }
  .titles {
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: var(--text-lg);
    color: var(--text);
    overflow-wrap: anywhere;
  }
  h2.auto {
    font-weight: 500;
  }
  .sub {
    margin: 2px 0 0;
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--border);
  }
  .pad {
    padding: var(--space-4);
  }
  .sk {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
  }
  .files-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4) 0;
  }
  h3 {
    margin: 0;
    font-size: var(--text-xs);
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .files {
    list-style: none;
    margin: 0;
    padding: var(--space-1) var(--space-3);
    display: grid;
    gap: 2px;
    max-height: 30%;
    overflow: auto;
    border-bottom: 1px solid var(--border);
  }
  .file {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-1) var(--space-2);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }
  .file:hover {
    background: var(--bg-hover);
  }
  .file.sel {
    background: var(--bg-selected);
    border-color: var(--border-hover);
  }
  .file:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
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
  }
  .diff {
    flex: 1;
    min-height: 0;
  }
</style>
