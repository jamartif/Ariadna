<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import { t, type MessageKey } from '$lib/i18n';
  import type { Branch } from '$lib/api/types';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { branchesOf } from '$lib/stores/branches.svelte';
  import { remotesOf } from '$lib/stores/remotes.svelte';
  import { toasts } from '$lib/stores/toasts.svelte';
  import Icon from './Icon.svelte';
  import type { IconName } from './icons';

  /**
   * MENÚ CONTEXTUAL DE UNA RAMA del riel.
   *
   * - Se abre con clic derecho sobre la fila o con la tecla Menú / Mayús+F10 (lo hace `BranchRail`).
   * - `role="menu"` con `menuitem`s: ↑/↓/Inicio/Fin mueven el foco, Escape cierra y devuelve el foco
   *   a la fila, Tab sale del menú. Las acciones que no aplican llevan `aria-disabled` y el motivo
   *   en el `title` (siguen siendo enfocables para poder leerlo).
   * - Se coloca en el punto del clic y, si no cabe, se voltea y se pega al borde de la ventana.
   * - «Comparar con…» no abre un submenú flotante: cambia el contenido del mismo menú al segundo
   *   paso (elegir la otra rama), con «Volver» como primera entrada.
   */
  interface Props {
    tab: TabState;
    branch: Branch;
    /** La fila es una rama del remoto (`origin/x`). */
    remote: boolean;
    /** Punto del clic, en coordenadas de la ventana. */
    x: number;
    y: number;
    /** Cierra el menú; `BranchRail` devuelve el foco a la fila. */
    onclose: () => void;
    /** «Crear una rama desde esta…»: prellena el punto de partida en el riel. */
    oncreatefrom: (startPoint: string) => void;
    /** «Renombrar…»: abre el formulario del inspector con ese nombre. */
    onrename: (name: string) => void;
    /** Abre el detalle de la rama en el panel derecho. */
    oninspect: (name: string) => void;
  }
  let { tab, branch, remote, x, y, onclose, oncreatefrom, onrename, oninspect }: Props = $props();

  interface Item {
    id: string;
    label: string;
    icon: IconName;
    /** Motivo por el que no se puede usar (va al `title`). `null`/ausente = disponible. */
    blocked?: string | null;
    danger?: boolean;
    /** Cambia al segundo paso del menú en vez de ejecutar una acción. */
    submenu?: boolean;
    /** Ejecuta la acción sin cerrar el menú (cambios de paso, como «Volver»). */
    keep?: boolean;
    run?: () => void | Promise<void>;
  }

  let mode = $state<'actions' | 'compare'>('actions');
  let el = $state<HTMLDivElement>();
  // Posición inicial en el punto del clic; `place()` la ajusta al tamaño real del menú.
  let pos = $state({ left: 0, top: 0 });

  const b = $derived(branchesOf(tab));
  const r = $derived(remotesOf(tab));
  const cur = $derived(b.current);
  const busy = $derived(b.busy !== null || r.isBusy());
  const integrating = $derived((tab.snapshot?.integration ?? 'none') !== 'none');
  const local = $derived(b.locals.find((x2) => x2.name === branch.name));
  const shortName = $derived(remote ? branch.name.slice(branch.name.indexOf('/') + 1) : branch.name);
  /** Remoto de esta rama: el suyo si es remota, el de su upstream (u `origin`) si es local. */
  const remoteName = $derived(
    remote
      ? (branch.name.split('/')[0] ?? null)
      : (local?.upstream?.split('/')[0] ??
          (r.views.some((v) => v.name === 'origin') ? 'origin' : (r.views[0]?.name ?? null)))
  );
  const web = $derived(r.linkFor(branch.name, remote));
  const isHead = $derived(branch.isHead === true);
  const canInput = (v: string | null | undefined): string | null => (v === null || v === undefined || v === '' ? t('rail.menu.noRemote') : null);

  const busyReason = $derived(busy ? t('rail.menu.busy') : null);

  const groups = $derived<Item[][]>([
    [
      {
        id: 'checkout',
        label: remote ? t('rail.menu.checkoutRemote') : t('rail.menu.checkout'),
        icon: 'arrow-right',
        blocked: isHead ? t('rail.menu.alreadyCurrent') : busy ? busyReason : null,
        run: () => void b.checkout(branch.name)
      },
      {
        id: 'from',
        label: t('rail.menu.createFrom'),
        icon: 'plus',
        blocked: busyReason,
        run: () => oncreatefrom(branch.name)
      },
      {
        id: 'merge',
        label: t('rail.menu.merge'),
        icon: 'git-merge',
        blocked: remote
          ? t('rail.menu.onlyLocal')
          : isHead
            ? t('rail.menu.alreadyCurrent')
            : !cur
              ? t('rail.menu.noCurrent')
              : integrating
                ? t('rail.menu.integration')
                : busyReason,
        run: () => b.askMerge(branch.name)
      },
      {
        id: 'rebase',
        label: t('rail.menu.rebase'),
        icon: 'git-fork',
        blocked: isHead ? t('rail.menu.alreadyCurrent') : !cur ? t('rail.menu.noCurrent') : integrating ? t('rail.menu.integration') : busyReason,
        run: () => void b.rebase(branch.name)
      }
    ],
    [
      {
        id: 'fetch',
        label: remoteName ? t('rail.menu.fetch', { remote: remoteName }) : t('rail.menu.fetchGeneric'),
        icon: 'download',
        blocked: busyReason ?? canInput(remoteName),
        run: () => {
          if (remoteName) void r.fetch(remoteName);
        }
      },
      {
        id: 'push',
        label: t('rail.menu.push'),
        icon: 'arrow-up',
        blocked: remote ? t('rail.menu.onlyLocal') : canInput(remoteName) ?? busyReason,
        run: () => void b.pushBranch(branch.name)
      },
      {
        id: 'pull',
        label: t('rail.menu.pull'),
        icon: 'arrow-down',
        blocked: !isHead ? t('rail.menu.onlyCurrent') : canInput(remoteName) ?? busyReason,
        run: () => {
          if (remoteName) void r.pull(remoteName);
        }
      }
    ],
    [
      {
        id: 'rename',
        label: t('rail.menu.rename'),
        icon: 'pencil',
        blocked: remote ? t('rail.menu.onlyLocal') : busyReason,
        run: () => onrename(branch.name)
      },
      {
        id: 'delete',
        label: t('rail.menu.delete'),
        icon: 'trash',
        danger: true,
        blocked: remote ? t('rail.menu.onlyLocal') : isHead ? t('branches.cannotDeleteCurrent') : busyReason,
        run: () => void b.remove(branch.name)
      },
      {
        id: 'delete-remote',
        label: remoteName ? t('rail.menu.deleteRemote', { remote: remoteName }) : t('rail.menu.deleteRemoteGeneric'),
        icon: 'trash',
        danger: true,
        blocked: remote ? busyReason ?? canInput(remoteName) : t('rail.menu.onlyRemote'),
        run: () => {
          if (remoteName) void b.removeRemote(remoteName, shortName);
        }
      }
    ],
    [
      { id: 'copy-name', label: t('rail.menu.copyName'), icon: 'copy', run: () => copy(branch.name, 'rail.menu.copiedName', { name: branch.name }) },
      { id: 'copy-sha', label: t('rail.menu.copySha'), icon: 'clipboard', run: () => copy(branch.oid, 'rail.menu.copiedSha', { sha: branch.oid.slice(0, 7) }) },
      {
        id: 'diff',
        label: cur ? t('rail.menu.diffAgainst', { base: cur.name }) : t('rail.menu.diffAgainst', { base: 'HEAD' }),
        icon: 'file-diff',
        blocked: isHead ? t('rail.menu.alreadyCurrent') : !cur ? t('rail.menu.noCurrent') : null,
        run: () => {
          if (cur) void compareWith(cur.name);
        }
      },
      { id: 'compare', label: t('rail.menu.compare'), icon: 'list', submenu: true, run: () => void toCompare() }
    ],
    [
      {
        id: 'open',
        label: t('rail.menu.open'),
        icon: 'external-link',
        blocked: !web ? t('rail.menu.noWeb') : !web.remote.canOpen ? t('rail.menu.noWeb') : busyReason,
        run: () => {
          if (web) void r.open(web.remote.name, 'branch', web.branch);
        }
      },
      {
        id: 'pr',
        label: t('rail.menu.createPr'),
        icon: 'git-pull-request',
        blocked: !web ? t('rail.menu.noPr') : !web.remote.canCreatePullRequest ? t('rail.menu.noPr') : busyReason,
        run: () => {
          if (web) void r.open(web.remote.name, 'pull-request', web.branch);
        }
      }
    ]
  ]);

  /** Segundo paso: con qué rama comparar (todas menos la propia). */
  const candidates = $derived(
    [...b.locals.map((x2) => ({ name: x2.name, remote: false })), ...b.remotes.map((x2) => ({ name: x2.name, remote: true }))].filter(
      (c) => c.name !== branch.name
    )
  );

  const compareItems = $derived<Item[]>([
    { id: 'back', label: t('rail.menu.compareBack'), icon: 'chevron-left', keep: true, run: () => void back() },
    ...candidates.map((c) => ({
      id: `cmp:${c.name}`,
      label: c.name,
      icon: (c.remote ? 'cloud' : 'git-branch') as IconName,
      run: () => void compareWith(c.name)
    }))
  ]);

  const items = $derived<Item[]>(mode === 'compare' ? compareItems : groups.flat());

  async function copy(text: string, key: MessageKey, params: Record<string, string>): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
      toasts.success(t(key, params));
    } catch {
      toasts.warning(t('rail.menu.copyFailed'));
    }
  }

  /** Compara la rama del clic con `base` y abre su detalle (donde se pinta el diff). */
  function compareWith(base: string): void {
    // Todo lo que lee propiedades se resuelve antes de cerrar: al cerrarse, el riel desmonta este
    // componente y las propiedades (y los `$derived` que dependen de ellas) dejan de leerse. Los
    // controladores son objetos estables, así que se capturan y se usan después.
    const ctl = b;
    const head = branch.name;
    ctl.select(head);
    oninspect(head);
    onclose();
    void ctl.compare(base, head);
  }

  async function toCompare(): Promise<void> {
    mode = 'compare';
    await tick();
    place();
    focusAt(0);
  }

  async function back(): Promise<void> {
    mode = 'actions';
    await tick();
    place();
    focusAt(0);
  }

  function choose(item: Item): void {
    if (item.blocked) return;
    if (item.submenu) {
      void toCompare();
      return;
    }
    // La acción corre **antes** de cerrar: al cerrarse, el riel desmonta el menú y sus propiedades
    // dejan de leerse. `run` resuelve lo suyo de forma síncrona; lo lento sigue por su cuenta.
    const pending = item.run?.();
    if (!item.keep) onclose();
    void pending;
  }

  // ---- Teclado y foco ---------------------------------------------------------------------

  function nodes(): HTMLElement[] {
    return [...(el?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
  }

  function focusAt(i: number): void {
    const list = nodes();
    if (list.length === 0) {
      el?.focus();
      return;
    }
    const n = ((i % list.length) + list.length) % list.length;
    list[n]?.focus();
  }

  function keydown(e: KeyboardEvent): void {
    const list = nodes();
    const i = list.indexOf(document.activeElement as HTMLElement);
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        focusAt(i + 1);
        break;
      case 'ArrowUp':
        e.preventDefault();
        focusAt(i < 0 ? list.length - 1 : i - 1);
        break;
      case 'Home':
        e.preventDefault();
        focusAt(0);
        break;
      case 'End':
        e.preventDefault();
        focusAt(list.length - 1);
        break;
      case 'ArrowRight':
        // Solo «Comparar con…» tiene segundo paso; en el resto no hace nada.
        if (mode === 'actions' && items[i]?.submenu) {
          e.preventDefault();
          void toCompare();
        }
        break;
      case 'ArrowLeft':
        if (mode === 'compare') {
          e.preventDefault();
          void back();
        }
        break;
      case 'Escape':
        e.preventDefault();
        e.stopPropagation();
        onclose();
        break;
      case 'Tab':
        // WAI-ARIA: Tab sale del menú y lo cierra (el foco sigue su camino normal).
        onclose();
        break;
      default:
        break;
    }
  }

  // ---- Colocación sin salirse de la ventana ------------------------------------------------

  function place(): void {
    const node = el;
    if (!node) return;
    const rect = node.getBoundingClientRect();
    const width = rect.width || 240;
    const height = rect.height || 320;
    const vw = window.innerWidth || document.documentElement.clientWidth || 1024;
    const vh = window.innerHeight || document.documentElement.clientHeight || 768;
    const m = 8;
    let left = x;
    let top = y;
    // A la izquierda del cursor si a la derecha no cabe; luego se pega al borde si tampoco así.
    if (left + width + m > vw) left = x - width;
    if (left < m) left = Math.max(m, vw - width - m);
    // Hacia arriba si abajo no cabe.
    if (top + height + m > vh) top = y - height;
    if (top < m) top = Math.max(m, vh - height - m);
    pos = { left: Math.round(left), top: Math.round(top) };
  }

  // El riel abre el menú de otra fila sin cerrar el anterior: mismas props nuevas, misma
  // instancia. Al cambiar el punto de apertura se vuelve a colocar y al primer paso.
  let placedFor = { x: Number.NaN, y: Number.NaN };
  $effect(() => {
    const px = x;
    const py = y;
    if (placedFor.x === px && placedFor.y === py) return;
    placedFor = { x: px, y: py };
    untrack(() => {
      mode = 'actions';
      place();
      queueMicrotask(() => focusAt(0));
    });
  });

  onMount(() => {
    place();
    focusAt(0);
    const outside = (e: PointerEvent) => {
      if (!el) return;
      const target = e.target;
      if (target instanceof Node && el.contains(target)) return;
      onclose();
    };
    const dismiss = () => onclose();
    window.addEventListener('pointerdown', outside, true);
    window.addEventListener('resize', dismiss);
    window.addEventListener('blur', dismiss);
    return () => {
      window.removeEventListener('pointerdown', outside, true);
      window.removeEventListener('resize', dismiss);
      window.removeEventListener('blur', dismiss);
    };
  });
</script>

<div
  bind:this={el}
  class="menu"
  role="menu"
  tabindex="-1"
  aria-label={mode === 'compare' ? t('rail.menu.compareTitle', { name: branch.name }) : t('rail.menu.label', { name: branch.name })}
  style:left="{pos.left}px"
  style:top="{pos.top}px"
  data-testid="branch-menu"
  data-mode={mode}
  onkeydown={keydown}
>
  <p class="head" aria-hidden="true">
    <Icon name={remote ? 'cloud' : 'git-branch'} size={12} />
    <span class="hm mono">{branch.name}</span>
  </p>

  {#if mode === 'actions'}
    {#each groups as group, gi (gi)}
      {#if gi > 0}<div class="sep" role="separator"></div>{/if}
      <div class="group" role="group">
        {#each group as item (item.id)}
          {@render entry(item)}
        {/each}
      </div>
    {/each}
  {:else}
    <div class="group" role="group">
      {#each compareItems as item (item.id)}
        {@render entry(item)}
      {/each}
    </div>
  {/if}
</div>

{#snippet entry(item: Item)}
  <button
    type="button"
    role="menuitem"
    class="mi"
    class:bad={item.danger}
    aria-disabled={item.blocked ? 'true' : undefined}
    title={item.blocked ?? undefined}
    tabindex="-1"
    data-testid="branch-menu-{item.id}"
    onclick={() => choose(item)}
  >
    <Icon name={item.icon} size={13} />
    <span class="lbl">{item.label}</span>
    {#if item.blocked}<Icon name="ban" size={12} />{/if}
  </button>
{/snippet}

<style>
  .menu {
    position: fixed;
    z-index: 60;
    min-width: 232px;
    max-width: 320px;
    max-height: min(80vh, 560px);
    overflow: auto;
    padding: var(--space-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-surface);
    box-shadow: var(--shadow-lg, 0 12px 32px rgba(0, 0, 0, 0.35));
  }
  .menu:focus {
    outline: none;
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    margin: 0 0 var(--space-1);
    padding: 0 var(--space-2) var(--space-1);
    border-bottom: 1px solid var(--border);
    color: var(--text-subtle);
  }
  .head :global(svg) {
    flex: none;
    color: var(--accent);
  }
  .hm {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-2xs);
    font-weight: 650;
    color: var(--text-muted);
  }
  .sep {
    height: 1px;
    margin: var(--space-1) var(--space-1);
    background: var(--border);
  }
  .mi {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: 26px;
    padding: 0 var(--space-2);
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text);
    text-align: left;
    font-size: var(--text-sm);
  }
  .mi :global(svg) {
    flex: none;
    color: var(--text-subtle);
  }
  .mi:hover:not([aria-disabled='true']),
  .mi:focus {
    background: var(--bg-hover);
    outline: none;
  }
  /* El foco se ve siempre (también al abrir con el ratón): es la fila activa del menú, como en
     los menús nativos. */
  .mi:focus {
    box-shadow: inset 0 0 0 2px var(--focus-ring);
  }
  .mi[aria-disabled='true'] {
    color: var(--text-subtle);
    cursor: default;
  }
  .mi.bad {
    color: var(--danger);
  }
  .mi.bad :global(svg) {
    color: var(--danger);
  }
  .lbl {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
