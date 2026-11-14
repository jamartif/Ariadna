<script lang="ts">
  import { t, tn } from '$lib/i18n';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { branchesOf } from '$lib/stores/branches.svelte';
  import { pathBase, pathDir, pathKey, pathText } from '$lib/util/gitpath';
  import Banner from './Banner.svelte';
  import CodeDiff from './CodeDiff.svelte';
  import Icon from './Icon.svelte';

  /**
   * Diff de una rama contra otra (`git diff base...head`) dentro del detalle de rama: lista de
   * archivos y visor de solo lectura del elegido. Lo llenan las acciones «Ver el diff contra la
   * actual» y «Comparar con…» del menú contextual del riel (`branchesOf(tab).compare`).
   */
  interface Props {
    tab: TabState;
    /** Rama de la que se muestra el diff (la «head»). */
    head: string;
  }
  let { tab, head }: Props = $props();

  const b = $derived(branchesOf(tab));
  const d = $derived(b.diff);
  const files = $derived(b.diff?.head === head ? (b.diff?.files ?? []) : []);
  const base = $derived(b.diff?.base ?? 'HEAD');

  let sel = $state<string | null>(null);
  // La primera vez (o si el archivo elegido desaparece) se selecciona el primero.
  $effect(() => {
    const list = files;
    if (list.length === 0) return;
    if (sel !== null && list.some((f) => pathKey(f.path) === sel)) return;
    const first = list[0];
    if (first) sel = pathKey(first.path);
  });
  const current = $derived(files.find((f) => pathKey(f.path) === sel) ?? files[0] ?? null);
</script>

<section class="bdiff" aria-label={t('rail.menu.diffTitle', { head, base })} data-testid="branch-diff">
  <header class="bh">
    <Icon name="file-diff" size={14} />
    <h3>{t('rail.menu.diffTitle', { head, base })}</h3>
    <span class="chip num">{tn('history.files', files.length)}</span>
    <span class="grow"></span>
    <button
      type="button"
      class="btn btn-ghost btn-icon btn-sm"
      aria-label={t('rail.menu.diffClose')}
      title={t('rail.menu.diffClose')}
      data-testid="branch-diff-close"
      onclick={() => b.clearDiff()}
    >
      <Icon name="x" size={12} />
    </button>
  </header>

  {#if b.diffError}
    <div class="pad">
      <Banner kind="error" title={t('rail.menu.diffTitle', { head, base })} message={b.diffError.message} detail={b.diffError.detail} />
    </div>
  {:else if files.length === 0}
    <p class="none" data-testid="branch-diff-empty"><Icon name="circle-check" size={13} />{t('rail.menu.compareEmpty')}</p>
  {:else}
    <ul class="bfiles" aria-label={t('rail.menu.diffTitle', { head, base })}>
      {#each files as f (pathKey(f.path))}
        {@const on = current !== null && pathKey(f.path) === pathKey(current.path)}
        <li>
          <button type="button" class="bfile" class:sel={on} aria-pressed={on} onclick={() => (sel = pathKey(f.path))}>
            <Icon name="file" size={13} />
            <span class="name">{pathBase(f.path)}</span>
            {#if pathDir(f.path)}<span class="dir mono">{pathDir(f.path)}</span>{/if}
            {#if f.isBinary}<span class="chip">{t('history.binary')}</span>{/if}
            {#if f.isNewFile}<span class="chip">{t('diff.newFile')}</span>{/if}
            {#if f.isDeletedFile}<span class="chip">{t('diff.deletedFile')}</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
    {#if current}
      <div class="diff"><CodeDiff file={current} /></div>
    {/if}
  {/if}
</section>

<style>
  .bdiff {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-surface);
    overflow: hidden;
  }
  .bh {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--border);
    color: var(--text-muted);
  }
  .bh :global(svg) {
    flex: none;
    color: var(--accent);
  }
  h3 {
    margin: 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-xs);
    font-weight: 650;
    color: var(--text);
  }
  .grow {
    flex: 1;
  }
  .pad {
    padding: var(--space-3);
  }
  .none {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    padding: var(--space-3);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .bfiles {
    list-style: none;
    margin: 0;
    padding: var(--space-1);
    display: grid;
    gap: 1px;
    max-height: 40%;
    overflow: auto;
    border-bottom: 1px solid var(--border);
  }
  .bfile {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    width: 100%;
    padding: 2px var(--space-2);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text);
    text-align: left;
    font-size: var(--text-xs);
  }
  .bfile :global(svg) {
    flex: none;
    color: var(--text-subtle);
  }
  .bfile:hover {
    background: var(--bg-hover);
  }
  .bfile.sel {
    background: var(--bg-selected);
    border-color: var(--border-hover);
  }
  .bfile:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
  }
  .name {
    font-weight: 600;
    white-space: nowrap;
  }
  .dir {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-subtle);
  }
  .diff {
    flex: 1;
    min-height: 160px;
  }
</style>
