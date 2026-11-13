<script lang="ts">
  import { onMount } from 'svelte';
  import { t, tn } from '$lib/i18n';
  import { registerHandlers } from '$lib/shortcuts/registry';
  import type { TabState, Panel } from '$lib/stores/tab.svelte';
  import { branchesOf } from '$lib/stores/branches.svelte';
  import { conflictsOf } from '$lib/stores/conflicts.svelte';
  import { prOf } from '$lib/stores/pullRequests.svelte';
  import { stashOf } from '$lib/stores/stash.svelte';
  import Banner from './Banner.svelte';
  import BranchDetail from './BranchDetail.svelte';
  import BranchRail from './BranchRail.svelte';
  import ChangesPanel from './ChangesPanel.svelte';
  import ConflictEditor from './ConflictEditor.svelte';
  import ConflictsPanel from './ConflictsPanel.svelte';
  import CommitBar from './CommitBar.svelte';
  import DiffView from './DiffView.svelte';
  import EmptyState from './EmptyState.svelte';
  import HistoryDetail from './HistoryDetail.svelte';
  import HistoryPanel from './HistoryPanel.svelte';
  import Icon from './Icon.svelte';
  import PrDetail from './PrDetail.svelte';
  import PullRequestsPanel from './PullRequestsPanel.svelte';
  import RepoDialog from './RepoDialog.svelte';
  import Splitter from './Splitter.svelte';
  import StashDetail from './StashDetail.svelte';
  import StashesPanel from './StashesPanel.svelte';

  interface Props {
    tab: TabState;
    onclose: () => void;
    onlocate: () => void;
  }
  let { tab, onclose, onlocate }: Props = $props();

  /**
   * MAQUETA v2: riel de ramas + vista central + detalle.
   *
   *  ┌───────────────────────────────────────────────────────────┐
   *  │ barra superior (TitleBar): repo · rama actual · paleta     │
   *  ├────────────┬───────────────────────────┬──────────────────┤
   *  │ riel       │ vista (Cambios · Historial │ detalle / diff   │
   *  │ de ramas   │ · PR · Stashes · Conflictos)│ cabecera fija    │
   *  ├────────────┴───────────────────────────┴──────────────────┤
   *  │ barra de estado                                           │
   *  └───────────────────────────────────────────────────────────┘
   *
   * El riel sustituye la lista de ramas Y la pestaña «Ramas»: la gestión vive
   * siempre a la vista, el centro no la duplica. «Detalle» de una rama abre
   * `BranchDetail` en el panel derecho (modo temporal, no una vista más).
   */
  const KEY = 'ariadna.layout.v2';
  const MIN_RAIL = 208;
  const MAX_RAIL = 360;
  const MIN_MID = 340;
  const MAX_MID = 820;
  const NARROW = 840;

  function loadLayout(): { rail: number; mid: number; collapsed: boolean } {
    try {
      const o = JSON.parse(localStorage.getItem(KEY) ?? '{}') as { rail?: number; mid?: number; collapsed?: boolean };
      return {
        rail: Math.max(MIN_RAIL, Math.min(MAX_RAIL, o.rail ?? 264)),
        mid: Math.max(MIN_MID, Math.min(MAX_MID, o.mid ?? 560)),
        collapsed: o.collapsed === true
      };
    } catch {
      return { rail: 264, mid: 560, collapsed: false };
    }
  }

  const initial = loadLayout();
  let rail = $state(initial.rail);
  let mid = $state(initial.mid);
  let collapsedPref = $state(initial.collapsed);
  let repoOpen = $state(false);
  let inspect = $state<string | null>(null);
  /** Ancho real del espacio de trabajo (`bind:clientWidth`): decide el modo apilado. */
  let width = $state(0);

  const narrow = $derived(width > 0 && width < NARROW);
  const railCollapsed = $derived(collapsedPref || narrow);
  const railWidth = $derived(railCollapsed ? 52 : rail);

  function persist() {
    try {
      localStorage.setItem(KEY, JSON.stringify({ rail, mid, collapsed: collapsedPref }));
    } catch {
      /* ignorar */
    }
  }

  function toggleRail() {
    if (narrow) return;
    collapsedPref = !collapsedPref;
    persist();
  }

  type PanelDef = {
    id: Panel;
    label: 'panel.changes' | 'panel.history' | 'panel.stashes' | 'panel.conflicts' | 'panel.pullRequests';
    icon: 'file-diff' | 'history' | 'archive' | 'git-merge' | 'git-pull-request';
  };
  // Solo claves: el texto se traduce al pintar para que siga al idioma activo (`t` es reactivo).
  const basePanels: PanelDef[] = [
    { id: 'changes', label: 'panel.changes', icon: 'file-diff' },
    { id: 'history', label: 'panel.history', icon: 'history' },
    { id: 'stashes', label: 'panel.stashes', icon: 'archive' }
  ];
  const integrating = $derived((tab.snapshot?.integration ?? 'none') !== 'none');
  const conflictsActive = $derived(integrating || (tab.snapshot?.conflicts.length ?? 0) > 0);
  const stash = $derived(stashOf(tab));
  /**
   * «Pull requests» solo aparece si el remoto lo admite (`pull_request_support`, sin red). Si se llega
   * por la paleta o el atajo con un remoto sin soporte, la vista se muestra mientras esté activa
   * para explicar por qué no hay PR dentro de la app.
   */
  const prc = $derived(prOf(tab));
  const prVisible = $derived(prc.supported || tab.panel === 'pullRequests');
  const panels = $derived<PanelDef[]>([
    ...basePanels,
    ...(prVisible ? [{ id: 'pullRequests', label: 'panel.pullRequests', icon: 'git-pull-request' } as PanelDef] : []),
    ...(conflictsActive ? [{ id: 'conflicts', label: 'panel.conflicts', icon: 'git-merge' } as PanelDef] : [])
  ]);
  const cc = $derived(conflictsOf(tab));

  const snap = $derived(tab.snapshot);
  const branchName = $derived(snap?.status.isDetached ? t('status.detached') : (snap?.status.branchName ?? tab.info.branch ?? '—'));
  const totalChanges = $derived(tab.sections.staged.length + tab.sections.unstaged.length + tab.sections.untracked.length);

  // El soporte de PR se pregunta a Rust (local) al abrir la pestaña y cuando cambian los remotos.
  const remoteSig = $derived((tab.snapshot?.remotes ?? []).map((r) => `${r.name}=${r.fetchUrl}`).join('|'));
  let lastSig: string | null = null;
  $effect(() => {
    const sig = remoteSig;
    if (tab.hibernated || sig === lastSig) return;
    if (lastSig !== null) prc.release();
    lastSig = sig;
    void prc.loadSupport();
  });

  onMount(() =>
    registerHandlers({
      'rail.toggle': () => (toggleRail(), true),
      'pr.open': () => {
        tab.panel = 'pullRequests';
        return true;
      },
      'stash.open': () => {
        tab.panel = 'stashes';
        return true;
      },
      'stash.push': () => {
        tab.panel = 'changes';
        stashOf(tab).openPush();
        return true;
      }
    })
  );

  // Si la integración termina (o se aborta) con la vista abierta, se vuelve a Cambios.
  $effect(() => {
    if (!conflictsActive && tab.panel === 'conflicts' && !tab.loading) tab.panel = 'changes';
  });

  // Al cambiar de vista, el detalle vuelve a la vista (el de rama era temporal).
  let lastPanel: Panel | null = null;
  $effect(() => {
    if (lastPanel !== null && tab.panel !== lastPanel) inspect = null;
    lastPanel = tab.panel;
  });

  // La lista de stashes no viaja en el snapshot: se relee con cada revisión (un `git stash list`).
  $effect(() => {
    void tab.revision;
    if (!tab.hibernated) void stash.load();
  });

  function panelKey(e: KeyboardEvent, i: number) {
    let n = i;
    if (e.key === 'ArrowRight') n = (i + 1) % panels.length;
    else if (e.key === 'ArrowLeft') n = (i - 1 + panels.length) % panels.length;
    else if (e.key === 'Home') n = 0;
    else if (e.key === 'End') n = panels.length - 1;
    else return;
    e.preventDefault();
    const p = panels[n];
    if (!p) return;
    tab.panel = p.id;
    queueMicrotask(() => document.getElementById(`panel-tab-${p.id}`)?.focus());
  }

  function inspectBranch(name: string): void {
    branchesOf(tab).select(name);
    inspect = name;
  }

  const missing = $derived(tab.sessionError?.code === 'REPO_MISSING' || tab.info.missing);
  const conflictCount = $derived(tab.sections.conflicts.length);
</script>

<div class="workspace" bind:clientWidth={width}>
  <h1 class="sr-only">{tab.info.name} — {branchName}</h1>
  {#if tab.sessionError}
    <div class="banner-slot">
      <Banner
        kind="error"
        title={missing ? t('banner.missingTitle') : t('banner.errorTitle')}
        message={missing ? t('banner.missing', { path: tab.info.path }) : tab.sessionError.message}
        detail={tab.sessionError.detail}
      >
        {#snippet actions()}
          <button type="button" class="btn" onclick={() => tab.load()}>
            <Icon name="refresh-cw" size={14} />{t('banner.retry')}
          </button>
          {#if missing}
            <button type="button" class="btn" onclick={onlocate}>
              <Icon name="folder-open" size={14} />{t('banner.locate')}
            </button>
          {/if}
          <button type="button" class="btn btn-danger" onclick={onclose}>{t('banner.close')}</button>
        {/snippet}
      </Banner>
    </div>
  {/if}

  {#if missing}
    <EmptyState icon="triangle-alert" tone="danger" title={t('banner.missingTitle')} body={t('banner.missingBody')} />
  {:else}
    {#if integrating && tab.panel !== 'conflicts'}
      <div class="banner-slot slim">
        <Banner
          kind="warning"
          title={t(`integration.${snap?.integration ?? 'merging'}`)}
          message={cc.count > 0 ? tn('conflicts.banner.pending', cc.count) : t('conflicts.banner.ready')}
        >
          {#snippet actions()}
            <button type="button" class="btn btn-primary" onclick={() => (tab.panel = 'conflicts')}>
              <Icon name="git-merge" size={14} />{cc.count > 0 ? t('conflicts.banner.open') : t('conflicts.banner.continue')}
            </button>
          {/snippet}
        </Banner>
      </div>
    {/if}

    <div class="shell" class:stacked={narrow} style:--rail={railWidth + 'px'} style:--mid={mid + 'px'}>
      <div class="railslot">
        <BranchRail
          {tab}
          collapsed={railCollapsed}
          ontoggle={toggleRail}
          oninspect={inspectBranch}
          onrepo={() => (repoOpen = true)}
        />
      </div>

      {#if !railCollapsed && !narrow}
        <div class="splitslot">
          <Splitter
            value={rail}
            min={MIN_RAIL}
            max={MAX_RAIL}
            label={t('rail.resize')}
            onchange={(v) => {
              rail = v;
              persist();
            }}
          />
        </div>
      {/if}

      <section class="center" aria-label={t('layout.center')} data-testid="workspace">
        <header class="viewhead">
          <div class="views" role="tablist" aria-label={t('panel.list')} aria-orientation="horizontal" data-testid="views">
            {#each panels as p, i (p.id)}
              <button
                type="button"
                role="tab"
                id="panel-tab-{p.id}"
                data-testid="view-{p.id}"
                aria-selected={tab.panel === p.id}
                aria-controls="panel-{p.id}"
                tabindex={tab.panel === p.id ? 0 : -1}
                class="vtab"
                class:on={tab.panel === p.id}
                onclick={() => (tab.panel = p.id)}
                onkeydown={(e) => panelKey(e, i)}
              >
                <Icon name={p.icon} size={14} />
                <span class="vlabel">{t(p.label)}</span>
                {#if p.id === 'changes' && totalChanges > 0}<span class="chip">{totalChanges}</span>{/if}
                {#if p.id === 'conflicts'}
                  <span class="chip conflict" title={tn('tabs.a11y.conflicts', cc.count)}>
                    <Icon name={cc.count > 0 ? 'triangle-alert' : 'circle-check'} size={11} />{cc.count}
                  </span>
                {/if}
                {#if p.id === 'stashes' && stash.count > 0}<span class="chip" title={tn('stash.chip', stash.count)}>{stash.count}</span>{/if}
                {#if p.id === 'changes' && conflictCount > 0 && !conflictsActive}
                  <span class="chip conflict" title={tn('tabs.a11y.conflicts', conflictCount)}>
                    <Icon name="triangle-alert" size={11} />{conflictCount}
                  </span>
                {/if}
              </button>
            {/each}
          </div>
          <button
            type="button"
            class="btn btn-ghost btn-icon btn-sm refresh"
            title={t('changes.refresh')}
            aria-label={t('changes.refresh')}
            onclick={() => void tab.refresh()}
          >
            <Icon name="refresh-cw" size={14} />
          </button>
        </header>

        <div class="panel" role="tabpanel" id="panel-{tab.panel}" aria-labelledby="panel-tab-{tab.panel}" tabindex="-1">
          {#if tab.panel === 'changes'}
            <div class="changes-stack">
              <div class="grow"><ChangesPanel {tab} /></div>
              <CommitBar {tab} />
            </div>
          {:else if tab.panel === 'history'}
            <HistoryPanel {tab} />
          {:else if tab.panel === 'stashes'}
            <StashesPanel {tab} />
          {:else if tab.panel === 'conflicts'}
            <ConflictsPanel {tab} />
          {:else if tab.panel === 'pullRequests'}
            <PullRequestsPanel {tab} />
          {:else}
            <ChangesPanel {tab} />
          {/if}
        </div>
      </section>

      {#if !narrow}
        <div class="splitslot">
          <Splitter
            value={mid}
            min={MIN_MID}
            max={MAX_MID}
            label={t('layout.resizeMiddle')}
            onchange={(v) => {
              mid = v;
              persist();
            }}
          />
        </div>
      {/if}

      <aside class="detail" aria-label={t('layout.diff')}>
        {#if inspect}
          <div class="inspectbar">
            <Icon name="git-branch" size={14} />
            <span class="ititle">{t('rail.detailTitle')}</span>
            <span class="iname mono">{inspect}</span>
            <button type="button" class="btn btn-sm btn-ghost back" onclick={() => (inspect = null)}>
              <Icon name="x" size={13} />{t('rail.back')}
            </button>
          </div>
          <div class="detailbody"><BranchDetail {tab} /></div>
        {:else if tab.panel === 'history'}
          <HistoryDetail {tab} />
        {:else if tab.panel === 'stashes'}
          <StashDetail {tab} />
        {:else if tab.panel === 'conflicts'}
          <ConflictEditor {tab} />
        {:else if tab.panel === 'pullRequests'}
          <PrDetail {tab} />
        {:else}
          <DiffView {tab} />
        {/if}
      </aside>
    </div>

    <footer class="statusbar">
      <span class="item mono" title={t('status.branch')}>
        <Icon name="git-branch" size={13} />
        {branchName}
      </span>
      {#if snap && snap.status.upstream}
        <span class="item mono">
          {snap.status.upstream}
          {#if snap.status.ahead > 0}<span class="sync"><Icon name="arrow-up" size={11} />{snap.status.ahead}</span>{/if}
          {#if snap.status.behind > 0}<span class="sync"><Icon name="arrow-down" size={11} />{snap.status.behind}</span>{/if}
        </span>
      {/if}
      {#if snap && snap.integration !== 'none'}
        <span class="item integ"><Icon name="git-merge" size={13} />{t(`integration.${snap.integration}`)}</span>
      {/if}
      <span class="grow"></span>
      <span class="item" role="status">
        {#if tab.operations.length > 0}
          <Icon name="loader" size={13} spin />{tab.operations[tab.operations.length - 1]?.label}
        {:else}
          {t('status.ready')}
        {/if}
      </span>
      <span class="item rev mono" title={t('status.revision')}>r{tab.revision}</span>
    </footer>
  {/if}
</div>

{#if repoOpen}
  <RepoDialog {tab} onclose={() => (repoOpen = false)} />
{/if}

<style>
  .workspace {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    min-width: 0;
    background: var(--bg-surface);
  }
  .banner-slot {
    padding: var(--space-3);
    background: var(--bg-app);
    border-bottom: 1px solid var(--border);
  }
  .banner-slot.slim {
    padding: var(--space-2) var(--space-3);
  }

  /* ---- Maqueta: riel | vista | detalle --------------------------------------- */
  .shell {
    flex: 1;
    min-height: 0;
    min-width: 0;
    display: flex;
  }
  .railslot {
    flex: none;
    width: var(--rail);
    min-height: 0;
    display: flex;
  }
  .railslot > :global(*) {
    min-width: 0;
  }
  /* El divisor ocupa 8 px y se solapa ±4 px sobre las columnas. */
  .splitslot {
    flex: none;
    width: 0;
    overflow: visible;
    z-index: 4;
  }
  .center {
    flex: 0 1 var(--mid);
    min-width: min(320px, 100%);
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-surface);
  }
  .detail {
    flex: 1 1 0;
    min-width: min(300px, 100%);
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--border);
    background: var(--bg-surface);
  }
  .detailbody {
    flex: 1;
    min-height: 0;
  }

  /* Modo apilado (≈ 720 px CSS, zoom 200 %): riel de iconos a la izquierda, la vista
     encima del detalle. Se activa desde el script (`narrow`), que también contrae el riel. */
  .shell.stacked {
    display: grid;
    grid-template-columns: var(--rail) minmax(0, 1fr);
    grid-template-rows: minmax(0, 44%) minmax(0, 1fr);
  }
  .shell.stacked .railslot {
    grid-column: 1;
    grid-row: 1 / span 2;
    width: var(--rail);
  }
  .shell.stacked .center {
    grid-column: 2;
    grid-row: 1;
    min-width: 0;
  }
  .shell.stacked .detail {
    grid-column: 2;
    grid-row: 2;
    min-width: 0;
    border-left: none;
    border-top: 1px solid var(--border);
  }
  .shell.stacked .splitslot {
    display: none;
  }

  /* Cabecera del detalle de rama (modo temporal sobre el panel derecho). */
  .inspectbar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 38px;
    padding: 0 var(--space-3);
    border-bottom: 1px solid var(--border);
    background: var(--bg-subtle);
    color: var(--accent);
  }
  .ititle {
    font-size: var(--text-sm);
    font-weight: 650;
    color: var(--text);
  }
  .iname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  /* ---- Cabecera de vistas ------------------------------------------------------- */
  .viewhead {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 44px;
    padding: 0 var(--space-2) 0 var(--space-3);
    border-bottom: 1px solid var(--border);
    background: var(--bg-surface);
    container-type: inline-size;
  }
  .views {
    display: flex;
    align-items: center;
    gap: 2px;
    min-width: 0;
    padding: 3px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .views::-webkit-scrollbar {
    display: none;
  }
  .vtab {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-height: 28px;
    padding: 0 var(--space-2);
    flex: none;
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-muted);
    font-size: var(--text-sm);
    font-weight: 600;
    white-space: nowrap;
    transition:
      background-color var(--dur-fast) var(--ease),
      color var(--dur-fast) var(--ease);
  }
  .vtab:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .vtab.on {
    color: var(--text);
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 34%, transparent);
  }
  .vtab.on :global(svg) {
    color: var(--accent);
  }
  .chip.conflict {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .refresh {
    flex: none;
    margin-left: auto;
  }
  /* Ventana estrecha: el icono decorativo de las pestañas se sacrifica antes que la etiqueta. */
  @container (max-width: 430px) {
    .vtab :global(svg) {
      display: none;
    }
  }
  .panel {
    flex: 1;
    min-height: 0;
  }
  .changes-stack {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .grow {
    flex: 1;
    min-height: 0;
  }

  /* ---- Barra de estado --------------------------------------------------------- */
  .statusbar {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    min-height: 26px;
    padding: 0 var(--space-3);
    border-top: 1px solid var(--border);
    background: var(--bg-app);
    color: var(--text-muted);
    font-size: var(--text-xs);
  }
  .statusbar .grow {
    flex: 1;
  }
  .item {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .sync {
    display: inline-flex;
    align-items: center;
    font-variant-numeric: tabular-nums;
  }
  .integ {
    color: var(--warning);
    font-weight: 600;
  }
  .rev {
    color: var(--text-subtle);
  }

  @media (forced-colors: active) {
    .vtab.on {
      outline: 2px solid Highlight;
      outline-offset: -2px;
    }
    .detail {
      border-left-color: CanvasText;
    }
  }
</style>
