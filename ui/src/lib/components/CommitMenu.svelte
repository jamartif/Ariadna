<script lang="ts">
  import { onMount } from 'svelte';
  import type { Commit } from '$lib/api/types';
  import { t, type MessageKey } from '$lib/i18n';
  import { toasts } from '$lib/stores/toasts.svelte';
  import type { TabState } from '$lib/stores/tab.svelte';
  import Icon from './Icon.svelte';
  import type { IconName } from './icons';

  export interface CommitMenuProps {
    tab: TabState;
    commit: Commit;
    /** Punto del clic, en coordenadas de la ventana. */
    x: number;
    y: number;
    onclose: () => void;
    /** «Crear etiqueta…»: abre el formulario apuntando a este commit. */
    oncreatetag: (commit: Commit) => void;
  }

  let { tab, commit, x, y, onclose, oncreatetag }: CommitMenuProps = $props();

  interface Item {
    id: string;
    label: string;
    icon: IconName;
    run: () => void;
  }

  async function copy(text: string, done: MessageKey): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
      toasts.success(t(done));
    } catch {
      toasts.error(t('history.menu.copyFailed'), new Error(t('history.menu.copyFailed')));
    }
    onclose();
  }

  const groups = $derived<Item[][]>([
    [{ id: 'tag', label: t('history.menu.createTag'), icon: 'tag', run: () => { oncreatetag(commit); onclose(); } }],
    [
      { id: 'sha', label: t('history.menu.copySha'), icon: 'clipboard', run: () => void copy(commit.sha, 'history.menu.copyShaDone') },
      { id: 'subject', label: t('history.menu.copySubject'), icon: 'copy', run: () => void copy(commit.subject, 'history.menu.copySubjectDone') }
    ]
  ]);

  let menuEl = $state<HTMLDivElement>();
  let cursor = $state(0);

  const flat = $derived(groups.flat());

  onMount(() => {
    queueMicrotask(() => focusAt(0));
  });

  function focusAt(i: number): void {
    if (flat.length === 0) return;
    cursor = (i + flat.length) % flat.length;
    menuEl?.querySelectorAll<HTMLButtonElement>('.item')[cursor]?.focus();
  }

  function key(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      focusAt(cursor + 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      focusAt(cursor - 1);
    } else if (e.key === 'Home') {
      e.preventDefault();
      focusAt(0);
    } else if (e.key === 'End') {
      e.preventDefault();
      focusAt(flat.length - 1);
    } else if (e.key === 'Tab') {
      e.preventDefault();
      focusAt(cursor + (e.shiftKey ? -1 : 1));
    }
  }

  /** Punto de la ventana donde dibujar el menú sin que se salga por abajo o por la derecha. */
  const pos = $derived.by(() => {
    const w = 220;
    const h = 44 + flat.length * 32;
    const vw = typeof window === 'undefined' ? 1024 : window.innerWidth;
    const vh = typeof window === 'undefined' ? 768 : window.innerHeight;
    return { left: `${Math.max(8, Math.min(x, vw - w - 8))}px`, top: `${Math.max(8, Math.min(y, vh - h - 8))}px` };
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose} oncontextmenu={(e) => { e.preventDefault(); onclose(); }}></div>

<div
  bind:this={menuEl}
  class="menu float"
  role="menu"
  aria-label={t('history.menu.label')}
  tabindex="-1"
  style:left={pos.left}
  style:top={pos.top}
  onkeydown={key}
>
  {#each groups as group, gi (gi)}
    {#if gi > 0}<div class="sep" role="separator"></div>{/if}
    {#each group as item (item.id)}
      <button
        type="button"
        class="item"
        role="menuitem"
        onclick={() => item.run()}
        onmouseenter={() => (cursor = flat.findIndex((f) => f.id === item.id))}
      >
        <Icon name={item.icon} size={14} />
        <span class="label">{item.label}</span>
      </button>
    {/each}
  {/each}
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
  .menu {
    position: fixed;
    z-index: 41;
    display: flex;
    flex-direction: column;
    min-width: 220px;
    padding: var(--space-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-surface);
    box-shadow: var(--elev-3);
  }
  .item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    height: 32px;
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: var(--text-sm);
    text-align: left;
    cursor: pointer;
  }
  .item:hover,
  .item:focus-visible {
    background: var(--bg-hover);
    outline: none;
  }
  .item :global(svg) {
    flex: none;
    color: var(--text-muted);
  }
  .sep {
    height: 1px;
    margin: var(--space-1) 0;
    background: var(--border);
  }
</style>
