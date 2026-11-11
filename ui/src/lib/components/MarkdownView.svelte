<script lang="ts">
  import { t } from '$lib/i18n';
  import { parseMarkdown, type Block, type Inline } from '$lib/util/markdown';

  /**
   * Markdown de PR pintado nodo a nodo desde el AST de `util/markdown.ts`. Nunca se usa
   * `{@html}`: el HTML en bruto sale como texto, las imágenes no se cargan y los enlaces son
   * siempre `https:`. Un clic en un enlace lo abre el navegador del sistema (`onopen`).
   */
  interface Props {
    source: string;
    onopen?: (url: string) => void;
    /** Texto cuando no hay contenido. */
    empty?: string;
  }
  let { source, onopen, empty }: Props = $props();

  const blocks = $derived(parseMarkdown(source));
  const HEADINGS = ['h3', 'h3', 'h4', 'h5', 'h6', 'h6'] as const;
</script>

{#snippet inl(nodes: Inline[])}
  {#each nodes as n, i (i)}
    {#if n.t === 'text'}{n.v}
    {:else if n.t === 'code'}<code class="mono">{n.v}</code>
    {:else if n.t === 'br'}<br />
    {:else if n.t === 'strong'}<strong>{@render inl(n.c)}</strong>
    {:else if n.t === 'em'}<em>{@render inl(n.c)}</em>
    {:else if n.t === 'del'}<del>{@render inl(n.c)}</del>
    {:else if n.t === 'link'}
      <a
        href={n.href}
        rel="noopener noreferrer"
        title={n.href}
        onclick={(e) => {
          e.preventDefault();
          onopen?.(n.href);
        }}>{@render inl(n.c)}</a>
    {/if}
  {/each}
{/snippet}

{#snippet blk(items: Block[])}
  {#each items as b, i (i)}
    {#if b.t === 'p'}
      <p>{@render inl(b.c)}</p>
    {:else if b.t === 'h'}
      <svelte:element this={HEADINGS[b.level - 1]} class="mdh">{@render inl(b.c)}</svelte:element>
    {:else if b.t === 'code'}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex (el bloque de código se desplaza con el teclado) -->
      <pre class="mono" role="region" tabindex="0" aria-label={t('pr.md.code')}><code>{b.v}</code></pre>
    {:else if b.t === 'quote'}
      <blockquote>{@render blk(b.c)}</blockquote>
    {:else if b.t === 'hr'}
      <hr />
    {:else if b.t === 'list'}
      {#if b.ordered}
        <ol start={b.start}>
          {#each b.items as it, k (k)}
            <li>{@render item(it)}</li>
          {/each}
        </ol>
      {:else}
        <ul>
          {#each b.items as it, k (k)}
            <li class:task={it.task !== null}>{@render item(it)}</li>
          {/each}
        </ul>
      {/if}
    {/if}
  {/each}
{/snippet}

{#snippet item(it: { task: boolean | null; c: Block[] })}
  {#if it.task !== null}
    <input type="checkbox" checked={it.task} disabled aria-label={it.task ? t('pr.md.done') : t('pr.md.todo')} />
  {/if}
  {@render blk(it.c)}
{/snippet}

<div class="md">
  {#if blocks.length === 0}
    {#if empty}<p class="none">{empty}</p>{/if}
  {:else}
    {@render blk(blocks)}
  {/if}
</div>

<style>
  .md {
    font-size: var(--text-md);
    line-height: 1.55;
    overflow-wrap: anywhere;
    min-width: 0;
  }
  .md :global(p) {
    margin: 0 0 var(--space-2);
  }
  .md :global(.mdh) {
    margin: var(--space-3) 0 var(--space-1);
    font-weight: 650;
    line-height: 1.3;
  }
  .md :global(h3.mdh) {
    font-size: var(--text-lg);
  }
  .md :global(h4.mdh) {
    font-size: var(--text-md);
  }
  .md :global(h5.mdh),
  .md :global(h6.mdh) {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .md :global(ul),
  .md :global(ol) {
    margin: 0 0 var(--space-2);
    padding-left: var(--space-5);
  }
  .md :global(li.task) {
    list-style: none;
    margin-left: calc(-1 * var(--space-4));
  }
  .md :global(li.task > input) {
    margin: 0 var(--space-2) 0 0;
    vertical-align: middle;
  }
  .md :global(li > p:last-child) {
    margin-bottom: 0;
  }
  .md :global(code) {
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    background: var(--bg-muted);
    font-size: 0.92em;
  }
  .md :global(pre) {
    margin: 0 0 var(--space-2);
    padding: var(--space-3);
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
    font-size: var(--text-sm);
    line-height: 1.45;
  }
  .md :global(pre code) {
    padding: 0;
    background: none;
    font-size: inherit;
  }
  .md :global(blockquote) {
    margin: 0 0 var(--space-2);
    padding: 0 0 0 var(--space-3);
    border-left: 3px solid var(--border-strong);
    color: var(--text-muted);
  }
  .md :global(hr) {
    border: 0;
    border-top: 1px solid var(--border);
    margin: var(--space-3) 0;
  }
  .md :global(a) {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
  }
  .none {
    color: var(--text-subtle);
    font-style: italic;
  }
</style>
