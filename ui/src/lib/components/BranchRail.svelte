<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { t, tn } from '$lib/i18n';
  import type { Branch } from '$lib/api/types';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { branchesOf } from '$lib/stores/branches.svelte';
  import { remotesOf } from '$lib/stores/remotes.svelte';
  import { branchNameProblem } from '$lib/util/branchName';
  import { formatCombo, isMac, registerHandlers, SHORTCUTS } from '$lib/shortcuts/registry';
  import BranchMenu from './BranchMenu.svelte';
  import Icon from './Icon.svelte';

  /**
   * RIEL DE RAMAS — gestión de ramas siempre visible a la izquierda, en todas las vistas.
   *
   * - Cabecera con contador, «nueva rama», «remotos y etiquetas» y contraer.
   * - Tarjeta de la rama actual con seguimiento y traer/bajar/subir (contadores ahead/behind).
   * - Filtro instantáneo + árbol Local/Remoto agrupado por prefijo (`feature/`, `origin/`).
   * - Inspector de la rama seleccionada (acciones: cambiar, fusionar, renombrar, borrar, detalle).
   * - Contraído: tira de iconos que mantiene el acceso a rama actual, filtro y nueva rama.
   */
  interface Props {
    tab: TabState;
    collapsed: boolean;
    ontoggle: () => void;
    /** Abre el detalle de la rama en el panel derecho. */
    oninspect: (name: string) => void;
    /** Abre el diálogo de remotos y etiquetas. */
    onrepo: () => void;
  }
  let { tab, collapsed, ontoggle, oninspect, onrepo }: Props = $props();

  const b = $derived(branchesOf(tab));
  const r = $derived(remotesOf(tab));
  const snap = $derived(tab.snapshot);
  const cur = $derived(b.current);
  const remoteName = $derived(cur?.upstream?.split('/')[0] ?? r.views[0]?.name ?? null);

  let filter = $state('');
  let filterInput = $state<HTMLInputElement>();
  let creating = $state(false);
  let newName = $state('');
  let startPoint = $state('');
  let nameInput = $state<HTMLInputElement>();
  let cleaning = $state(false);
  let localOpen = $state(true);
  let remoteOpen = $state(true);
  let closedGroups = $state.raw<ReadonlySet<string>>(new Set());
  let focusName = $state<string | null>(null);

  const q = $derived(filter.trim().toLowerCase());
  const match = (x: Branch) => q === '' || x.name.toLowerCase().includes(q);
  const locals = $derived(b.locals.filter(match));
  const remotes = $derived(b.remotes.filter(match));
  const empty = $derived(!snap && tab.loading);

  /** Agrupa por primer segmento: `feature/x` → «feature/», `origin/main` → «origin/». */
  function group(list: Branch[], remote: boolean): [string, Branch[]][] {
    const map = new Map<string, Branch[]>();
    for (const br of list) {
      const i = br.name.indexOf('/');
      const g = i < 0 ? '' : remote ? br.name.slice(0, i + 1) : br.name.slice(0, i);
      const arr = map.get(g);
      if (arr) arr.push(br);
      else map.set(g, [br]);
    }
    // La raíz primero; los grupos por orden alfabético.
    return [...map.entries()].sort(([a], [b2]) => (a === '' ? -1 : b2 === '' ? 1 : a.localeCompare(b2)));
  }
  const localGroups = $derived(group(locals, false));
  const remoteGroups = $derived(group(remotes, true));
  const openGroup = (key: string) => !closedGroups.has(key);

  /** Filas visibles en orden de pintado (para el foco móvil con ↑/↓). */
  const flatRows = $derived.by(() => {
    const rows: { br: Branch; remote: boolean }[] = [];
    if (localOpen) for (const [g, list] of localGroups) if (g === '' || openGroup(`local:${g}`)) rows.push(...list.map((br) => ({ br, remote: false })));
    if (remoteOpen) for (const [g, list] of remoteGroups) if (openGroup(`remote:${g}`)) rows.push(...list.map((br) => ({ br, remote: true })));
    return rows;
  });
  const tabStop = $derived.by(() => {
    const names = flatRows.map((x) => x.br.name);
    if (focusName && names.includes(focusName)) return focusName;
    if (b.selected && names.includes(b.selected)) return b.selected;
    return names[0] ?? null;
  });

  const selBranch = $derived(b.selectedBranch);
  /** El inspector aparece solo tras pulsar una rama (la tarjeta ya resume la actual). */
  const showInspector = $derived(b.selected !== null && selBranch !== undefined);
  const selRemote = $derived(selBranch ? b.isRemote(selBranch.name) : false);
  const busy = $derived(b.busy !== null || r.isBusy());
  const integrating = $derived((snap?.integration ?? 'none') !== 'none');

  // Si la rama seleccionada desaparece de verdad (borrado externo, snapshot nuevo), se limpia la
  // selección. Se compara con el snapshot anterior: una rama recién creada nunca estaba, así que
  // no se descarta mientras el estado nuevo viaja.
  let prevNames = new Set<string>();
  $effect(() => {
    const names = new Set([...b.locals, ...b.remotes].map((x) => x.name));
    const sel = b.selected;
    if (sel !== null && prevNames.has(sel) && !names.has(sel)) b.selected = null;
    prevNames = names;
  });

  // La tarjeta de la rama actual necesita el nombre del remoto: se piden los remotos (Rust, local).
  const remoteSig = $derived((snap?.remotes ?? []).map((x) => `${x.name}=${x.fetchUrl}`).join('|'));
  $effect(() => {
    void remoteSig;
    if (!tab.hibernated) void r.load();
  });

  onMount(() =>
    registerHandlers({
      'rail.focus': () => {
        if (collapsed) ontoggle();
        queueMicrotask(() => {
          filterInput?.focus();
          filterInput?.select();
        });
        return true;
      }
    })
  );

  const mac = isMac();
  const toggleCombo = $derived(formatCombo(SHORTCUTS.find((s) => s.id === 'rail.toggle')?.combo ?? 'Mod+B', mac));
  const focusCombo = $derived(formatCombo(SHORTCUTS.find((s) => s.id === 'rail.focus')?.combo ?? 'Mod+Shift+B', mac));
  const fetching = $derived(r.busy?.startsWith('fetch:') ?? false);
  const pulling = $derived(r.busy?.startsWith('pull:') ?? false);
  const pushing = $derived(r.busy?.startsWith('push:') ?? false);

  function expand() {
    if (collapsed) ontoggle();
  }

  function startCreate(startFrom?: string): void {
    creating = true;
    newName = '';
    startPoint = startFrom ?? selBranch?.name ?? '';
    queueMicrotask(() => nameInput?.focus());
  }

  const problem = $derived(newName === '' ? null : branchNameProblem(newName));

  async function submitCreate(e: Event) {
    e.preventDefault();
    if (newName === '' || problem !== null) return;
    const ok = await b.create(newName, startPoint.trim() === '' ? null : startPoint.trim());
    if (ok) {
      // Al crearla, se selecciona: el inspector muestra la rama recién creada con sus acciones.
      b.select(newName.trim());
      creating = false;
      newName = '';
    }
  }

  function select(name: string): void {
    focusName = name;
    b.select(name);
  }

  function open(row: Branch): void {
    // Clic: selecciona (el inspector ofrece las acciones). Doble clic: cambia de rama.
    select(row.name);
  }

  async function checkoutOrCreate(br: Branch): Promise<void> {
    if (await b.checkout(br.name)) b.selected = null;
  }

  function focusRow(name: string): void {
    const el = Array.from(document.querySelectorAll<HTMLElement>('[data-brail-name]')).find((x) => x.dataset.brailName === name);
    el?.focus();
  }

  // ---- Menú contextual de una fila ---------------------------------------------------------

  /** Rama cuyo menú está abierto, con el punto donde se abrió (coordenadas de la ventana). */
  let menu = $state<{ br: Branch; remote: boolean; x: number; y: number } | null>(null);

  function openMenu(br: Branch, remote: boolean, x: number, y: number): void {
    select(br.name);
    menu = { br, remote, x, y };
  }

  /** Cierra el menú y devuelve el foco a la fila (teclado y lectores de pantalla). */
  function closeMenu(): void {
    const name = menu?.br.name ?? null;
    menu = null;
    if (name !== null) queueMicrotask(() => focusRow(name));
  }

  /** Clic derecho: el menú nativo ya está suprimido por `installContextMenuGuard`. */
  function rowContextMenu(e: MouseEvent, br: Branch, remote: boolean): void {
    e.preventDefault();
    openMenu(br, remote, e.clientX, e.clientY);
  }

  /** Menú de teclado de la fila: tecla Menú o Mayús+F10, junto a la fila enfocada. */
  function rowMenuKey(e: KeyboardEvent, br: Branch, remote: boolean): boolean {
    if (e.key !== 'ContextMenu' && !(e.shiftKey && e.key === 'F10')) return false;
    e.preventDefault();
    const rect = (e.currentTarget as HTMLElement | null)?.getBoundingClientRect();
    openMenu(br, remote, rect ? rect.left + 24 : 24, rect ? Math.min(rect.bottom, rect.top + 24) : 24);
    return true;
  }

  /** «Crear una rama desde esta…»: el formulario se abre con ese punto de partida. */
  async function createFrom(startPoint: string): Promise<void> {
    b.select(startPoint);
    await tick();
    startCreate(startPoint);
  }

  /** «Renombrar…»: abre el formulario del inspector con esa rama seleccionada. */
  async function renameFrom(name: string): Promise<void> {
    b.select(name);
    await tick();
    startRename();
  }

  function rowKey(e: KeyboardEvent, name: string): void {
    const i = flatRows.findIndex((x) => x.br.name === name);
    let next = -1;
    if (e.key === 'ArrowDown') next = Math.min(flatRows.length - 1, i + 1);
    else if (e.key === 'ArrowUp') next = Math.max(0, i - 1);
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = flatRows.length - 1;
    else return;
    e.preventDefault();
    const target = flatRows[next];
    if (!target) return;
    focusName = target.br.name;
    focusRow(target.br.name);
  }

  function filterKey(e: KeyboardEvent): void {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      const first = flatRows[0];
      if (first) {
        focusName = first.br.name;
        focusRow(first.br.name);
      }
    } else if (e.key === 'Enter') {
      // Cambio rápido: Intro cambia a la primera coincidencia (o la selecciona si ya es la actual).
      const first = flatRows[0];
      if (!first) return;
      e.preventDefault();
      if (first.br.isHead) select(first.br.name);
      else void checkoutOrCreate(first.br);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      if (filter !== '') filter = '';
      else filterInput?.blur();
    }
  }

  function toggleGroup(key: string): void {
    const next = new Set(closedGroups);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    closedGroups = next;
  }

  let renameTo = $state('');
  let renameInput = $state<HTMLInputElement>();
  const renameProblem = $derived(renameTo === '' ? null : renameTo === selBranch?.name ? 'branches.problem.same' : branchNameProblem(renameTo));

  function startRename(): void {
    if (!selBranch) return;
    renameTo = selBranch.name;
    cleaning = true;
    queueMicrotask(() => renameInput?.select());
  }

  async function submitRename(e: Event) {
    e.preventDefault();
    if (!selBranch || renameProblem !== null || renameTo === '') return;
    if (await b.rename(selBranch.name, renameTo)) cleaning = false;
  }

  async function remove(): Promise<void> {
    if (!selBranch) return;
    await b.remove(selBranch.name);
  }
</script>

{#snippet branchRow(br: Branch, remote: boolean)}
  <li>
    <button
      type="button"
      class="row"
      class:head={br.isHead}
      class:sel={selBranch?.name === br.name && b.selected !== null}
      data-brail-name={br.name}
      data-testid="rail-branch"
      tabindex={tabStop === br.name ? 0 : -1}
      aria-current={br.isHead ? 'true' : undefined}
      aria-haspopup="menu"
      title={`${br.name} — ${t('rail.menu.hint')}`}
      onfocus={() => (focusName = br.name)}
      onclick={() => open(br)}
      ondblclick={() => !br.isHead && void checkoutOrCreate(br)}
      oncontextmenu={(e) => rowContextMenu(e, br, remote)}
      onkeydown={(e) => {
        if (!rowMenuKey(e, br, remote)) rowKey(e, br.name);
      }}
    >
      <Icon name={remote ? 'cloud' : 'git-branch'} size={13} />
      <span class="name mono">{br.name}</span>
      {#if br.isHead}<span class="dot" aria-hidden="true"></span>{/if}
      {#if br.ahead > 0}<span class="sync" title={t('sidebar.ahead', { n: br.ahead })}><Icon name="arrow-up" size={10} />{br.ahead}</span>{/if}
      {#if br.behind > 0}<span class="sync" title={t('sidebar.behind', { n: br.behind })}><Icon name="arrow-down" size={10} />{br.behind}</span>{/if}
      {#if br.isHead}<span class="sr-only">{t('sidebar.current')}</span>{/if}
      {#if br.ahead > 0}<span class="sr-only">{t('sidebar.ahead', { n: br.ahead })}</span>{/if}
      {#if br.behind > 0}<span class="sr-only">{t('sidebar.behind', { n: br.behind })}</span>{/if}
    </button>
  </li>
{/snippet}

{#snippet rows(list: Branch[], remote: boolean)}
  <ul class="rows">
    {#each list as br (br.name)}{@render branchRow(br, remote)}{/each}
  </ul>
{/snippet}

{#if collapsed}
  <aside class="rail collapsed" aria-label={t('rail.label')} data-testid="rail" data-collapsed="true">
    <button
      type="button"
      class="ico"
      aria-expanded="false"
      aria-label={t('rail.expand')}
      title={`${t('rail.expand')} (${toggleCombo})`}
      data-testid="rail-expand"
      onclick={ontoggle}
    >
      <Icon name="panel-left" size={16} />
    </button>
    <span class="rule" aria-hidden="true"></span>
    <button
      type="button"
      class="ico"
      aria-label={t('branches.new')}
      title={t('branches.new')}
      data-testid="rail-new-branch"
      onclick={async () => {
        expand();
        await tick();
        startCreate();
      }}
    >
      <Icon name="plus" size={16} />
    </button>
    <button
      type="button"
      class="ico cur"
      aria-label={cur ? t('rail.currentA11y', { name: cur.name }) : t('rail.noCurrent')}
      title={cur ? t('rail.currentA11y', { name: cur.name }) : t('rail.noCurrent')}
      data-testid="rail-current"
      onclick={() => { expand(); if (cur) select(cur.name); }}
    >
      <Icon name="git-branch" size={16} />
    </button>
    <button
      type="button"
      class="ico"
      aria-label={t('rail.find')}
      title={`${t('rail.find')} (${focusCombo})`}
      data-testid="rail-find"
      onclick={async () => {
        expand();
        await tick();
        filterInput?.focus();
      }}
    >
      <Icon name="search" size={16} />
    </button>
    <span class="grow"></span>
    <button type="button" class="ico" aria-label={t('rail.repo')} title={t('rail.repo')} data-testid="rail-repo" onclick={onrepo}>
      <Icon name="cloud" size={16} />
    </button>
  </aside>
{:else}
  <aside class="rail" aria-label={t('rail.label')} data-testid="rail" data-collapsed="false">
    <header class="head">
      <Icon name="git-branch" size={14} />
      <h2>{t('rail.title')}</h2>
      <span class="chip num" title={tn('rail.count', b.locals.length + b.remotes.length)}>{b.locals.length + b.remotes.length}</span>
      <span class="grow"></span>
      <button
        type="button"
        class="btn btn-ghost btn-icon btn-sm"
        aria-label={t('branches.new')}
        title={t('branches.new')}
        data-testid="rail-new-branch"
        onclick={() => startCreate()}
      >
        <Icon name="plus" size={15} />
      </button>
      <button
        type="button"
        class="btn btn-ghost btn-icon btn-sm"
        aria-label={t('rail.repo')}
        title={t('rail.repo')}
        data-testid="rail-repo"
        onclick={onrepo}
      >
        <Icon name="cloud" size={15} />
      </button>
      <button
        type="button"
        class="btn btn-ghost btn-icon btn-sm"
        aria-expanded="true"
        aria-label={t('rail.collapse')}
        title={`${t('rail.collapse')} (${toggleCombo})`}
        data-testid="rail-collapse"
        onclick={ontoggle}
      >
        <Icon name="panel-left" size={15} />
      </button>
    </header>

    {#if cur}
      <section class="current" aria-label={t('rail.current')} data-testid="rail-current">
        <div class="cur-top">
          <span class="cur-dot" aria-hidden="true"></span>
          <span class="cur-name mono" title={cur.name} data-testid="rail-current-name">{cur.name}</span>
          {#if cur.ahead > 0}<span class="sync" title={t('sidebar.ahead', { n: cur.ahead })}><Icon name="arrow-up" size={11} />{cur.ahead}</span>{/if}
          {#if cur.behind > 0}<span class="sync" title={t('sidebar.behind', { n: cur.behind })}><Icon name="arrow-down" size={11} />{cur.behind}</span>{/if}
        </div>
        <div class="cur-bottom">
          <span class="up" title={cur.upstream ?? t('rail.noTracks')}>
            {#if cur.upstream}<Icon name="cloud" size={10} />{/if}
            <span class="upname mono">{cur.upstream ?? t('rail.noTracks')}</span>
          </span>
          <span class="cur-actions" role="group" aria-label={t('rail.syncActions')}>
            <button
              type="button"
              class="btn btn-ghost btn-icon btn-sm"
              disabled={busy || !remoteName}
              title={t('remotes.fetch')}
              aria-label={t('remotes.fetch')}
              onclick={() => remoteName && r.fetch(remoteName)}
            >
              <Icon name={fetching ? 'loader' : 'refresh-cw'} size={13} spin={fetching} />
            </button>
            <button
              type="button"
              class="btn btn-ghost btn-icon btn-sm"
              disabled={busy || !remoteName}
              title={t('remotes.pull')}
              aria-label={t('remotes.pull')}
              onclick={() => remoteName && r.pull(remoteName)}
            >
              <Icon name={pulling ? 'loader' : 'arrow-down'} size={13} spin={pulling} />
            </button>
            <button
              type="button"
              class="btn btn-ghost btn-icon btn-sm"
              disabled={busy || !remoteName}
              title={t('remotes.push')}
              aria-label={t('remotes.push')}
              onclick={() => remoteName && r.push(remoteName)}
            >
              <Icon name={pushing ? 'loader' : 'arrow-up'} size={13} spin={pushing} />
            </button>
          </span>
        </div>
      </section>
    {:else}
      <p class="nocur">{t('rail.noCurrent')}</p>
    {/if}

    <div class="filter">
      <Icon name="search" size={13} />
      <input
        bind:this={filterInput}
        type="search"
        class="finput"
        bind:value={filter}
        placeholder={t('rail.filterPlaceholder')}
        aria-label={t('rail.filter')}
        aria-keyshortcuts="Control+Shift+b Meta+Shift+b"
        autocomplete="off"
        spellcheck="false"
        data-testid="rail-filter"
        onkeydown={filterKey}
      />
      {#if filter !== ''}
        <button type="button" class="btn btn-ghost btn-icon btn-sm clear" aria-label={t('rail.clearFilter')} title={t('rail.clearFilter')} data-testid="rail-clear-filter" onclick={() => (filter = '')}>
          <Icon name="x" size={12} />
        </button>
      {/if}
    </div>

    {#if b.prompt?.kind === 'checkout-dirty'}
      <div class="notice" role="alert" data-testid="rail-checkout-prompt">
        <strong>{t('branches.dirtyTitle')}</strong>
        <p>{tn('branches.dirtyBody', b.prompt.changes, { name: b.prompt.name })}</p>
        <div class="nact">
          <button type="button" class="btn btn-sm btn-ghost" data-testid="rail-cancel-prompt" onclick={() => b.cancelPrompt()}>{t('branches.cancel')}</button>
          <button type="button" class="btn btn-sm btn-primary" data-testid="rail-switch-anyway" onclick={() => b.prompt && b.checkout(b.prompt.name, true)}>{t('branches.switchAnyway')}</button>
        </div>
      </div>
    {/if}

    {#if creating}
      <form class="create" onsubmit={submitCreate} aria-label={t('branches.new')} data-testid="rail-create">
        <label class="lbl" for="rail-new-name">{t('branches.name')}</label>
        <input
          id="rail-new-name"
          bind:this={nameInput}
          class="field mono"
          bind:value={newName}
          placeholder="feature/mi-cambio"
          autocomplete="off"
          spellcheck="false"
          aria-invalid={problem ? 'true' : undefined}
          aria-describedby="rail-new-hint"
        />
        <label class="lbl" for="rail-new-start">{t('branches.startPoint')}</label>
        <input id="rail-new-start" class="field mono" bind:value={startPoint} placeholder={t('branches.startPlaceholder')} autocomplete="off" spellcheck="false" />
        <p id="rail-new-hint" class="hint" class:bad={problem !== null} aria-live="polite">
          {#if problem}<Icon name="circle-alert" size={12} />{t(problem)}{:else}{t('branches.nameHint')}{/if}
        </p>
        <div class="nact">
          <button type="button" class="btn btn-sm btn-ghost" data-testid="rail-create-cancel" onclick={() => (creating = false)}>{t('branches.cancel')}</button>
          <button
            type="submit"
            class="btn btn-sm btn-primary"
            data-testid="rail-create-submit"
            disabled={newName === '' || problem !== null || b.busy !== null}
          >
            {#if b.busy === 'create'}<Icon name="loader" size={13} spin />{/if}{t('branches.create')}
          </button>
        </div>
      </form>
    {/if}

    <nav class="tree" aria-label={t('rail.branchList')} data-testid="rail-tree">
      {#if empty}
        <div class="sk" aria-busy="true" aria-label={t('app.loading')}>
          {#each [80, 62, 72, 55] as w, i (i)}<span class="skeleton" style:width="{w}%" style:height="14px"></span>{/each}
        </div>
      {:else if b.locals.length === 0 && b.remotes.length === 0}
        <p class="none">{filter !== '' ? t('branches.noMatches', { query: filter.trim() }) : t('branches.emptyBody')}</p>
      {:else}
        <div class="section">
          <button type="button" class="grouphead" aria-expanded={localOpen} onclick={() => (localOpen = !localOpen)}>
            <Icon name={localOpen ? 'chevron-down' : 'chevron-right'} size={11} />
            <span>{t('branches.local')}</span>
            <span class="chip num">{locals.length}</span>
          </button>
          {#if localOpen}
            {#each localGroups as [g, list] (g)}
              {#if g === ''}
                {@render rows(list, false)}
              {:else}
                <button type="button" class="subhead" aria-expanded={openGroup(`local:${g}`)} onclick={() => toggleGroup(`local:${g}`)}>
                  <Icon name={openGroup(`local:${g}`) ? 'chevron-down' : 'chevron-right'} size={10} />
                  <span class="mono">{g}/</span>
                  <span class="subcount num">{list.length}</span>
                </button>
                {#if openGroup(`local:${g}`)}{@render rows(list, false)}{/if}
              {/if}
            {/each}
            {#if locals.length === 0}<p class="none sm">{t('branches.noMatches', { query: filter.trim() })}</p>{/if}
          {/if}
        </div>

        {#if b.remotes.length > 0}
          <div class="section">
            <button type="button" class="grouphead" aria-expanded={remoteOpen} onclick={() => (remoteOpen = !remoteOpen)}>
              <Icon name={remoteOpen ? 'chevron-down' : 'chevron-right'} size={11} />
              <span>{t('branches.remote')}</span>
              <span class="chip num">{remotes.length}</span>
            </button>
            {#if remoteOpen}
              {#each remoteGroups as [g, list] (g)}
                <button type="button" class="subhead" aria-expanded={openGroup(`remote:${g}`)} onclick={() => toggleGroup(`remote:${g}`)}>
                  <Icon name={openGroup(`remote:${g}`) ? 'chevron-down' : 'chevron-right'} size={10} />
                  <span class="mono">{g}</span>
                  <span class="subcount num">{list.length}</span>
                </button>
                {#if openGroup(`remote:${g}`)}{@render rows(list, true)}{/if}
              {/each}
              {#if remotes.length === 0}<p class="none sm">{t('branches.noMatches', { query: filter.trim() })}</p>{/if}
            {/if}
          </div>
        {/if}
      {/if}
    </nav>

    {#if showInspector && selBranch}
      <div class="inspector" role="group" aria-label={t('rail.inspector')} data-testid="rail-inspector">
        <div class="ihead">
          <Icon name={selRemote ? 'cloud' : 'git-branch'} size={13} />
          <span class="iname mono" title={selBranch.name}>{selBranch.name}</span>
          <span class="kind" class:current={selBranch.isHead}>
            {selRemote ? t('branches.kindRemote') : selBranch.isHead ? t('rail.current') : t('branches.kindLocal')}
          </span>
          <button
            type="button"
            class="btn btn-ghost btn-icon btn-sm close"
            aria-label={t('rail.clearSelection')}
            title={t('rail.clearSelection')}
            data-testid="rail-clear-selection"
            onclick={() => (b.selected = null)}
          >
            <Icon name="x" size={12} />
          </button>
        </div>
        <p class="imeta mono">
          {selBranch.oid.slice(0, 7)}
          {#if selBranch.upstream}{' · '}{selBranch.upstream}{/if}
        </p>

        {#if cleaning && !selRemote}
          <form class="rename" onsubmit={submitRename}>
            <label class="lbl" for="rail-rename">{t('branches.renameTo')}</label>
            <input
              id="rail-rename"
              bind:this={renameInput}
              class="field mono"
              bind:value={renameTo}
              autocomplete="off"
              spellcheck="false"
              aria-invalid={renameProblem ? 'true' : undefined}
              aria-describedby="rail-rename-hint"
            />
            <p id="rail-rename-hint" class="hint" class:bad={renameProblem !== null} aria-live="polite">
              {#if renameProblem}<Icon name="circle-alert" size={12} />{t(renameProblem)}{/if}
            </p>
            <div class="nact">
              <button type="button" class="btn btn-sm btn-ghost" onclick={() => (cleaning = false)}>{t('branches.cancel')}</button>
              <button type="submit" class="btn btn-sm btn-primary" disabled={renameTo === '' || renameProblem !== null || busy}>{t('branches.rename')}</button>
            </div>
          </form>
        {:else if b.prompt?.kind === 'merge' && b.prompt.name === selBranch.name}
          <div class="notice" role="group" aria-label={t('branches.mergeAsk', { name: selBranch.name, into: cur?.name ?? 'HEAD' })}>
            <strong>{t('branches.mergeAsk', { name: selBranch.name, into: cur?.name ?? 'HEAD' })}</strong>
            <p>{t('branches.mergeNote')}</p>
            <div class="nact">
              <button type="button" class="btn btn-sm btn-ghost" onclick={() => b.cancelPrompt()}>{t('branches.cancel')}</button>
              <button type="button" class="btn btn-sm btn-primary" onclick={() => b.merge(selBranch.name)}>{t('branches.mergeConfirm')}</button>
            </div>
          </div>
        {:else}
          <div class="iactions" role="group" aria-label={t('rail.actions')}>
            <button
              type="button"
              class="btn btn-sm btn-primary wide"
              data-testid="rail-checkout"
              disabled={selBranch.isHead || busy}
              onclick={() => void checkoutOrCreate(selBranch)}
            >
              {#if b.busy === 'checkout'}<Icon name="loader" size={13} spin />{:else}<Icon name="arrow-right" size={13} />{/if}
              {selRemote ? t('branches.checkoutRemote') : t('branches.checkout')}
            </button>
            {#if !selRemote}
              <button
                type="button"
                class="btn btn-sm"
                disabled={selBranch.isHead || busy || !cur || integrating}
                aria-label={t('branches.merge', { into: cur?.name ?? 'HEAD' })}
                title={integrating ? t('branches.integrationNote') : t('branches.mergeAsk', { name: selBranch.name, into: cur?.name ?? 'HEAD' })}
                onclick={() => b.askMerge(selBranch.name)}
              >
                {#if b.busy === 'merge'}<Icon name="loader" size={13} spin />{:else}<Icon name="git-merge" size={13} />{/if}
                {t('rail.merge')}
              </button>
            {/if}
            <div class="more">
              {#if !selRemote}
                <button type="button" class="btn btn-sm btn-ghost" disabled={busy} onclick={startRename} title={t('branches.rename')}>
                  <Icon name="pencil" size={13} /><span class="lbl-sm">{t('branches.rename')}</span>
                </button>
                <button
                  type="button"
                  class="btn btn-sm btn-ghost danger"
                  disabled={selBranch.isHead || busy}
                  title={selBranch.isHead ? t('branches.cannotDeleteCurrent') : t('branches.delete')}
                  onclick={remove}
                >
                  <Icon name="trash" size={13} /><span class="lbl-sm">{t('branches.delete')}</span>
                </button>
              {/if}
              <button
                type="button"
                class="btn btn-sm btn-ghost"
                disabled={busy}
                aria-label={t('rail.detailsOf', { name: selBranch.name })}
                title={t('rail.details')}
                onclick={() => selBranch && oninspect(selBranch.name)}
              >
                <Icon name="info" size={13} /><span class="lbl-sm">{t('rail.details')}</span>
              </button>
            </div>
          </div>
        {/if}
      </div>
    {/if}
  </aside>

  {#if menu}
    <BranchMenu
      {tab}
      branch={menu.br}
      remote={menu.remote}
      x={menu.x}
      y={menu.y}
      onclose={closeMenu}
      oncreatefrom={(sp) => void createFrom(sp)}
      onrename={(n) => void renameFrom(n)}
      {oninspect}
    />
  {/if}
{/if}

<style>
  .rail {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg-rail);
    border-right: 1px solid var(--border);
  }
  .grow {
    flex: 1;
  }

  /* ---- Cabecera ---------------------------------------------------------------- */
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-2) var(--space-1) var(--space-3);
    min-height: 38px;
    color: var(--text-muted);
  }
  .head :global(svg) {
    color: var(--accent);
    flex: none;
  }
  h2 {
    font-size: var(--text-xs);
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--text);
  }

  /* ---- Rama actual -------------------------------------------------------------- */
  .current {
    margin: var(--space-1) var(--space-2) var(--space-2);
    padding: var(--space-2) var(--space-2) var(--space-1) var(--space-2);
    border: 1px solid var(--border-hover);
    border-radius: var(--radius-md);
    background: var(--accent-soft);
    box-shadow: inset 2px 0 0 var(--accent);
    display: grid;
    gap: 2px;
  }
  .cur-top {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
  }
  .cur-dot {
    width: 7px;
    height: 7px;
    flex: none;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-glow);
  }
  .cur-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-sm);
    font-weight: 700;
    color: var(--text);
  }
  .sync {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    flex: none;
    font-size: var(--text-2xs);
    font-weight: 650;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .cur-bottom {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
  }
  .up {
    display: flex;
    align-items: center;
    gap: 3px;
    flex: 1;
    min-width: 0;
    font-size: var(--text-2xs);
    color: var(--text-muted);
  }
  .up :global(svg) {
    flex: none;
    color: var(--text-subtle);
  }
  .upname {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cur-actions {
    display: flex;
    gap: 0;
    flex: none;
  }
  .cur-actions :global(.btn) {
    width: 24px;
    min-width: 24px;
    min-height: 24px;
  }
  .nocur {
    margin: var(--space-1) var(--space-3) var(--space-2);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  /* ---- Filtro ------------------------------------------------------------------ */
  .filter {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    margin: 0 var(--space-2) var(--space-1);
    padding: 0 var(--space-1) 0 var(--space-2);
    min-height: var(--control-h);
    border: 1px solid var(--border-field);
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--bg-surface) 70%, transparent);
    color: var(--text-subtle);
  }
  .filter:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-glow);
  }
  .filter :global(svg) {
    flex: none;
    color: var(--text-subtle);
  }
  .finput {
    flex: 1;
    min-width: 0;
    height: 26px;
    border: none;
    outline: none;
    background: none;
    font-size: var(--text-sm);
    color: var(--text);
  }
  .finput::placeholder {
    color: var(--text-subtle);
  }
  .clear {
    width: 22px;
    min-width: 22px;
    min-height: 22px;
  }

  /* ---- Avisos y formularios ---------------------------------------------------- */
  .notice {
    display: grid;
    gap: 2px;
    margin: 0 var(--space-2) var(--space-2);
    padding: var(--space-2);
    border: 1px solid var(--warning);
    border-radius: var(--radius-md);
    background: var(--warning-soft);
    font-size: var(--text-xs);
  }
  .notice strong {
    font-size: var(--text-xs);
  }
  .notice p {
    margin: 0;
    color: var(--text-muted);
  }
  .nact {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-1);
    margin-top: var(--space-1);
    flex-wrap: wrap;
  }
  .create,
  .rename {
    display: grid;
    gap: 2px;
    margin: 0 var(--space-2) var(--space-2);
    padding: var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-surface);
  }
  .lbl {
    font-size: var(--text-2xs);
    font-weight: 650;
    color: var(--text-muted);
    margin-top: 2px;
  }
  .hint {
    display: flex;
    align-items: center;
    gap: 4px;
    margin: 0;
    min-height: 14px;
    font-size: var(--text-2xs);
    color: var(--text-subtle);
  }
  .hint.bad {
    color: var(--danger);
  }
  .create :global(.field),
  .rename :global(.field) {
    min-height: 26px;
    padding: 0 var(--space-2);
    font-size: var(--text-xs);
  }

  /* ---- Árbol ------------------------------------------------------------------ */
  .tree {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding-bottom: var(--space-2);
  }
  .section {
    padding: 0 var(--space-1);
  }
  .grouphead,
  .subhead {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    width: 100%;
    min-height: 24px;
    padding: 0 var(--space-2);
    border: none;
    background: none;
    color: var(--text-muted);
    text-align: left;
    font-size: var(--text-2xs);
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }
  .grouphead:hover,
  .subhead:hover {
    color: var(--text);
    background: var(--bg-hover);
    border-radius: var(--radius-sm);
  }
  .grouphead .chip {
    margin-left: auto;
    height: 16px;
    text-transform: none;
    letter-spacing: 0;
  }
  .subhead {
    min-height: 22px;
    padding-left: var(--space-4);
    text-transform: none;
    letter-spacing: 0;
    font-weight: 600;
  }
  .subcount {
    margin-left: auto;
    font-variant-numeric: tabular-nums;
    opacity: 0.8;
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    width: 100%;
    min-height: var(--row-h);
    padding: 0 var(--space-2) 0 var(--space-3);
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-muted);
    text-align: left;
    font-size: var(--text-sm);
  }
  .row:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .row :global(svg) {
    flex: none;
    color: var(--text-subtle);
  }
  .row.head {
    background: color-mix(in srgb, var(--accent-soft) 70%, transparent);
    color: var(--text);
  }
  .row.head :global(svg) {
    color: var(--accent);
  }
  .row.sel {
    background: var(--bg-selected);
    color: var(--text);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .row:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row.head .name {
    font-weight: 650;
  }
  .dot {
    width: 6px;
    height: 6px;
    flex: none;
    border-radius: 50%;
    background: var(--accent);
  }
  .none {
    margin: var(--space-3) var(--space-3) var(--space-2);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .none.sm {
    margin: var(--space-1) var(--space-3);
    font-size: var(--text-2xs);
  }
  .sk {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
  }

  /* ---- Inspector de la seleccionada ------------------------------------------- */
  .inspector {
    flex: none;
    display: grid;
    gap: 2px;
    padding: var(--space-2);
    border-top: 1px solid var(--border);
    background: var(--bg-surface);
  }
  .ihead {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
  }
  .ihead :global(svg) {
    flex: none;
    color: var(--accent);
  }
  .iname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-xs);
    font-weight: 650;
  }
  .kind {
    flex: none;
    padding: 0 5px;
    height: 16px;
    line-height: 16px;
    border-radius: var(--pill);
    background: var(--bg-muted);
    color: var(--text-muted);
    font-size: 0.625rem;
    font-weight: 650;
  }
  .kind.current {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .imeta {
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-2xs);
    color: var(--text-subtle);
  }
  .iactions {
    display: grid;
    gap: var(--space-1);
    margin-top: var(--space-1);
  }
  .iactions > :global(.btn.wide) {
    width: 100%;
  }
  .iactions .more {
    display: flex;
    gap: 2px;
  }
  .iactions .more :global(.btn) {
    flex: 1;
    padding: 0 var(--space-1);
  }
  .lbl-sm {
    font-size: var(--text-2xs);
  }
  .danger {
    color: var(--danger);
  }

  /* ---- Contraído --------------------------------------------------------------- */
  .rail.collapsed {
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-2) 0;
    border-right: 1px solid var(--border);
    background: var(--bg-rail);
  }
  .ico {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 30px;
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-muted);
  }
  .ico:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .ico.cur {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .rule {
    width: 20px;
    height: 1px;
    background: var(--border);
  }

  /* Colores forzados: la rama actual y la seleccionada necesitan contorno, no solo fondo. */
  @media (forced-colors: active) {
    .row.head,
    .row.sel,
    .current {
      outline: 2px solid Highlight;
      outline-offset: -2px;
    }
    .row.head :global(svg),
    .ihead :global(svg),
    .cur-dot {
      color: CanvasText;
      background: CanvasText;
    }
  }
</style>
