<script lang="ts">
  import { onMount } from 'svelte';
  import { t, tn } from '$lib/i18n';
  import { registerHandlers } from '$lib/shortcuts/registry';
  import type { TabState, Panel } from '$lib/stores/tab.svelte';
  import { conflictsOf } from '$lib/stores/conflicts.svelte';
  import { prOf } from '$lib/stores/pullRequests.svelte';
  import { stashOf } from '$lib/stores/stash.svelte';
  import Banner from './Banner.svelte';
  import BranchDetail from './BranchDetail.svelte';
  import BranchesPanel from './BranchesPanel.svelte';
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
  import Sidebar from './Sidebar.svelte';
  import StashDetail from './StashDetail.svelte';
  import StashesPanel from './StashesPanel.svelte';
  import Splitter from './Splitter.svelte';

  interface Props {
    tab: TabState;
    onclose: () => void;
    onlocate: () => void;
  }
  let { tab, onclose, onlocate }: Props = $props();

  const KEY = 'ariadna.layout.v1';
  const MIN_SIDE = 160;
  const MAX_SIDE = 420;
  const MIN_MID = 300;
  const MAX_MID = 720;

  function loadLayout(): { side: number; mid: number } {
    try {
      const o = JSON.parse(localStorage.getItem(KEY) ?? '{}') as { side?: number; mid?: number };
      return {
        side: Math.max(MIN_SIDE, Math.min(MAX_SIDE, o.side ?? 220)),
        mid: Math.max(MIN_MID, Math.min(MAX_MID, o.mid ?? 400))
      };
    } catch {
      return { side: 220, mid: 400 };
    }
  }

  const initial = loadLayout();
  let side = $state(initial.side);
  let mid = $state(initial.mid);

  function persist() {
    try {
      localStorage.setItem(KEY, JSON.stringify({ side, mid }));
    } catch {
      /* ignorar */
    }
  }

  type PanelDef = {
    id: Panel;
    label: 'panel.changes' | 'panel.history' | 'panel.branches' | 'panel.stashes' | 'panel.conflicts' | 'panel.pullRequests';
    icon: 'file-diff' | 'history' | 'git-branch' | 'archive' | 'git-merge' | 'git-pull-request';
  };
  const basePanels: PanelDef[] = [
    { id: 'changes', label: 'panel.changes', icon: 'file-diff' },
    { id: 'history', label: 'panel.history', icon: 'history' },
    { id: 'branches', label: 'panel.branches', icon: 'git-branch' },
    { id: 'stashes', label: 'panel.stashes', icon: 'archive' }
  ];
  /** El panel «Conflictos» solo existe mientras hay un merge, rebase o cherry-pick en curso. */
  const integrating = $derived((tab.snapshot?.integration ?? 'none') !== 'none');
  /** También hay panel de Conflictos si quedan archivos en conflicto sin integración (p. ej. tras aplicar un stash). */
  const conflictsActive = $derived(integrating || (tab.snapshot?.conflicts.length ?? 0) > 0);
  const stash = $derived(stashOf(tab));
  /**
   * «Pull requests» solo aparece si el remoto lo admite (`pull_request_support`, sin red). Si se llega
   * por la paleta o el atajo con un remoto sin soporte, la pestaña se muestra mientras esté activa
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

  // Si la integración termina (o se aborta) con el panel abierto, se vuelve a Cambios.
  $effect(() => {
    if (!conflictsActive && tab.panel === 'conflicts' && !tab.loading) tab.panel = 'changes';
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

  const missing = $derived(tab.sessionError?.code === 'REPO_MISSING' || tab.info.missing);
  const snap = $derived(tab.snapshot);
  const conflictCount = $derived(tab.sections.conflicts.length);
</script>

<div class="workspace">
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
    <div
      class="columns"
      style:--side="{side}px"
      style:--mid="{mid}px"
    >
      <aside class="col side">
        <Sidebar {tab} snapshot={snap} loading={tab.loading} />
      </aside>
      <Splitter
        value={side}
        min={MIN_SIDE}
        max={MAX_SIDE}
        label={t('layout.resizeSidebar')}
        onchange={(v) => {
          side = v;
          persist();
        }}
      />

      <section class="col middle" aria-label={t('layout.middle')}>
        <div class="panel-tabs" role="tablist" aria-label={t('panel.list')}>
          {#each panels as p, i (p.id)}
            <button
              type="button"
              role="tab"
              id="panel-tab-{p.id}"
              aria-selected={tab.panel === p.id}
              aria-controls="panel-{p.id}"
              tabindex={tab.panel === p.id ? 0 : -1}
              class="ptab"
              class:on={tab.panel === p.id}
              onclick={() => (tab.panel = p.id)}
              onkeydown={(e) => panelKey(e, i)}
            >
              <Icon name={p.icon} size={14} />{t(p.label)}
              {#if p.id === 'changes' && tab.info.changeCount > 0}<span class="chip">{tab.info.changeCount}</span>{/if}
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

        <div
          class="panel"
          role="tabpanel"
          id="panel-{tab.panel}"
          aria-labelledby="panel-tab-{tab.panel}"
          tabindex="-1"
        >
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
            <BranchesPanel {tab} />
          {/if}
        </div>
      </section>

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

      <section class="col right" aria-label={t('layout.diff')}>
        {#if tab.panel === 'history'}
          <HistoryDetail {tab} />
        {:else if tab.panel === 'branches'}
          <BranchDetail {tab} />
        {:else if tab.panel === 'stashes'}
          <StashDetail {tab} />
        {:else if tab.panel === 'conflicts'}
          <ConflictEditor {tab} />
        {:else if tab.panel === 'pullRequests'}
          <PrDetail {tab} />
        {:else}
          <DiffView {tab} />
        {/if}
      </section>
    </div>

    <footer class="statusbar">
      <span class="item mono" title={t('status.branch')}>
        <Icon name="git-branch" size={13} />
        {#if snap?.status.isDetached}{t('status.detached')}{:else}{snap?.status.branchName ?? tab.info.branch ?? '—'}{/if}
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

<style>
  .workspace {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
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
  .columns {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: var(--side) 1px var(--mid) 1px minmax(0, 1fr);
  }
  .col {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .side {
    background: var(--bg-app);
  }
  .middle {
    display: flex;
    flex-direction: column;
    background: var(--bg-surface);
  }
  .panel-tabs {
    display: flex;
    gap: 0;
    padding: var(--space-2) var(--space-2) 0;
    overflow-x: auto;
    scrollbar-width: none;
    border-bottom: 1px solid var(--border);
    container-type: inline-size;
  }
  /* Columna estrecha: sin iconos para que las cuatro pestañas quepan sin recortes ocultos. */
  @container (max-width: 440px) {
    .ptab {
      padding: 0 var(--space-1);
      gap: var(--space-1);
    }
    .ptab :global(svg:first-child) {
      display: none;
    }
  }
  .ptab {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 32px;
    padding: 0 var(--space-2);
    flex: none;
    white-space: nowrap;
    border: none;
    border-bottom: 2px solid transparent;
    background: none;
    color: var(--text-muted);
    font-size: var(--text-sm);
    font-weight: 550;
    margin-bottom: -1px;
  }
  .ptab:hover {
    color: var(--text);
  }
  .ptab.on {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .chip.conflict {
    background: var(--danger-soft);
    color: var(--danger);
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
  .right {
    background: var(--bg-surface);
  }
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

  /* Zoom 200 % / ventanas estrechas (≈ 640 px CSS): la barra lateral se oculta y el diff pasa debajo. */
  @media (max-width: 900px) {
    .columns {
      /* La columna central nunca se come el detalle: como mucho la mitad del ancho. */
      grid-template-columns: 0 0 minmax(0, min(var(--mid), 50%)) 1px minmax(0, 1fr);
    }
    .columns > :global(.splitter:first-of-type) {
      visibility: hidden;
    }
  }
  @media (max-width: 680px) {
    .columns {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: minmax(0, 1fr) minmax(0, 1fr);
    }
    .side,
    .columns > :global(.splitter) {
      display: none;
    }
    .right {
      border-top: 1px solid var(--border);
    }
  }

  /* Colores forzados: la selección no puede depender solo del color de fondo. */
  @media (forced-colors: active) {
    .ptab.on {
      border-bottom-color: Highlight;
    }
  }
</style>
