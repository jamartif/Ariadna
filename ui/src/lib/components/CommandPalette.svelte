<script lang="ts">
  import { t, tn } from '$lib/i18n';
  import type { PaletteAction } from '$lib/stores/commands';
  import { fuzzyFilter } from '$lib/util/fuzzy';
  import { focusTrap } from '$lib/util/focusTrap';
  import Icon from './Icon.svelte';

  let { actions, onclose }: { actions: PaletteAction[]; onclose: () => void } = $props();

  let query = $state('');
  let active = $state(0);
  let listEl = $state<HTMLUListElement>();

  const results = $derived(fuzzyFilter(query, actions, (a) => `${a.group} ${a.label}`));
  const idx = $derived(Math.min(active, Math.max(0, results.length - 1)));

  $effect(() => {
    void query;
    active = 0;
  });

  $effect(() => {
    const el = listEl?.querySelector<HTMLElement>(`[data-index="${idx}"]`);
    el?.scrollIntoView?.({ block: 'nearest' });
  });

  function run(a: PaletteAction | undefined) {
    if (!a) return;
    onclose();
    // Tras cerrar, el foco vuelve al origen y la acción se ejecuta.
    queueMicrotask(() => a.run());
  }

  function key(e: KeyboardEvent) {
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        active = Math.min(results.length - 1, idx + 1);
        break;
      case 'ArrowUp':
        e.preventDefault();
        active = Math.max(0, idx - 1);
        break;
      case 'Enter':
        e.preventDefault();
        run(results[idx]?.item);
        break;
      case 'Escape':
        e.preventDefault();
        e.stopPropagation();
        onclose();
        break;
    }
  }

  /** Divide la etiqueta en trozos resaltados según las posiciones del acierto. */
  function parts(a: PaletteAction, positions: number[]): { text: string; hit: boolean }[] {
    const offset = a.group.length + 1;
    const set = new Set(positions.filter((p) => p >= offset).map((p) => p - offset));
    const out: { text: string; hit: boolean }[] = [];
    [...a.label].forEach((ch, i) => {
      const hit = set.has(i);
      const last = out[out.length - 1];
      if (last && last.hit === hit) last.text += ch;
      else out.push({ text: ch, hit });
    });
    return out;
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose}>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label={t('palette.title')}
    tabindex="-1"
    use:focusTrap
    onclick={(e) => e.stopPropagation()}
    onkeydown={key}
  >
    <div class="search">
      <Icon name="search" size={16} />
      <input
        data-autofocus
        class="input"
        type="text"
        role="combobox"
        aria-expanded="true"
        aria-controls="palette-list"
        aria-autocomplete="list"
        aria-activedescendant={results.length ? `palette-opt-${idx}` : undefined}
        placeholder={t('palette.placeholder')}
        aria-label={t('palette.placeholder')}
        bind:value={query}
        autocomplete="off"
        spellcheck="false"
      />
      <kbd>Esc</kbd>
    </div>
    <ul id="palette-list" class="list" role="listbox" aria-label={t('palette.results')} bind:this={listEl}>
      {#each results as r, i (r.item.id)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <li
          id="palette-opt-{i}"
          data-index={i}
          role="option"
          aria-selected={i === idx}
          class:active={i === idx}
          onclick={() => run(r.item)}
          onmousemove={() => (active = i)}
        >
          <span class="group">{r.item.group}</span>
          <span class="label">
            {#each parts(r.item, r.match.positions) as p, k (k)}{#if p.hit}<mark>{p.text}</mark>{:else}{p.text}{/if}{/each}
          </span>
          {#if r.item.shortcut}<kbd>{r.item.shortcut}</kbd>{/if}
        </li>
      {:else}
        <li class="none" role="presentation">{t('palette.none', { q: query })}</li>
      {/each}
    </ul>
    <div class="foot" aria-live="polite">
      <span>{tn('palette.count', results.length)}</span>
      <span><kbd>↑</kbd> <kbd>↓</kbd> {t('palette.navigate')} · <kbd>↵</kbd> {t('palette.run')}</span>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 14vh;
    background: var(--bg-overlay);
    -webkit-backdrop-filter: blur(6px) saturate(1.1);
    backdrop-filter: blur(6px) saturate(1.1);
    animation: fade var(--dur) var(--ease);
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  .dialog {
    width: min(620px, calc(100vw - 2 * var(--space-4)));
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--bg-float) 92%, transparent);
    -webkit-backdrop-filter: blur(24px);
    backdrop-filter: blur(24px);
    box-shadow: var(--elev-2), var(--elev-inset);
    overflow: hidden;
  }
  .search {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--border);
    color: var(--text-subtle);
  }
  .input {
    flex: 1;
    min-width: 0;
    border: none;
    background: none;
    outline: none;
    font-size: var(--text-lg);
    color: var(--text);
  }
  .input::placeholder {
    color: var(--text-subtle);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    margin: 0;
    padding: var(--space-2);
    list-style: none;
  }
  li[role='option'] {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 36px;
    padding: 0 var(--space-3);
    border-radius: var(--radius-md);
    cursor: pointer;
  }
  li.active {
    background: var(--bg-selected);
  }
  .group {
    flex: none;
    width: 84px;
    color: var(--text-subtle);
    font-size: var(--text-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  mark {
    background: none;
    color: var(--accent);
    font-weight: 700;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .none {
    padding: var(--space-5);
    text-align: center;
    color: var(--text-muted);
  }
  .foot {
    display: flex;
    justify-content: space-between;
    padding: var(--space-2) var(--space-4);
    border-top: 1px solid var(--border);
    color: var(--text-subtle);
    font-size: var(--text-xs);
  }
  @media (forced-colors: active) {
    li.active {
      outline: 2px solid Highlight;
      outline-offset: -2px;
    }
  }
</style>
